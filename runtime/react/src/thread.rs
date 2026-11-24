//! The JS thread on raea's JS engine (raea-runtime-jsengine: JavaScriptCore on Apple, QuickJS elsewhere): the prelude,
//! the shims, the bundle, then timers and Bevy's events.

use super::*;
use raea_runtime_jsengine::{Arg, Engine, Ret, arg, clock, now_ms};

/// `__raea_flush(json)`: a commit's ops to Bevy
fn flush(a: &[String]) -> Result<Ret, String> {
    let a0 = raea_core_alloc::here().0;
    let json = arg(a, 0).to_string();
    let len = json.len() as u64;
    FLUSH.with(|f| {
        if let Some((tx, state)) = f.borrow().as_ref() {
            let _ = tx.send(Batch(json));
            state.flushed.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        }
    });
    FLUSH_BYTES.fetch_add(len, std::sync::atomic::Ordering::Relaxed);
    FLUSH_ALLOCS.fetch_add(raea_core_alloc::here().0 - a0, std::sync::atomic::Ordering::Relaxed);
    let (n, cv) = &FLUSHED;
    *n.lock().unwrap() += 1;
    cv.notify_all();
    Ok(Ret::Undefined)
}

/// `__raea_log(level, text)`: to Bevy's log
fn log(a: &[String]) -> Result<Ret, String> {
    let text = arg(a, 1);
    // a headless program's console to the terminal, as node's: log and info to stdout, warn and error to stderr (#168)
    let (on, echo) = (native::CONSOLE.load(std::sync::atomic::Ordering::Relaxed), native::ECHO.load(std::sync::atomic::Ordering::Relaxed));
    if on && echo {
        use std::io::Write;
        match arg(a, 0) {
            "1" | "2" => {
                let _ = writeln!(std::io::stderr().lock(), "{text}");
            }
            _ => {
                let mut o = std::io::stdout().lock();
                let _ = writeln!(o, "{text}");
                let _ = o.flush();
            }
        }
        return Ok(Ret::Undefined);
    }
    match arg(a, 0) {
        "2" => error!("[js] {text}"),
        "1" => warn!("[js] {text}"),
        _ => info!("[js] {text}"),
    }
    Ok(Ret::Undefined)
}

/// Every native call a composition's thread answers; `raea types` types this same list (sdk/raea/src/native-calls.ts).
pub fn register_natives() {
    raea_native::register("page.viewBox", "{}", "[number, number, number, number] | null", "the box (x, y, w, h in frame px) a camera in a page element draws its view in, as last laid out; null when it draws under the whole frame (3D picking in the box, docs/sdk.feature S3a)", |_| Ok(serde_json::json!(*VIEW_BOX.lock().unwrap())));
    // the calls crate's, matchmove.solve (scene/track.rs), physics.* (systems/bake), scene.* (scene/contact.rs), *.shot (runtime/shots)
    [native::register, layout::register, scene::track::register, scene::physics::register, raea_runtime_shots::register, raea_runtime_canvas::register, scene::register_scene].iter().for_each(|r| r());
}

