//! The JS engine, behind one trait (docs/ports.feature PT0b): JavaScriptCore on Apple (the system's engine, nothing
//! shipped), QuickJS everywhere else (rquickjs: a C engine that builds wherever Bevy does, wasm included). Every raea
//! host that runs JS (js-exec and the Node shim in raea-native, a page's roots in raea-runtime-js, React's thread in
//! raea-runtime-react) runs it through this, so a bundle runs the same on each engine: the host evaluates scripts,
//! calls globals by name with strings and numbers, and installs its natives as Rust functions.
//!
//! Which engine a host gets: `Engine::default()`, JavaScriptCore on Apple unless the `portable` feature is on (it
//! forces QuickJS, so the portable path is tested on a Mac), QuickJS elsewhere; `BEVY_TSX_ENGINE=jsc|quickjs` picks one
//! of those compiled in at run time.

use std::cell::RefCell;
use std::rc::Rc;

#[cfg(target_vendor = "apple")]
pub mod jsc;
#[cfg(any(feature = "portable", not(target_vendor = "apple")))]
mod quickjs;

/// An argument to a global function.
pub enum Arg<'a> {
    Num(f64),
    Str(&'a str),
    Null,
}

/// What a native hands back to JS.
pub enum Ret {
    Undefined,
    Num(f64),
    Str(String),
}

/// A native's body: its arguments as passed (each as JS's String(v)) to its value, or Err to throw an Error with that
/// message.
pub type Native = Box<dyn Fn(&[String]) -> Result<Ret, String>>;

thread_local! {
    static T0: std::time::Instant = std::time::Instant::now();
    static HELD: std::cell::Cell<Option<f64>> = const { std::cell::Cell::new(None) };
}

/// ms since this thread first asked: the clock a host's JS timers run on (or the time `at_clock` holds it at).
pub fn now_ms() -> f64 {
    HELD.with(|h| h.get()).unwrap_or_else(|| T0.with(|t| t.elapsed().as_secs_f64() * 1e3))
}

/// `f` run with this thread's JS clock held at `ms`: a root stepped on a composition's frames (an app screen, frame
/// over fps) sees the same timers and performance.now in a render and its play, so the render repeats.
pub fn at_clock<T>(ms: f64, f: impl FnOnce() -> T) -> T {
    let was = HELD.with(|h| h.replace(Some(ms)));
    let out = f();
    HELD.with(|h| h.set(was));
    out
}

/// A native answering `now_ms()`, a host's `now` for its JS.
pub fn clock() -> Native {
    Box::new(|_| Ok(Ret::Num(now_ms())))
}

/// A native's argument `i`, "" when it wasn't passed.
pub fn arg(args: &[String], i: usize) -> &str {
    args.get(i).map_or("", String::as_str)
}

/// An ES module's source by its URL (the host has already rewritten each import to the URL it loads from), or why it
/// can't be loaded (the import then fails with that).
pub type Fetch = Box<dyn Fn(&str) -> Result<String, String>>;

/// A JS engine with one global scope, used from the thread that made it. Calls into it run its promise jobs
/// (microtasks) before they return.
pub trait JsEngine {
    /// The engine's name, for logs and benchmarks ("jsc", "quickjs").
    fn name(&self) -> &'static str;
    /// `name` on the global object, a function backed by `f`.
    fn native(&mut self, name: &str, f: Native) -> Result<(), String>;
    /// `src` run as a classic script (`file` names it in stack traces); Err is what it threw.
    fn eval(&mut self, src: &str, file: &str) -> Result<(), String>;
    /// The global function `func` called with `args`: its value as a number (NaN when it isn't one), what it threw, or
    /// None when there's no such function.
    fn call(&mut self, func: &str, args: &[Arg]) -> Option<Result<f64, String>>;
    /// `src` run as the ES module `url`, its imports from `fetch`; what its evaluation settles to (a promise: top-level
    /// await) is kept as the global `global`, for `watch`; a module that fails to load or throws rejects it.
    fn module(&mut self, url: &str, src: &str, fetch: Fetch, global: &str) -> Result<(), String>;
    /// A script running longer than `limit` is stopped: what it was running fails with an uncatchable error, which
    /// `stopped` recognizes.
    fn time_limit(&mut self, limit: std::time::Duration);
    /// A garbage collection now (between frames, or when a run is done with its garbage).
    fn collect(&mut self) {}
    /// A young-generation collection now, where the engine has one (JavaScriptCore): true when it ran.
    fn collect_young(&mut self) -> bool {
        false
    }
    /// The engine's free pages given back to the system now, where it keeps them (JavaScriptCore): the call's ms.
    fn release_memory(&mut self) -> Option<f64> {
        None
    }
    /// Between frames: the engine's own upkeep (JavaScriptCore's GC timers live on the thread's run loop).
    fn idle(&mut self) {}
}

/// Whether `error` is a run stopped by its time limit (JavaScriptCore's watchdog, QuickJS's interrupt).
pub fn stopped(error: &str) -> bool {
    error.contains("JavaScript execution terminated") || error.contains("interrupted")
}

/// What a watched promise settled to: None while pending, Ok when fulfilled, Err(its reason and stack) when rejected.
pub type Watch = Rc<RefCell<Option<Result<(), String>>>>;

