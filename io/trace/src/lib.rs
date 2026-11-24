//! raea's flight recorder: every run of an app leaves a session folder to look back at, above all when it crashed.
//!
//! `start("raea")` makes `~/Library/Logs/raea/raea/<UTC start>-<pid>/` (RAEA_TRACE_DIR moves the root) and from then on:
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
//! The last 60 sessions per app are kept. `raea traces` lists them and shows one (`summarize`).

use std::collections::VecDeque;
use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use serde_json::{Value, json};

mod crash;
mod look;
mod props;
mod stats;

use crash::*;
pub use look::{sessions, status, summarize};
pub use props::overrides;
pub use stats::{choose as choose_stats, chosen as chosen_stats, count, pass_ms, stat, stat_value, stats, table as stats_table};

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
#[cfg(unix)]
static mut CRASH_FD: i32 = -1;
const RING: usize = 400;
const KEEP: usize = 60;

fn root() -> PathBuf {
    std::env::var_os("RAEA_TRACE_DIR").map(PathBuf::from).or_else(raea_io_home::logs).unwrap_or_else(|| std::env::temp_dir().join("raea-logs"))
}

/// UTC as 2026-10-17T23:40:12.345Z (and the folder-safe 20261017-234012).
pub fn utc(t: SystemTime) -> (String, String) {
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

/// The memory the allocator holds freed handed back to the system now rather than when it gets round to it: macOS's
/// malloc kept a film's freed sound buffers (its master's copies of 7 minutes of sound) counted in its footprint until
/// well into the checks after it, 1.4 GB over its last frame's (SG4a). glibc's malloc_trim on Linux; nothing elsewhere.
pub fn give_back() {
    #[cfg(target_vendor = "apple")]
    unsafe {
        malloc_zone_pressure_relief(std::ptr::null_mut(), 0);
    }
    #[cfg(all(target_os = "linux", target_env = "gnu"))]
    unsafe {
        libc::malloc_trim(0);
    }
}

#[cfg(target_vendor = "apple")]
unsafe extern "C" {
    /// every zone's free memory returned to the system (`goal` 0: all it can), libmalloc's
    fn malloc_zone_pressure_relief(zone: *mut libc::c_void, goal: usize) -> usize;
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
    // for the fatal-signal handler, which Windows doesn't have
    #[cfg(unix)]
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

/// A clean exit: `end.json` written (a session without it ended unclean), and the stats the run chose (stats.rs).
pub fn end() {
    let Ok(mut g) = SESSION.lock() else { return };
    let dir = g.as_ref().map(|s| s.dir.clone());
    if let Some(s) = g.as_mut() {
        flush_second(s, true);
        let (iso, _) = utc(SystemTime::now());
        let _ = std::fs::write(s.dir.join("end.json"), json!({"ended": iso, "seconds": s.t0.elapsed().as_secs_f64(), "glitches": s.glitches}).to_string());
    }
    // printed with the session unlocked (stderr's tee writes into it)
    drop(g);
    stats::finish(dir.as_deref());
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

/// A GPU submission done: its subsystem (`quads`, `kgpu`, `raea-render`, `bevy`, `video`, ...) and its latency (ms, submit to
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
        stats::pass_ms(&s.key, ms);
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
        // a line in events.jsonl, and on stderr (itself kept in stderr.log) only a warning or an error a render's reader
        // acts on: the `log` crate is third-party crates' (Bevy's Ctrl+C handler at info, fontdb's skipped faces: a
        // system font it can't parse, or a missing @font-face file raea_sys_page_net already names), which printed
        // on every render and buried what mattered (#34)
        // bevy_time's "did not receive the time from the render world" too: raea drives Time by frame with the render
        // world in step, so it says nothing a reader acts on
        if r.level() <= log::Level::Warn && !r.target().starts_with("fontdb") && !r.target().starts_with("bevy_time") {
            eprintln!("[{} {}] {}", r.level(), r.target(), r.args());
        }
        event("log", json!({"level": r.level().as_str(), "target": r.target(), "message": r.args().to_string()}));
    }
    fn flush(&self) {}
}
