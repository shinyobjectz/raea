//! raea's flight recorder: every run of an app leaves a session folder to look back at, above all when it crashed.
//!
//! `start("void")` makes `~/Library/Logs/raea/void/<UTC start>-<pid>/` (RAEA_TRACE_DIR moves the root) and from then on:
//! - `stderr.log`: everything any crate prints to stderr, teed (still printed), so no `eprintln!` needs changing
//! - `events.jsonl`: one JSON object a line, written and flushed as it happens (it survives a crash): `log` records,
//!   errors and warnings, what the app reports (`event`), and every performance glitch (`frame`: a frame that missed its
//!   slot or ran over budget, with its breakdown)
//! - `perf.jsonl`: each second, the frames presented and missed, frame work p50/p99/max, and resident memory
//! - `panic.txt`: a panic's message, place, thread and backtrace, and the last events before it
//! - `crash.txt`: a fatal signal (SIGSEGV, SIGBUS, SIGILL, SIGFPE, SIGABRT, SIGTRAP), its raw backtrace, written from the
//!   signal handler with nothing but write(2) and backtrace_symbols_fd
//! - `session.json` at the start (app, pid, args, version, machine) and `end.json` at a clean exit (`end`); a session
//!   without `end.json` ended unclean, and the next start of any raea app copies macOS's crash report for its pid
//!   (~/Library/Logs/DiagnosticReports/*.ips) in beside it as `crash.ips`
//!
//! The last 60 sessions per app are kept. `raea-motion traces` lists them and shows one (`summarize`).

use std::collections::VecDeque;
use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use serde_json::{Value, json};

struct Session {
    dir: PathBuf,
    events: File,
    perf: File,
    /// the last events, for a panic's report
    ring: VecDeque<String>,
    t0: Instant,
    /// this second's frames: work ms each, and how many missed their slot
    sec: Vec<f64>,
    missed: u64,
    sec_at: Instant,
    glitches: u64,
    /// this second's GPU submissions by subsystem: their latencies (ms)
    gpu: std::collections::BTreeMap<String, Vec<f64>>,
    /// this second's GPU time by subsystem and pass (ms, timestamps on the GPU)
    gpu_time: std::collections::BTreeMap<String, Vec<f64>>,
    /// a "source/pass" key written in place to find its entry (ZeroHeap: one was format!-ed per pass per frame)
    key: String,
}

static SESSION: Mutex<Option<Session>> = Mutex::new(None);
/// the crash file's descriptor, opened at start, for the signal handler (which may only write)
static mut CRASH_FD: i32 = -1;
const RING: usize = 400;
const KEEP: usize = 60;

fn root() -> PathBuf {
    std::env::var_os("RAEA_TRACE_DIR").map(PathBuf::from).unwrap_or_else(|| {
        let home = std::env::var_os("HOME").map(PathBuf::from).unwrap_or_else(std::env::temp_dir);
        home.join("Library/Logs/raea")
    })
}

/// UTC as 2026-10-17T23:40:12.345Z (and the folder-safe 20261017-234012).
fn utc(t: SystemTime) -> (String, String) {
    let d = t.duration_since(UNIX_EPOCH).unwrap_or_default();
    let secs = d.as_secs() as i64;
    let (days, rem) = (secs.div_euclid(86400), secs.rem_euclid(86400));
    // civil from days (Howard Hinnant's algorithm)
    let z = days + 719468;
    let era = z.div_euclid(146097);
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + if month <= 2 { 1 } else { 0 };
    let (h, m, s) = (rem / 3600, rem % 3600 / 60, rem % 60);
    (format!("{year:04}-{month:02}-{day:02}T{h:02}:{m:02}:{s:02}.{:03}Z", d.subsec_millis()), format!("{year:04}{month:02}{day:02}-{h:02}{m:02}{s:02}"))
}

/// Resident memory in MB (the kernel's count for this process).
pub fn rss_mb() -> f64 {
    // macOS only: libc binds proc_pidinfo there, not on iOS (whose apps read task_info's footprint themselves)
    #[cfg(target_os = "macos")]
    unsafe {
        let mut info: libc::proc_taskinfo = std::mem::zeroed();
        let size = std::mem::size_of::<libc::proc_taskinfo>() as i32;
        if libc::proc_pidinfo(std::process::id() as i32, libc::PROC_PIDTASKINFO, 0, &mut info as *mut _ as *mut libc::c_void, size) == size {
            return info.pti_resident_size as f64 / 1048576.0;
        }
    }
    0.0
}

