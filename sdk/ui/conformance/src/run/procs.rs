//! The processes a case runs: their group, signals, sweeps, ports and fixtures, stopped on drop.

use super::*;

pub(crate) static PGID: AtomicI32 = AtomicI32::new(0);
/// the feed's host beside raea-page-host (launch.rs feed_host)
pub(crate) static HOST_PGID: AtomicI32 = AtomicI32::new(0);

extern "C" fn on_signal(sig: libc::c_int) {
    unsafe {
        for g in [PGID.load(Ordering::SeqCst), HOST_PGID.load(Ordering::SeqCst)].into_iter().filter(|g| *g > 0) {
            libc::killpg(g, libc::SIGKILL);
        }
        libc::_exit(128 + sig);
    }
}

pub fn signals() {
    unsafe {
        for s in [libc::SIGINT, libc::SIGTERM, libc::SIGHUP] {
            libc::signal(s, on_signal as *const () as libc::sighandler_t);
        }
    }
}

/// $TMPDIR/conformance: runs/<pid>/ (each case's world), fixture-<hash>/ (the generated workspace), out-<stamp>/.
pub(crate) fn base() -> PathBuf {
    std::env::temp_dir().join("conformance")
}

/// Run folders left by runs that were killed (their pid is gone).
pub(crate) fn sweep() {
    let Ok(rd) = std::fs::read_dir(base().join("runs")) else { return };
    for e in rd.flatten() {
        let pid: i32 = e.file_name().to_string_lossy().parse().unwrap_or(0);
        if pid > 0 && pid as u32 != std::process::id() && unsafe { libc::kill(pid, 0) } != 0 {
            let _ = std::fs::remove_dir_all(e.path());
        }
    }
}

pub(crate) struct Proc {
    pub(crate) child: Child,
    /// where the signal handler finds its process group (PGID, HOST_PGID), cleared once it's stopped
    pub(crate) group: &'static AtomicI32,
}

impl Drop for Proc {
    fn drop(&mut self) {
        let g = self.child.id() as i32;
        unsafe { libc::killpg(g, libc::SIGTERM) };
        let t = Instant::now();
        while t.elapsed() < Duration::from_secs(3) {
            if matches!(self.child.try_wait(), Ok(Some(_))) {
                break;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        unsafe { libc::killpg(g, libc::SIGKILL) };
        let _ = self.child.wait();
        self.group.store(0, Ordering::SeqCst);
    }
}

/// One case's world: a launched substrate (or an attached one) and the feed's host beside it, its folder, its log.
pub(crate) struct World {
    pub(crate) proc: Option<Proc>,
    pub(crate) host: Option<Proc>,
    pub(crate) dir: Option<PathBuf>,
    pub(crate) workspace: PathBuf,
    pub(crate) log: Option<PathBuf>,
    pub(crate) mcp: Mcp,
}

impl Drop for World {
    fn drop(&mut self) {
        self.proc.take();
        self.host.take();
        if let Some(d) = self.dir.take() {
            let _ = std::fs::remove_dir_all(d);
        }
    }
}

pub(crate) fn free_port() -> Result<u16, String> {
    let l = std::net::TcpListener::bind("127.0.0.1:0").map_err(|e| format!("no free port: {e}"))?;
    Ok(l.local_addr().map_err(|e| e.to_string())?.port())
}

/// FNV-1a over the fixture's files (names and bytes): a new fixture makes a new cache.
pub(crate) fn fixture_hash(dir: &Path) -> u64 {
    let mut files = vec![];
    if let Ok(rd) = std::fs::read_dir(dir) {
        files = rd.flatten().map(|e| e.path()).filter(|p| p.is_file()).collect();
    }
    files.sort();
    let mut h: u64 = 0xcbf29ce484222325;
    for f in files {
        for b in f.file_name().unwrap_or_default().as_encoded_bytes().iter().chain(std::fs::read(&f).unwrap_or_default().iter()) {
            h ^= *b as u64;
            h = h.wrapping_mul(0x100000001b3);
        }
    }
    // make.sh links each file of the bench Pong one by one (its contents follow the repo's): a file added there is a
    // new fixture (10-28: a tweaked pong-v2.tsx couldn't import the steps.ts its game had gained)
    let mut pong: Vec<_> = std::fs::read_dir(dir.join("../../../react/bench/games/pong")).map(|rd| rd.flatten().map(|e| e.file_name()).collect()).unwrap_or_default();
    pong.sort();
    for n in pong {
        for b in n.as_encoded_bytes() {
            h ^= *b as u64;
            h = h.wrapping_mul(0x100000001b3);
        }
    }
    h
}

/// The generated fixture workspace (fixture/make.sh), made once per fixture version.
pub(crate) fn fixture(dir: &Path) -> Result<PathBuf, String> {
    let cache = base().join(format!("fixture-{:016x}", fixture_hash(dir)));
    if cache.join(".done").exists() {
        return Ok(cache);
    }
    let tmp = base().join(format!("fixture-tmp-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir_all(&tmp).map_err(|e| e.to_string())?;
    let out = Command::new("bash").arg(dir.join("make.sh")).arg(&tmp).output().map_err(|e| format!("fixture/make.sh: {e}"))?;
    if !out.status.success() {
        return Err(format!("fixture/make.sh failed: {}", String::from_utf8_lossy(&out.stderr).trim()));
    }
    std::fs::write(tmp.join(".done"), "").map_err(|e| e.to_string())?;
    let _ = std::fs::remove_dir_all(&cache);
    std::fs::rename(&tmp, &cache).map_err(|e| format!("fixture cache: {e}"))?;
    Ok(cache)
}

/// The workspace's files dated as fixture/times.txt says (PATH SECONDS-AGO), so the feed's order is the same every run.
pub(crate) fn date_files(ws: &Path, times: &Path) {
    let now = SystemTime::now();
    for line in std::fs::read_to_string(times).unwrap_or_default().lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let Some((p, ago)) = line.rsplit_once(char::is_whitespace) else { continue };
        let Ok(ago) = ago.parse::<u64>() else { continue };
        if let Ok(f) = std::fs::File::options().write(true).open(ws.join(p.trim())) {
            let _ = f.set_modified(now - Duration::from_secs(ago));
        }
    }
}
