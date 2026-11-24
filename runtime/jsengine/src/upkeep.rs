//! A page's JS engine looked after between frames, the same on every engine.
//!
//! - Its free pages go back to the system every few seconds (RAEA_JSC_RELEASE_S, 5; 0 never), as raea-runtime-react's
//!   JS thread already gives them back. A page's JS didn't: JavaScriptCore keeps what it frees resident until asked,
//!   and the 0.1.6 app's window showed ~300 MB of its JS memory reclaimable and still held (11-16).
//! - With RAEA_INSPECT on, its heap is logged: every RAEA_JS_HEAP_S seconds (30; 0 never) a full collection, then one
//!   JSON line in ~/.raea/logs/js-heap-<pid>-<n>.jsonl (the process's nth engine). So what a page holds can be read over a
//!   whole session, on any engine, without a debugger: what grows from line to line (JavaScriptCore's
//!   objectTypeCounts, QuickJS's object and string counts) is what it keeps. The collection stops the page's thread
//!   (gc_ms in each line says for how long), which is why the log is off unless asked for.

use std::fs::File;
use std::io::Write;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use crate::JsEngine;

/// RAEA_INSPECT is on (set and not "0"): the heap log, and on JavaScriptCore the context listed in Safari's Develop
/// menu for its Web Inspector.
pub fn inspect() -> bool {
    !matches!(raea_io_trace::stat_value("RAEA_INSPECT").as_deref(), None | Some("" | "0"))
}

pub struct Upkeep {
    release_every: f64,
    released: Instant,
    heap_every: f64,
    logged: Instant,
    log: Option<File>,
    started: Instant,
}

impl Upkeep {
    /// Its settings from the environment; with the heap log on, its file opened (one per engine).
    pub fn from_env() -> Upkeep {
        let secs = |k: &str| std::env::var(k).ok().and_then(|v| v.parse::<f64>().ok());
        let heap_every = secs("RAEA_JS_HEAP_S").unwrap_or(if inspect() { 30.0 } else { 0.0 });
        let log = if heap_every > 0.0 { open_log() } else { None };
        let now = Instant::now();
        Upkeep { release_every: secs("RAEA_JSC_RELEASE_S").unwrap_or(5.0), released: now, heap_every, logged: now, log, started: now }
    }

    /// Between frames: what's due of the two.
    pub fn tick(&mut self, js: &mut dyn JsEngine) {
        if self.release_every > 0.0 && self.released.elapsed().as_secs_f64() >= self.release_every {
            js.release_memory();
            self.released = Instant::now();
        }
        if self.log.is_some() && self.logged.elapsed().as_secs_f64() >= self.heap_every {
            self.write(js);
            self.logged = Instant::now();
        }
    }

    fn write(&mut self, js: &mut dyn JsEngine) {
        let t = Instant::now();
        js.collect();
        let gc_ms = t.elapsed().as_secs_f64() * 1e3;
        let Some(heap) = js.heap() else {
            eprintln!("raea-runtime-jsengine: {} doesn't say what its heap holds; no heap log", js.name());
            self.log = None;
            return;
        };
        let at = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
        let up = self.started.elapsed().as_secs();
        let engine = js.name();
        if let Some(f) = self.log.as_mut() {
            let _ = writeln!(f, r#"{{"at":{at},"up_s":{up},"engine":"{engine}","gc_ms":{gc_ms:.1},"heap":{heap}}}"#);
        }
    }
}

fn open_log() -> Option<File> {
    static N: AtomicU32 = AtomicU32::new(0);
    let home = raea_io_home::user_home()?;
    let dir = PathBuf::from(home).join(".raea").join("logs");
    if let Err(e) = std::fs::create_dir_all(&dir) {
        eprintln!("raea-runtime-jsengine: no heap log in {}: {e}", dir.display());
        return None;
    }
    let path = dir.join(format!("js-heap-{}-{}.jsonl", std::process::id(), N.fetch_add(1, Ordering::Relaxed)));
    match File::options().create(true).append(true).open(&path) {
        Ok(f) => {
            eprintln!("raea-runtime-jsengine: the JS heap's log: {}", path.display());
            Some(f)
        }
        Err(e) => {
            eprintln!("raea-runtime-jsengine: no heap log at {}: {e}", path.display());
            None
        }
    }
}