/// Physical footprint in MB: the memory this process really holds (dirty and compressed pages, IOKit's), as Activity
/// Monitor's Memory column and Xcode report it. Resident size also counts clean pages an allocator has freed and marked
/// reusable, which the kernel takes back when it needs them.
pub fn footprint_mb() -> f64 {
    #[cfg(target_os = "macos")]
    unsafe {
        let mut info: libc::rusage_info_v2 = std::mem::zeroed();
        if libc::proc_pid_rusage(std::process::id() as i32, libc::RUSAGE_INFO_V2, &mut info as *mut _ as *mut libc::rusage_info_t) == 0 {
            return info.ri_phys_footprint as f64 / 1048576.0;
        }
    }
    0.0
}

/// The folder of this run's session, when one was started.
pub fn dir() -> Option<PathBuf> {
    SESSION.lock().ok()?.as_ref().map(|s| s.dir.clone())
}

/// Start recording this run of `app` (once; later calls do nothing). Returns the session folder.
pub fn start(app: &str) -> Option<PathBuf> {
    let mut g = SESSION.lock().ok()?;
    if let Some(s) = g.as_ref() {
        return Some(s.dir.clone());
    }
    let base = root().join(app);
    std::fs::create_dir_all(&base).ok()?;
    // the sessions before this one that ended unclean get macOS's crash report for their pid
    adopt_crash_reports(&base);
    prune(&base);
    let (iso, stamp) = utc(SystemTime::now());
    let pid = std::process::id();
    let dir = base.join(format!("{stamp}-{pid}"));
    std::fs::create_dir_all(&dir).ok()?;
    let session = json!({
        "app": app, "pid": pid, "started": iso, "args": std::env::args().collect::<Vec<_>>(),
        "version": option_env!("CARGO_PKG_VERSION"), "exe": std::env::current_exe().ok(),
        "os": std::env::consts::OS, "arch": std::env::consts::ARCH,
        "machine": std::process::Command::new("sysctl").args(["-n", "hw.model"]).output().ok().map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string()),
        "cpus": std::thread::available_parallelism().map(|n| n.get()).unwrap_or(0),
        "image": image_line(),
    });
    let _ = std::fs::write(dir.join("session.json"), serde_json::to_string_pretty(&session).unwrap_or_default());
    let open = |name: &str| OpenOptions::new().create(true).append(true).open(dir.join(name)).ok();
    let (events, perf) = (open("events.jsonl")?, open("perf.jsonl")?);
    if let Some(f) = open("crash.txt") {
        use std::os::fd::IntoRawFd;
        unsafe { CRASH_FD = f.into_raw_fd() };
        // the image's UUID and slide written now, so a stripped build's raw backtrace below them can be symbolicated
        // later against its dSYM (atos -o NAME.dSYM -l LOAD ADDR...); left empty when nothing crashes
        CRASH_IMAGE.get_or_init(|| image_line().map(|l| format!("image: {l}\n")).unwrap_or_default());
    }
    *g = Some(Session { dir: dir.clone(), events, perf, ring: VecDeque::new(), t0: Instant::now(), sec: Vec::new(), missed: 0, sec_at: Instant::now(), glitches: 0, gpu: Default::default(), gpu_time: Default::default(), key: String::new() });
    drop(g);
    tee_stderr(&dir);
    install_panic_hook();
    install_signal_handlers();
    let _ = log::set_logger(&LOGGER).map(|_| log::set_max_level(log::LevelFilter::Info));
    event("start", json!({"session": dir}));
    // RAEA_TRACE_SELFTEST=panic|segv: the recorder checked end to end (a panic on a thread, a bad memory access)
    match std::env::var("RAEA_TRACE_SELFTEST").as_deref() {
        Ok("panic") => {
            let _ = std::thread::Builder::new().name("selftest".into()).spawn(|| panic!("RAEA_TRACE_SELFTEST: a panic, on purpose")).map(|h| h.join());
        }
        Ok("segv") => unsafe {
            libc::raise(libc::SIGSEGV);
        },
        _ => {}
    }
    Some(dir)
}