/// The JS thread: the prelude, the shims, the bundle, then timers and Bevy's events. Both engines run promise jobs as
/// each call returns, so there are no microtask checkpoints to make.
pub(crate) fn js_thread(bundle: PathBuf, tx: Sender<Batch>, events: Receiver<String>, state: std::sync::Arc<JsState>) {
    FLUSH.with(|f| *f.borrow_mut() = Some((tx, state.clone())));
    let code = match std::fs::read_to_string(&bundle) {
        Ok(c) => c,
        Err(e) => {
            error!("raea-runtime-react: {}: {e}", bundle.display());
            return;
        }
    };
    interactive_thread();
    js_runtime_starts();
    let mut ctx = match Engine::default().start() {
        Ok(c) => c,
        Err(e) => {
            error!("raea-runtime-react: {e}");
            return;
        }
    };
    let natives: [(&str, raea_runtime_jsengine::Native); 3] = [("__raea_flush", Box::new(flush)), ("__raea_log", Box::new(log)), ("__raea_now", clock())];
    for (name, f) in natives {
        if let Err(e) = ctx.native(name, f) {
            error!("raea-runtime-react: {e}");
            return;
        }
    }
    register_natives();
    for (src, name) in [(PRELUDE, "prelude.js")] {
        if let Err(e) = ctx.eval(src, name) {
            error!("raea-runtime-react: {name}: {e}");
            return;
        }
    }
    if let Err(e) = raea_native::host::install(&mut *ctx) {
        error!("raea-runtime-react: {e}");
        return;
    }
    if let Err(e) = ctx.eval(&code, "bundle.js") {
        // printed plainly as well: this thread can start before Bevy's log subscriber exists, which drops the line
        eprintln!("[js] bundle.js threw before its first commit: {e}");
        *BUNDLE_ERROR.lock().unwrap() = Some(e);
        return;
    }
    // RAEA_JSC_EDEN=N: a young collection after every Nth event's commit (1: each), at the frame's boundary, where the
    // JS thread waits for the next frame anyway, so the collections JavaScriptCore would start on allocation, inside a
    // frame's render, don't come
    let eden_every: u64 = std::env::var("RAEA_JSC_EDEN").ok().and_then(|v| v.parse().ok()).unwrap_or(0);
    let mut delivered: u64 = 0;
    let release_every: f64 = std::env::var("RAEA_JSC_RELEASE_S").ok().and_then(|v| v.parse().ok()).unwrap_or(5.0);
    let mut released = Instant::now();
    let mut fault = raea_runtime_shared::DropFault::from_env();
    // the next timers call has a timer to run (an empty tick isn't work, #158): the bundle's, an event handler's, or one
    // that came due while the thread waited
    let mut due = true;
    loop {
        let busy = Instant::now();
        js_call_begins(false, &state);
        let timers = ctx.call("__raea_timers", &[Arg::Num(now_ms())]);
        js_call_ends(&state);
        let next = match timers {
            None => break,
            Some(Ok(n)) if n.is_finite() => n,
            Some(Ok(_)) => -1.0,
            Some(Err(e)) => {
                error!("raea-runtime-react: a timer threw: {e}");
                -1.0
            }
        };
        JS_NS.fetch_add(busy.elapsed().as_nanos() as u64, std::sync::atomic::Ordering::Relaxed);
        state.timers(due, next);
        let wait = if next < 0.0 { Duration::from_millis(500) } else { Duration::from_millis(next.clamp(0.0, 500.0) as u64) };
        due = (0.0..=500.0).contains(&next);
        let msg = match events.recv_timeout(wait) {
            Ok(json) => Some(json),
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => continue,
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => None,
        };
        if msg.as_deref().is_some_and(|j| fault.drops(j)) {
            continue;
        }
        let busy = Instant::now();
        let event = match &msg {
            Some(json) => Arg::Str(json),
            None => Arg::Null,
        };
        js_call_begins(true, &state);
        let outcome = ctx.call("__raea_deliver", &[event]);
        js_call_ends(&state);
        match &outcome {
            None => break,
            Some(Err(e)) => error!("raea-runtime-react: an event threw: {e}"),
            Some(Ok(_)) => {}
        }
        // the prelude answers 0 for an event queued with no one waiting: the bridge's event loop has stopped
        state.delivered(!matches!(outcome, Some(Ok(n)) if n == 0.0));
        due = true;
        JS_NS.fetch_add(busy.elapsed().as_nanos() as u64, std::sync::atomic::Ordering::Relaxed);
        // a young collection after the frame when RAEA_JSC_EDEN is set (off: each cost ~0.5 ms and held memory no
        // flatter in pong; every 16th frame left 1 collection of 360 started by allocation inside a frame, where 71
        // were, at 2.4 ms of the JS thread each), then the engine's upkeep (JavaScriptCore's GC timers), after the frame's commit is out (Bevy
        // applies and presents it meanwhile): the timers start most collections, on the collector thread, while the
        // JS thread waits for the next frame
        delivered += 1;
        if eden_every > 0 && delivered % eden_every == 0 {
            let t = Instant::now();
            ctx.collect_young();
            EDEN_NS.fetch_add(t.elapsed().as_nanos() as u64, std::sync::atomic::Ordering::Relaxed);
        }
        ctx.idle();
        // every few seconds, JavaScriptCore's free pages back to the system (RAEA_JSC_RELEASE_S; 0 never)
        if release_every > 0.0 && released.elapsed().as_secs_f64() >= release_every {
            if let Some(ms) = ctx.release_memory() {
                RELEASE_NS.fetch_add((ms * 1e6) as u64, std::sync::atomic::Ordering::Relaxed);
            }
            released = Instant::now();
        }
        if msg.is_none() {
            break;
        }
    }
}