/// The promise kept as the global `global` (JsEngine::module's), watched from Rust.
pub fn watch(js: &mut dyn JsEngine, global: &str) -> Result<Watch, String> {
    let cell = Rc::new(RefCell::new(None));
    let c = cell.clone();
    let native = format!("{global}_settle");
    js.native(
        &native,
        Box::new(move |a| {
            *c.borrow_mut() = Some(if a.first().is_some_and(|s| s == "1") { Ok(()) } else { Err(a.get(1).cloned().unwrap_or_default()) });
            Ok(Ret::Undefined)
        }),
    )?;
    js.eval(&format!("Promise.resolve(globalThis.{global}).then(() => {native}(1), (e) => {native}(2, e && e.stack ? String(e) + '\\n' + e.stack : String(e)));"), "watch")?;
    Ok(cell)
}

/// The engines this build has.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Engine {
    /// JavaScriptCore (Apple only)
    Jsc,
    /// QuickJS (rquickjs)
    QuickJs,
}

impl Default for Engine {
    /// BEVY_TSX_ENGINE's engine, even one this build doesn't have (its `start` says so, rather than another engine
    /// running in its place), else the platform's.
    fn default() -> Self {
        match std::env::var("BEVY_TSX_ENGINE").as_deref() {
            Ok("jsc") => return Engine::Jsc,
            Ok("quickjs") => return Engine::QuickJs,
            _ => {}
        }
        if cfg!(all(target_vendor = "apple", not(feature = "portable"))) { Engine::Jsc } else { Engine::QuickJs }
    }
}

impl Engine {
    /// A fresh engine with no natives yet.
    pub fn start(self) -> Result<Box<dyn JsEngine>, String> {
        match self {
            #[cfg(target_vendor = "apple")]
            Engine::Jsc => Ok(Box::new(jsc::Jsc::new())),
            #[cfg(any(feature = "portable", not(target_vendor = "apple")))]
            Engine::QuickJs => Ok(Box::new(quickjs::Quick::new()?)),
            #[allow(unreachable_patterns)]
            other => Err(format!("this build has no {other:?} engine")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;
    use std::rc::Rc;

    /// Every engine compiled in gives the same answers: natives in and out, a call's number, what a script throws.
    fn same(engine: Engine) {
        let mut js = engine.start().unwrap();
        let seen = Rc::new(RefCell::new(Vec::new()));
        let s = seen.clone();
        js.native("note", Box::new(move |a| {
            s.borrow_mut().push(a.join("|"));
            Ok(Ret::Undefined)
        }))
        .unwrap();
        js.native("twice", Box::new(|a| Ok(Ret::Num(a[0].parse::<f64>().map_err(|e| e.to_string())? * 2.0)))).unwrap();
        js.native("shout", Box::new(|a| Ok(Ret::Str(a[0].to_uppercase())))).unwrap();
        js.native("fail", Box::new(|_| Err("no".into()))).unwrap();
        js.eval("note('a', 1, twice(4), shout('x')); function add(a, b) { return a + Number(b); }", "t.js").unwrap();
        // a promise job runs before the eval that queued it returns
        js.eval("Promise.resolve().then(() => note('job'))", "t.js").unwrap();
        assert_eq!(seen.borrow().as_slice(), ["a|1|8|X", "job"]);
        assert_eq!(js.call("add", &[Arg::Num(2.0), Arg::Str("3")]).unwrap().unwrap(), 5.0);
        assert!(js.call("missing", &[]).is_none());
        let thrown = js.eval("try { fail() } catch (e) { note(String(e.message)) } throw new Error('out')", "t.js");
        assert!(thrown.unwrap_err().contains("out"), "{}", js.name());
        assert_eq!(seen.borrow().last().unwrap(), "no");
    }

    /// ES modules through the host's fetch, their completion watched, and a run stopped by its time limit.
    fn modules(engine: Engine) {
        let mut js = engine.start().unwrap();
        let seen = Rc::new(RefCell::new(Vec::new()));
        let s = seen.clone();
        js.native("note", Box::new(move |a| {
            s.borrow_mut().push(a.join("|"));
            Ok(Ret::Undefined)
        }))
        .unwrap();
        let fetch: Fetch = Box::new(|url| match url {
            "file:///m/b.mjs" => Ok("export const b = 2; export default await Promise.resolve(3);".into()),
            other => Err(format!("{other}: no such module")),
        });
        js.module("file:///m/a.mjs", "import c, { b } from 'file:///m/b.mjs'; note('a', b, c);", fetch, "__entry").unwrap();
        let w = watch(&mut *js, "__entry").unwrap();
        js.eval("0", "turn").unwrap();
        assert_eq!(w.borrow().clone(), Some(Ok(())), "{}", js.name());
        js.module("file:///m/meta.mjs", "note(import.meta.url);", Box::new(|u| Err(u.to_string())), "__meta").unwrap();
        assert_eq!(seen.borrow().as_slice(), ["a|2|3", "file:///m/meta.mjs"]);

        let fetch: Fetch = Box::new(|url| Err(format!("{url}: refused")));
        js.module("file:///m/c.mjs", "import 'file:///m/gone.mjs';", fetch, "__bad").unwrap();
        let w = watch(&mut *js, "__bad").unwrap();
        for _ in 0..8 {
            js.eval("0", "turn").unwrap();
        }
        assert!(w.borrow().clone().is_some_and(|r| r.is_err()), "{}: {:?}", js.name(), w.borrow().clone());

        js.time_limit(std::time::Duration::from_millis(100));
        let e = js.eval("for (;;) {}", "spin").unwrap_err();
        assert!(stopped(&e), "{}: {e}", js.name());
    }

    #[cfg(target_vendor = "apple")]
    #[test]
    fn jsc() {
        same(Engine::Jsc);
        modules(Engine::Jsc);
    }

    #[cfg(any(feature = "portable", not(target_vendor = "apple")))]
    #[test]
    fn quickjs() {
        same(Engine::QuickJs);
        modules(Engine::QuickJs);
    }
}