/// A clean exit: `end.json` written (a session without it ended unclean).
pub fn end() {
    let Ok(mut g) = SESSION.lock() else { return };
    if let Some(s) = g.as_mut() {
        flush_second(s, true);
        let (iso, _) = utc(SystemTime::now());
        let _ = std::fs::write(s.dir.join("end.json"), json!({"ended": iso, "seconds": s.t0.elapsed().as_secs_f64(), "glitches": s.glitches}).to_string());
    }
}

fn write_line(s: &mut Session, line: String) {
    let _ = writeln!(s.events, "{line}");
    let _ = s.events.flush();
    s.ring.push_back(line);
    while s.ring.len() > RING {
        s.ring.pop_front();
    }
}

/// Something that happened: `kind` (`open`, `error`, `gpu`, ...) and its details, a line in events.jsonl.
pub fn event(kind: &str, data: Value) {
    let Ok(mut g) = SESSION.lock() else { return };
    let Some(s) = g.as_mut() else { return };
    let mut o = json!({"t": (s.t0.elapsed().as_secs_f64() * 1e3).round() / 1e3, "kind": kind});
    if let (Some(m), Value::Object(d)) = (o.as_object_mut(), data) {
        m.extend(d);
    }
    write_line(s, o.to_string());
}

/// A frame presented: its work (ms), whether it missed its slot, its budget (ms, one refresh), and what it was made of
/// (step, present, ...). Over budget or missed, it's a glitch, written with its breakdown; every frame counts toward the
/// second's summary in perf.jsonl.
pub fn frame(work_ms: f64, missed: bool, budget_ms: f64, breakdown: impl FnOnce() -> Value) {
    let Ok(mut g) = SESSION.lock() else { return };
    let Some(s) = g.as_mut() else { return };
    s.sec.push(work_ms);
    if missed {
        s.missed += 1;
    }
    if missed || work_ms > budget_ms {
        s.glitches += 1;
        let mut o = json!({"t": (s.t0.elapsed().as_secs_f64() * 1e3).round() / 1e3, "kind": "glitch", "work_ms": (work_ms * 100.0).round() / 100.0, "budget_ms": budget_ms, "missed": missed});
        if let (Some(m), Value::Object(d)) = (o.as_object_mut(), breakdown()) {
            m.extend(d);
        }
        write_line(s, o.to_string());
    }
    flush_second(s, false);
}

fn flush_second(s: &mut Session, force: bool) {
    if !force && s.sec_at.elapsed().as_secs_f64() < 1.0 {
        return;
    }
    let mut v = std::mem::take(&mut s.sec);
    v.sort_by(f64::total_cmp);
    let pct = |p: f64| if v.is_empty() { 0.0 } else { (v[((v.len() - 1) as f64 * p).round() as usize] * 100.0).round() / 100.0 };
    let mut line = json!({"t": (s.t0.elapsed().as_secs_f64() * 10.0).round() / 10.0, "frames": v.len(), "missed": s.missed, "work_p50": pct(0.5), "work_p99": pct(0.99), "work_max": pct(1.0), "rss_mb": (rss_mb() * 10.0).round() / 10.0, "footprint_mb": (footprint_mb() * 10.0).round() / 10.0});
    // the GPU's second, by subsystem: submissions and their latency (submit to done) p50, p99, max
    let gpu: serde_json::Map<String, Value> = std::mem::take(&mut s.gpu)
        .into_iter()
        .map(|(k, mut v)| {
            v.sort_by(f64::total_cmp);
            let p = |q: f64| (v[((v.len() - 1) as f64 * q).round() as usize] * 100.0).round() / 100.0;
            (k, json!({"n": v.len(), "p50": p(0.5), "p99": p(0.99), "max": p(1.0)}))
        })
        .collect();
    if !gpu.is_empty() {
        line["gpu"] = Value::Object(gpu);
    }
    let time: serde_json::Map<String, Value> = std::mem::take(&mut s.gpu_time)
        .into_iter()
        .map(|(k, mut v)| {
            v.sort_by(f64::total_cmp);
            let p = |q: f64| (v[((v.len() - 1) as f64 * q).round() as usize] * 1000.0).round() / 1000.0;
            (k, json!({"n": v.len(), "p50": p(0.5), "p99": p(0.99), "max": p(1.0)}))
        })
        .collect();
    if !time.is_empty() {
        line["gpu_time"] = Value::Object(time);
    }
    let _ = writeln!(s.perf, "{line}");
    let _ = s.perf.flush();
    s.missed = 0;
    s.sec_at = Instant::now();
}

/// A GPU submission done: its subsystem (`quads`, `kgpu`, `rdb`, `bevy`, `video`, ...) and its latency (ms, submit to
/// done, as the CPU sees it): counted in the second's summary (perf.jsonl `gpu`). raea-gputrace reports these, and the
/// slow ones and stalls as events.
pub fn gpu(source: &str, ms: f64) {
    let Ok(mut g) = SESSION.lock() else { return };
    let Some(s) = g.as_mut() else { return };
    // found by name, a key made only the first time (ZeroHeap: one a submission)
    match s.gpu.get_mut(source) {
        Some(v) => v.push(ms),
        None => {
            s.gpu.insert(source.to_string(), vec![ms]);
        }
    }
    flush_second(s, false);
}

/// A frame's GPU time, measured on the GPU (timestamps): its subsystem, each pass's ms, and the budget (one refresh).
/// Each pass counts in the second's summary (perf.jsonl `gpu_time`, as "source/pass"); a frame whose passes add up past
/// the budget is a `gpu_busy` event with them all. raea-gputrace's PassTimer reports these.
pub fn gpu_passes(source: &str, passes: &[(&str, f64)], budget_ms: f64) {
    let Ok(mut g) = SESSION.lock() else { return };
    let Some(s) = g.as_mut() else { return };
    let mut total = 0.0;
    let add = |s: &mut Session, name: &str, ms: f64| {
        use std::fmt::Write as _;
        s.key.clear();
        let _ = write!(s.key, "{source}/{name}");
        match s.gpu_time.get_mut(s.key.as_str()) {
            Some(v) => v.push(ms),
            None => {
                s.gpu_time.insert(s.key.clone(), vec![ms]);
            }
        }
    };
    for (name, ms) in passes {
        total += ms;
        add(s, name, *ms);
    }
    add(s, "total", total);
    if total > budget_ms {
        s.glitches += 1;
        let line = json!({"t": (s.t0.elapsed().as_secs_f64() * 1e3).round() / 1e3, "kind": "gpu_busy", "source": source, "ms": (total * 1000.0).round() / 1000.0, "budget_ms": budget_ms, "passes": passes.iter().map(|(n, m)| json!([n, (m * 1000.0).round() / 1000.0])).collect::<Vec<_>>()});
        write_line(s, line.to_string());
    }
    flush_second(s, false);
}

// ---- log records

struct Logger;
static LOGGER: Logger = Logger;

impl log::Log for Logger {
    fn enabled(&self, m: &log::Metadata) -> bool {
        m.level() <= log::Level::Info
    }
    fn log(&self, r: &log::Record) {
        if !self.enabled(r.metadata()) {
            return;
        }
        // printed as before (stderr, itself kept in stderr.log), and a line in events.jsonl
        eprintln!("[{} {}] {}", r.level(), r.target(), r.args());
        event("log", json!({"level": r.level().as_str(), "target": r.target(), "message": r.args().to_string()}));
    }
    fn flush(&self) {}
}

// ---- stderr, teed

fn tee_stderr(dir: &Path) {
    #[cfg(unix)]
    unsafe {
        let mut fds = [0i32; 2];
        if libc::pipe(fds.as_mut_ptr()) != 0 {
            return;
        }
        let original = libc::dup(2);
        if original < 0 || libc::dup2(fds[1], 2) < 0 {
            return;
        }
        libc::close(fds[1]);
        let Ok(mut log) = OpenOptions::new().create(true).append(true).open(dir.join("stderr.log")) else { return };
        let read = fds[0];
        let _ = std::thread::Builder::new().name("raea trace stderr".into()).spawn(move || {
            let mut buf = [0u8; 8192];
            loop {
                let n = libc::read(read, buf.as_mut_ptr() as *mut libc::c_void, buf.len());
                if n <= 0 {
                    break;
                }
                let b = &buf[..n as usize];
                libc::write(original, b.as_ptr() as *const libc::c_void, b.len());
                let _ = log.write_all(b);
                let _ = log.flush();
            }
        });
    }
}

// ---- the image, for symbolicating a stripped build's backtraces

/// The crash file's image line, made at start (the signal handler only reads it).
static CRASH_IMAGE: std::sync::OnceLock<String> = std::sync::OnceLock::new();

/// The main executable's Mach-O UUID, load address and ASLR slide: what atos needs to turn a stripped build's backtrace
/// addresses into names against the dSYM kept for that UUID (`atos -o NAME.dSYM -l LOAD ADDR...`).
#[cfg(any(target_os = "macos", target_os = "ios"))]
fn image_line() -> Option<String> {
    unsafe extern "C" {
        fn _dyld_get_image_header(i: u32) -> *const u8;
        fn _dyld_get_image_vmaddr_slide(i: u32) -> isize;
    }
    unsafe {
        let h = _dyld_get_image_header(0);
        if h.is_null() {
            return None;
        }
        let slide = _dyld_get_image_vmaddr_slide(0);
        // mach_header_64: magic, cputype, cpusubtype, filetype, ncmds, sizeofcmds, flags, reserved (32 bytes)
        let ncmds = (h.add(16) as *const u32).read_unaligned();
        let mut p = h.add(32);
        let mut uuid = None;
        for _ in 0..ncmds {
            let cmd = (p as *const u32).read_unaligned();
            let size = (p.add(4) as *const u32).read_unaligned() as usize;
            // LC_UUID
            if cmd == 0x1b {
                let b = std::slice::from_raw_parts(p.add(8), 16);
                uuid = Some(format!(
                    "{:02X}{:02X}{:02X}{:02X}-{:02X}{:02X}-{:02X}{:02X}-{:02X}{:02X}-{:02X}{:02X}{:02X}{:02X}{:02X}{:02X}",
                    b[0], b[1], b[2], b[3], b[4], b[5], b[6], b[7], b[8], b[9], b[10], b[11], b[12], b[13], b[14], b[15]
                ));
                break;
            }
            if size == 0 {
                break;
            }
            p = p.add(size);
        }
        Some(format!("uuid {} load 0x{:x} slide 0x{:x}", uuid.unwrap_or_else(|| "?".into()), h as usize, slide))
    }
}

#[cfg(not(any(target_os = "macos", target_os = "ios")))]
fn image_line() -> Option<String> {
    None
}

// ---- panics

fn install_panic_hook() {
    let prev = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let bt = std::backtrace::Backtrace::force_capture();
        let thread = std::thread::current().name().unwrap_or("unnamed").to_string();
        let place = info.location().map(|l| format!("{}:{}:{}", l.file(), l.line(), l.column())).unwrap_or_default();
        let msg = info.payload().downcast_ref::<&str>().map(|s| s.to_string()).or_else(|| info.payload().downcast_ref::<String>().cloned()).unwrap_or_else(|| "(not a string)".into());
        // the session's lock may be held by the panicking thread: try, never wait
        if let Ok(mut g) = SESSION.try_lock() {
            if let Some(s) = g.as_mut() {
                let (iso, _) = utc(SystemTime::now());
                let recent: Vec<String> = s.ring.iter().cloned().collect();
                let text = format!("panic at {iso} ({:.3} s into the run)\nthread: {thread}\nat: {place}\nmessage: {msg}\nimage: {}\n\nbacktrace:\n{bt}\n\nthe last {} events:\n{}\n", s.t0.elapsed().as_secs_f64(), image_line().unwrap_or_default(), recent.len(), recent.join("\n"));
                let _ = std::fs::write(s.dir.join("panic.txt"), text);
                let line = json!({"t": s.t0.elapsed().as_secs_f64(), "kind": "panic", "thread": thread, "at": place, "message": msg});
                write_line(s, line.to_string());
            }
        }
        prev(info);
    }));
}

// ---- fatal signals

#[cfg(unix)]
extern "C" fn on_signal(sig: i32) {
    // async-signal-safe only: write(2) and backtrace(3)/backtrace_symbols_fd(3), into the file opened at start
    unsafe {
        let fd = CRASH_FD;
        if fd >= 0 {
            let name: &[u8] = match sig {
                libc::SIGSEGV => b"fatal signal SIGSEGV (a bad memory access)\n",
                libc::SIGBUS => b"fatal signal SIGBUS (a bad memory access)\n",
                libc::SIGILL => b"fatal signal SIGILL (an illegal instruction)\n",
                libc::SIGFPE => b"fatal signal SIGFPE (an arithmetic fault)\n",
                libc::SIGABRT => b"fatal signal SIGABRT (abort)\n",
                libc::SIGTRAP => b"fatal signal SIGTRAP (a trap: a Swift/Objective-C runtime check or a breakpoint)\n",
                _ => b"fatal signal\n",
            };
            libc::write(fd, name.as_ptr() as *const libc::c_void, name.len());
            if let Some(img) = CRASH_IMAGE.get() {
                libc::write(fd, img.as_ptr() as *const libc::c_void, img.len());
            }
            let head = b"backtrace (raw; macOS's crash report, copied in as crash.ips at the next start, has it symbolicated):\n";
            libc::write(fd, head.as_ptr() as *const libc::c_void, head.len());
            let mut frames = [std::ptr::null_mut::<libc::c_void>(); 128];
            let n = backtrace(frames.as_mut_ptr(), 128);
            backtrace_symbols_fd(frames.as_ptr(), n, fd);
        }
        // the default action (a core and macOS's crash report), as if this handler weren't here
        libc::signal(sig, libc::SIG_DFL);
        libc::raise(sig);
    }
}

#[cfg(unix)]
unsafe extern "C" {
    fn backtrace(buf: *mut *mut libc::c_void, size: i32) -> i32;
    fn backtrace_symbols_fd(buf: *const *mut libc::c_void, size: i32, fd: i32);
}

fn install_signal_handlers() {
    #[cfg(unix)]
    unsafe {
        for sig in [libc::SIGSEGV, libc::SIGBUS, libc::SIGILL, libc::SIGFPE, libc::SIGABRT, libc::SIGTRAP] {
            libc::signal(sig, on_signal as extern "C" fn(i32) as libc::sighandler_t);
        }
    }
}

// ---- earlier sessions

/// Sessions under `base` that ended unclean (no end.json, their process gone) get macOS's crash report for their pid.
fn adopt_crash_reports(base: &Path) {
    let reports = std::env::var_os("HOME").map(|h| PathBuf::from(h).join("Library/Logs/DiagnosticReports"));
    let Ok(entries) = std::fs::read_dir(base) else { return };
    for e in entries.flatten() {
        let d = e.path();
        if d.join("end.json").exists() || d.join("crash.ips").exists() || d.join("unclean.json").exists() {
            continue;
        }
        let Some(pid) = d.file_name().and_then(|n| n.to_str()).and_then(|n| n.rsplit('-').next()).and_then(|p| p.parse::<i32>().ok()) else { continue };
        if unsafe { libc::kill(pid, 0) } == 0 {
            continue; // still running (another instance)
        }
        let mut found = None;
        let since = std::fs::metadata(&d).and_then(|m| m.created().or(m.modified())).ok();
        if let Some(dir) = &reports {
            if let Ok(list) = std::fs::read_dir(dir) {
                for r in list.flatten() {
                    let p = r.path();
                    if p.extension().and_then(|x| x.to_str()) != Some("ips") {
                        continue;
                    }
                    // only reports written after the session began
                    let at = r.metadata().and_then(|m| m.modified()).ok();
                    if let (Some(at), Some(since)) = (at, since) {
                        if at < since {
                            continue;
                        }
                    }
                    // the report's header line and body name the pid ("pid" : 1234)
                    if let Ok(text) = std::fs::read_to_string(&p) {
                        let needle = format!("\"pid\" : {pid},");
                        if text.contains(&needle) && text.len() < 64 << 20 {
                            found = Some(p);
                            break;
                        }
                    }
                }
            }
        }
        match found {
            Some(p) => {
                let _ = std::fs::copy(&p, d.join("crash.ips"));
            }
            None => {
                let _ = std::fs::write(d.join("unclean.json"), json!({"note": "ended without end.json and no macOS crash report names its pid: killed (SIGKILL, memguard, a force quit) or the report wasn't written yet"}).to_string());
            }
        }
    }
}

/// Only the last KEEP sessions stay.
fn prune(base: &Path) {
    let Ok(entries) = std::fs::read_dir(base) else { return };
    let mut dirs: Vec<PathBuf> = entries.flatten().map(|e| e.path()).filter(|p| p.is_dir()).collect();
    dirs.sort();
    while dirs.len() >= KEEP {
        let d = dirs.remove(0);
        let _ = std::fs::remove_dir_all(d);
    }
}

// ---- looking back

/// Every app's sessions, newest first: (app, folder).
pub fn sessions() -> Vec<(String, PathBuf)> {
    let mut out = Vec::new();
    if let Ok(apps) = std::fs::read_dir(root()) {
        for a in apps.flatten() {
            if let Ok(list) = std::fs::read_dir(a.path()) {
                for s in list.flatten() {
                    if s.path().join("session.json").exists() {
                        out.push((a.file_name().to_string_lossy().into_owned(), s.path()));
                    }
                }
            }
        }
    }
    out.sort_by(|a, b| b.1.file_name().cmp(&a.1.file_name()));
    out
}

/// How a session ended: ok, panicked, crashed (a signal or macOS's report), unclean (killed), or running.
pub fn status(d: &Path) -> &'static str {
    if d.join("panic.txt").exists() {
        "panicked"
    } else if std::fs::metadata(d.join("crash.txt")).map(|m| m.len() > 0).unwrap_or(false) || d.join("crash.ips").exists() {
        "crashed"
    } else if d.join("end.json").exists() {
        "ok"
    } else if d.join("unclean.json").exists() {
        "unclean"
    } else {
        "running or unclean"
    }
}

/// A session told as text: how it ended and why, its glitches (the worst, and when), its errors, its memory.
pub fn summarize(d: &Path) -> String {
    let mut out = String::new();
    let read = |n: &str| std::fs::read_to_string(d.join(n)).unwrap_or_default();
    let session: Value = serde_json::from_str(&read("session.json")).unwrap_or(Value::Null);
    out += &format!("{} ({}), pid {}, started {}: {}\n", session["app"].as_str().unwrap_or("?"), d.display(), session["pid"], session["started"].as_str().unwrap_or("?"), status(d));
    if let Ok(e) = serde_json::from_str::<Value>(&read("end.json")) {
        out += &format!("ran {:.1} s\n", e["seconds"].as_f64().unwrap_or(0.0));
    }
    let panic = read("panic.txt");
    if !panic.is_empty() {
        out += "\n== panic\n";
        out += &panic.lines().take(40).collect::<Vec<_>>().join("\n");
        out += "\n";
    }
    let crash = read("crash.txt");
    if !crash.is_empty() {
        out += "\n== crash (signal)\n";
        out += &crash.lines().take(30).collect::<Vec<_>>().join("\n");
        out += "\n";
    }
    let ips = read("crash.ips");
    if !ips.is_empty() {
        // the report's body: the exception and the crashed thread's first frames
        out += "\n== macOS crash report (crash.ips)\n";
        if let Some(body) = ips.split_once('\n').and_then(|(_, b)| serde_json::from_str::<Value>(b).ok()) {
            out += &format!("exception: {}\n", body["exception"]);
            if let Some(term) = body.get("termination") {
                out += &format!("termination: {term}\n");
            }
            let images = body["usedImages"].as_array().cloned().unwrap_or_default();
            if let Some(threads) = body["threads"].as_array() {
                if let Some(t) = threads.iter().find(|t| t["triggered"].as_bool() == Some(true)) {
                    out += &format!("crashed thread: {}\n", t["name"].as_str().or(t["queue"].as_str()).unwrap_or("?"));
                    for f in t["frames"].as_array().into_iter().flatten().take(16) {
                        let img = f["imageIndex"].as_u64().and_then(|i| images.get(i as usize)).and_then(|im| im["name"].as_str()).unwrap_or("?");
                        out += &format!("  {img}  {}\n", f["symbol"].as_str().unwrap_or("?"));
                    }
                }
            }
        }
    }
    let events: Vec<Value> = read("events.jsonl").lines().filter_map(|l| serde_json::from_str(l).ok()).collect();
    let glitches: Vec<&Value> = events.iter().filter(|e| e["kind"] == "glitch").collect();
    out += &format!("\n== glitches: {}\n", glitches.len());
    let mut worst = glitches.clone();
    worst.sort_by(|a, b| b["work_ms"].as_f64().unwrap_or(0.0).total_cmp(&a["work_ms"].as_f64().unwrap_or(0.0)));
    for g in worst.iter().take(10) {
        out += &format!("  at {:>8.2} s  {:>7.2} ms{}  {}\n", g["t"].as_f64().unwrap_or(0.0), g["work_ms"].as_f64().unwrap_or(0.0), if g["missed"] == true { " missed" } else { "" }, g.as_object().map(|o| o.iter().filter(|(k, _)| !["t", "kind", "work_ms", "missed", "budget_ms"].contains(&k.as_str())).map(|(k, v)| format!("{k}={v}")).collect::<Vec<_>>().join(" ")).unwrap_or_default());
    }
    // the GPU and video, by the subsystem behind each (raea-gputrace classifies GPU errors by the labels they name)
    let gpu_kinds = ["gpu_error", "gpu_lost", "gpu_stall", "gpu_slow", "video_error", "video_slow"];
    let gpu_events: Vec<&Value> = events.iter().filter(|e| e["kind"].as_str().is_some_and(|k| gpu_kinds.contains(&k))).collect();
    out += &format!("\n== GPU and video: {} events\n", gpu_events.len());
    let mut by: std::collections::BTreeMap<(String, String), (usize, f64)> = Default::default();
    for e in &gpu_events {
        let k = (e["source"].as_str().unwrap_or("?").to_string(), e["kind"].as_str().unwrap_or("?").to_string());
        let ms = e["ms"].as_f64().unwrap_or(0.0);
        let x = by.entry(k).or_insert((0, 0.0));
        x.0 += 1;
        x.1 = x.1.max(ms);
    }
    for ((src, kind), (n, worst)) in &by {
        out += &format!("  {src:<8} {kind:<12} {n:>6}{}\n", if *worst > 0.0 { format!("   worst {worst:.1} ms") } else { String::new() });
    }
    for e in gpu_events.iter().filter(|e| e["kind"] != "gpu_slow" && e["kind"] != "video_slow").take(10) {
        out += &format!("  {e}\n");
    }
    // each subsystem's GPU latency over the run (the per-second summaries), and Bevy's slowest passes
    let mut lat: std::collections::BTreeMap<String, (u64, f64, f64)> = Default::default();
    for p in read("perf.jsonl").lines().filter_map(|l| serde_json::from_str::<Value>(l).ok()) {
        if let Some(g) = p["gpu"].as_object() {
            for (src, v) in g {
                let x = lat.entry(src.clone()).or_insert((0, 0.0, 0.0));
                x.0 += v["n"].as_u64().unwrap_or(0);
                x.1 = x.1.max(v["p99"].as_f64().unwrap_or(0.0));
                x.2 = x.2.max(v["max"].as_f64().unwrap_or(0.0));
            }
        }
    }
    for (src, (n, p99, max)) in &lat {
        out += &format!("  {src:<8} {n} submissions, worst second's p99 {p99:.2} ms, max {max:.2} ms (submit to done)\n");
    }
    let mut passes: std::collections::BTreeMap<String, f64> = Default::default();
    for e in events.iter().filter(|e| e["kind"] == "bevy_passes") {
        for (k, v) in e["ms"].as_object().into_iter().flatten() {
            let x = passes.entry(k.clone()).or_insert(0.0);
            *x = x.max(v.as_f64().unwrap_or(0.0));
        }
    }
    let mut passes: Vec<_> = passes.into_iter().collect();
    passes.sort_by(|a, b| b.1.total_cmp(&a.1));
    if !passes.is_empty() {
        out += "  bevy's slowest passes (worst second): ";
        out += &passes.iter().take(6).map(|(k, v)| format!("{k} {v:.2} ms")).collect::<Vec<_>>().join(", ");
        out += "\n";
    }
    let errors: Vec<&Value> = events.iter().filter(|e| e["kind"] == "error" || e["level"] == "ERROR" || e["level"] == "WARN" || e["kind"] == "panic").collect();
    out += &format!("\n== errors and warnings: {}\n", errors.len());
    for e in errors.iter().rev().take(15).rev() {
        out += &format!("  {e}\n");
    }
    let perf: Vec<Value> = read("perf.jsonl").lines().filter_map(|l| serde_json::from_str(l).ok()).collect();
    if !perf.is_empty() {
        let rss: Vec<f64> = perf.iter().filter_map(|p| p["rss_mb"].as_f64()).collect();
        let missed: u64 = perf.iter().filter_map(|p| p["missed"].as_u64()).sum();
        let frames: u64 = perf.iter().filter_map(|p| p["frames"].as_u64()).sum();
        out += &format!("\n== perf: {frames} frames, {missed} missed; memory {:.0} MB at start, {:.0} MB max, {:.0} MB at the end\n", rss.first().unwrap_or(&0.0), rss.iter().cloned().fold(0.0, f64::max), rss.last().unwrap_or(&0.0));
    }
    let stderr = read("stderr.log");
    let tail: Vec<&str> = stderr.lines().rev().take(12).collect();
    if !tail.is_empty() {
        out += "\n== the end of stderr.log\n";
        for l in tail.into_iter().rev() {
            out += &format!("  {l}\n");
        }
    }
    out
}
