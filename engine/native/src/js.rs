//! The shell's `js-exec` on JavaScriptCore, the system's engine (in place of just-bash's QuickJS): a script, or code
//! given inline, run as an ES module in a fresh context on its own thread. TypeScript and TSX are transpiled in process
//! by raea-modules (deno_ast: no bun, so the same on the iPhone); imports resolve only to files inside the project;
//! console goes to the command's stdout and stderr; native calls are limited to an allow-list (no filesystem outside the
//! shell's own project, no network); a run past its time is terminated.
//!
//! This is also the engine's module seam: `Loader` (raea-modules) resolves every import, rewritten into each module's
//! text as the URL JavaScriptCore's module loader then fetches, so the agent's modules load live, file by file.

use std::cell::RefCell;
use std::collections::HashMap;
use std::ffi::c_void;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use serde_json::{Value, json};

use crate::jsc::{self, Arg, Context};
use crate::{R, s};

/// What a run printed and how it ended.
pub struct Js {
    pub exit: i32,
    pub stdout: String,
    pub stderr: String,
}

/// The module to run: a file in the project (a real path inside `root`), or code given inline (TypeScript when `ts`).
pub enum Entry {
    File(PathBuf),
    Code { text: String, ts: bool, dir: PathBuf },
}

thread_local! {
    static LOADER: RefCell<Option<raea_modules::Loader>> = const { RefCell::new(None) };
    /// each module's source by URL, loaded as an import naming it was resolved
    static SOURCES: RefCell<HashMap<String, String>> = RefCell::new(HashMap::new());
    static OUT: RefCell<(String, String)> = const { RefCell::new((String::new(), String::new())) };
    static T0: Instant = Instant::now();
    /// js::run_node: imports resolve as Node does (node.rs), not inside a sandboxed project
    static NODE: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

const PRELUDE: &str = r#"(function (g) {
  "use strict";
  const fmt = (a) => a.map((x) => typeof x === "string" ? x : (() => { try { return JSON.stringify(x); } catch { return String(x); } })()).join(" ");
  const out = g.__raea_out; delete g.__raea_out;
  g.console = { log: (...a) => out(1, fmt(a) + "\n"), info: (...a) => out(1, fmt(a) + "\n"), debug: (...a) => out(1, fmt(a) + "\n"),
    error: (...a) => out(2, fmt(a) + "\n"), warn: (...a) => out(2, fmt(a) + "\n") };
  const timers = new Map(); let next = 1;
  const now = g.__raea_now; delete g.__raea_now;
  g.setTimeout = (fn, ms = 0, ...a) => { const id = next++; timers.set(id, { at: now() + Math.max(0, +ms || 0), fn: () => fn(...a) }); return id; };
  g.setInterval = (fn, ms = 0, ...a) => { const id = next++; const ev = Math.max(1, +ms || 0); const t = { at: now() + ev, fn: () => { t.at = now() + ev; timers.set(id, t); fn(...a); } }; timers.set(id, t); return id; };
  g.clearTimeout = g.clearInterval = (id) => { timers.delete(id); };
  g.queueMicrotask = (fn) => Promise.resolve().then(fn);
  g.__raea_timers = (t) => {
    for (const [id, x] of [...timers].sort((a, b) => a[1].at - b[1].at)) { if (x.at <= t) { timers.delete(id); x.fn(); } }
    let soon = -1; for (const x of timers.values()) soon = soon < 0 ? x.at - t : Math.min(soon, x.at - t);
    return soon;
  };
  const exitToken = "__raea_exit:";
  g.process = { argv: g.__raea_argv, env: g.__raea_env, platform: g.__raea_platform, cwd: () => g.__raea_cwd,
    exit: (c = 0) => { throw new Error(exitToken + (c | 0)); }, stdin: { text: g.__raea_stdin },
    stdout: { write: (s) => { out(1, String(s)); return true; } }, stderr: { write: (s) => { out(2, String(s)); return true; } } };
})(globalThis);
"#;

unsafe extern "C" fn out_fn(ctx: *const c_void, _f: *mut c_void, _this: *mut c_void, argc: usize, argv: *const *const c_void, _exc: *mut *const c_void) -> *const c_void {
    let fd = unsafe { jsc::arg(ctx, argc, argv, 0) };
    let text = unsafe { jsc::arg(ctx, argc, argv, 1) };
    OUT.with(|o| {
        let mut o = o.borrow_mut();
        if fd == "2" { o.1.push_str(&text) } else { o.0.push_str(&text) }
    });
    jsc::undefined(ctx)
}

unsafe extern "C" fn now_fn(ctx: *const c_void, _f: *mut c_void, _this: *mut c_void, _argc: usize, _argv: *const *const c_void, _exc: *mut *const c_void) -> *const c_void {
    jsc::number(ctx, T0.with(|t| t.elapsed().as_secs_f64() * 1e3))
}

/// A module's text with each import rewritten to the URL raea-modules resolves it to (the module it names loaded and
/// kept for JavaScriptCore's fetch); an import that doesn't resolve stays, and fails as JavaScriptCore loads it.
fn rewrite(url: &str, js: &str) -> String {
    if NODE.with(|n| n.get()) {
        return rewrite_node(url, js);
    }
    raea_modules::rewrite_specifiers(url, js, |spec| {
        LOADER.with(|l| {
            let mut l = l.borrow_mut();
            let l = l.as_mut()?;
            // a refusal travels as a URL of its own, which fetch answers with the reason
            let refused = |e: raea_modules::Refusal| format!("raea:refused/{}", url::form_urlencoded::byte_serialize(e.to_string().as_bytes()).collect::<String>());
            let id = match l.resolve(spec, url) {
                Ok(id) => id,
                Err(e) => return Some(refused(e)),
            };
            let m = match l.load(&id) {
                Ok(m) => m,
                Err(e) => return Some(refused(e)),
            };
            SOURCES.with(|s| s.borrow_mut().insert(m.url.clone(), m.source));
            Some(m.url)
        })
    })
    .unwrap_or_else(|_| js.to_string())
}

/// js::run_node's rewrite: a builtin to its module, anything else resolved from the importing file's folder as Node
/// resolves an import (node.rs), each module loaded as it's named; one that doesn't resolve fails as JavaScriptCore loads it
fn rewrite_node(url: &str, js: &str) -> String {
    let dir = url.strip_prefix("file://").map(|p| Path::new(p).parent().map(Path::to_path_buf).unwrap_or_default()).unwrap_or_else(|| PathBuf::from("/"));
    raea_modules::rewrite_specifiers(url, js, |spec| {
        let refused = |e: String| format!("raea:refused/{}", url::form_urlencoded::byte_serialize(e.as_bytes()).collect::<String>());
        if let Some(name) = crate::node::builtin(spec) {
            let u = crate::node::builtin_url(name);
            SOURCES.with(|s| {
                s.borrow_mut().entry(u.clone()).or_insert_with(|| crate::node::builtin_module(name));
            });
            return Some(u);
        }
        let path = match crate::node::resolve_file(spec, &dir, true) {
            Ok(p) => p,
            Err(e) => return Some(refused(e)),
        };
        let u = format!("file://{}", path.display());
        if SOURCES.with(|s| s.borrow().contains_key(&u)) {
            return Some(u);
        }
        match crate::node::load(&path) {
            Ok(src) => {
                SOURCES.with(|s| s.borrow_mut().insert(u.clone(), src));
                Some(u)
            }
            Err(e) => Some(refused(e)),
        }
    })
    .unwrap_or_else(|_| js.to_string())
}

/// Runs one of raea's own TypeScript tools (`script`, with `args`) as Node would, on JavaScriptCore, on its own thread:
/// Node's core (node.rs: fs, path, os, crypto, child_process, process, Buffer), packages from node_modules, CommonJS
/// through require, the process's whole environment. Trusted code: no sandbox, unlike `run`.
pub fn run_node(script: &Path, args: &[String], stdin: String, timeout: Duration) -> Js {
    // absolute and without `.` or `..` parts: JavaScriptCore names the entry by its normalized URL
    let script = crate::node::lexical(&std::path::absolute(script).unwrap_or_else(|_| script.to_path_buf()));
    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("/"));
    let argv: Vec<String> = ["raea".to_string(), script.display().to_string()].into_iter().chain(args.iter().cloned()).collect();
    let env: serde_json::Map<String, Value> = std::env::vars().map(|(k, v)| (k, Value::String(v))).collect();
    let root = PathBuf::from("/");
    let worker = std::thread::spawn(move || {
        NODE.with(|n| n.set(true));
        run_here(&root, &cwd, Entry::File(script), argv, env, stdin, Vec::new(), timeout)
    });
    worker.join().unwrap_or(Js { exit: 1, stdout: String::new(), stderr: "node: the run panicked\n".into() })
}

/// Runs `entry` in a fresh context on its own thread; `allow` lists the native calls (by name or "prefix.*") it may make.
pub fn run(root: &Path, cwd: &Path, entry: Entry, argv: Vec<String>, env: serde_json::Map<String, Value>, stdin: String, allow: Vec<String>, timeout: Duration) -> Js {
    let root = root.to_path_buf();
    let cwd = cwd.to_path_buf();
    let worker = std::thread::spawn(move || run_here(&root, &cwd, entry, argv, env, stdin, allow, timeout));
    worker.join().unwrap_or(Js { exit: 1, stdout: String::new(), stderr: "js-exec: the run panicked\n".into() })
}

#[allow(clippy::too_many_arguments)]
fn run_here(root: &Path, cwd: &Path, entry: Entry, argv: Vec<String>, env: serde_json::Map<String, Value>, stdin: String, allow: Vec<String>, timeout: Duration) -> Js {
    // js::run_node's tools are trusted: every native call (None); a sandboxed run, only those allowed
    let trusted = NODE.with(|n| n.get());
    crate::ALLOW.with(|a| *a.borrow_mut() = if trusted { None } else { Some(allow) });
    OUT.with(|o| *o.borrow_mut() = (String::new(), String::new()));
    SOURCES.with(|s| s.borrow_mut().clear());
    let loader = match raea_modules::Loader::new(root, Vec::new()) {
        Ok(l) => l,
        Err(e) => return Js { exit: 1, stdout: String::new(), stderr: format!("js-exec: {e}\n") },
    };
    LOADER.with(|l| *l.borrow_mut() = Some(loader));
    let t0 = Instant::now();
    let c = Context::new();
    // a script running past the time is terminated by JavaScriptCore's watchdog; the loop below checks the rest
    c.time_limit(timeout);
    c.set_fn("__raea_out", out_fn);
    c.set_fn("__raea_now", now_fn);
    let globals = [
        ("__raea_argv", json!(argv)),
        ("__raea_env", Value::Object(env)),
        ("__raea_platform", json!(if cfg!(target_os = "ios") { "ios" } else { std::env::consts::OS })),
        ("__raea_stdin", json!(stdin)),
        ("__raea_cwd", json!(cwd.display().to_string())),
    ];
    for (k, v) in globals {
        let _ = c.eval(&format!("globalThis.{k} = {v};"), "js-exec");
    }
    if let Err(e) = c.eval(PRELUDE, "prelude") {
        return Js { exit: 1, stdout: String::new(), stderr: format!("js-exec: prelude: {e}\n") };
    }
    let node = NODE.with(|n| n.get());
    if let Err(e) = jsc::install(&c) {
        return Js { exit: 1, stdout: String::new(), stderr: format!("js-exec: {e}\n") };
    }
    if node {
        c.set_fn("__raea_node", crate::node::native);
        if let Err(e) = c.eval(crate::node::NODE_JS, "node") {
            return Js { exit: 1, stdout: String::new(), stderr: format!("node: {e}\n") };
        }
    }
    // the entry module
    let (url, js) = match &entry {
        // import.meta.main (Bun's and Node 24's): true in the entry, false in the modules it imports (node.rs)
        Entry::File(p) if node => crate::node::load_entry(p).map(|src| (format!("file://{}", p.display()), src)),
        Entry::File(p) => LOADER.with(|l| {
            let mut l = l.borrow_mut();
            let l = l.as_mut().unwrap();
            l.resolve(&p.display().to_string(), &format!("file://{}/", cwd.display())).and_then(|id| l.load(&id)).map(|m| (m.url, m.source)).map_err(|r| r.to_string())
        }),
        Entry::Code { text, ts, dir } => {
            let p = dir.join(if *ts { "[eval].ts" } else { "[eval].mjs" });
            if *ts {
                raea_modules::transpile(&p, text.clone()).map(|m| (format!("file://{}", p.display()), m.source)).map_err(|r| r.to_string())
            } else {
                Ok((format!("file://{}", p.display()), text.clone()))
            }
        }
    }
    .unwrap_or_else(|e| ("file:///[js-exec].mjs".to_string(), format!("throw new Error({:?});", e)));
    // the URL as JavaScriptCore takes it ([eval].ts has brackets)
    let url = url.replace('[', "%5B").replace(']', "%5D");
    let modules = jsc::modules::Modules::new(c.raw(), Box::new(|url| {
        if let Some(why) = url.strip_prefix("raea:refused/") {
            return Err(url::form_urlencoded::parse(format!("x={why}").as_bytes()).next().map(|(_, v)| v.into_owned()).unwrap_or_default());
        }
        let url = url.replace("%5B", "[").replace("%5D", "]");
        let js = SOURCES.with(|s| s.borrow().get(&url).cloned()).ok_or_else(|| format!("{url}: not loaded"))?;
        Ok(rewrite(&url, &js))
    }));
    let js = rewrite(&url.replace("%5B", "[").replace("%5D", "]"), &js);
    let result: Result<(), String> = (|| {
        let p = modules.eval(&url, &js)?;
        c.watch("__raea_entry", p);
        // the loop: due timers (microtasks drain after each call), until the module has settled and nothing is pending
        loop {
            if t0.elapsed() > timeout {
                return Err("__raea_timeout".into());
            }
            let now = T0.with(|t| t.elapsed().as_secs_f64() * 1e3);
            let next = match c.call_num("__raea_timers", &[Arg::Num(now)]) {
                Some(Ok(n)) if n.is_finite() => n,
                Some(Err(e)) => return Err(e),
                _ => -1.0,
            };
            if next < 0.0 {
                // done, or nothing scheduled can settle it: a few more turns for the module loader's own jobs (an
                // import fetched and failed settles over several)
                for _ in 0..8 {
                    if c.settled("__raea_entry").is_some() {
                        break;
                    }
                    let _ = c.eval("0", "turn");
                }
                break;
            }
            std::thread::sleep(Duration::from_millis(next.clamp(0.0, 50.0) as u64));
        }
        match c.settled("__raea_entry") {
            Some(Err(reason)) => Err(reason),
            _ => Ok(()),
        }
    })();
    let mut exit = 0;
    if let Err(e) = result {
        if e == "__raea_timeout" || e.contains("JavaScript execution terminated") {
            exit = 124;
            OUT.with(|o| o.borrow_mut().1.push_str(&format!("js-exec: stopped after {} ms\n", timeout.as_millis())));
        } else if let Some(code) = e.split("__raea_exit:").nth(1).and_then(|c| c.split(|ch: char| !ch.is_ascii_digit() && ch != '-').next()).and_then(|c| c.parse::<i32>().ok()) {
            exit = code;
        } else {
            exit = 1;
            OUT.with(|o| o.borrow_mut().1.push_str(&format!("{e}\n")));
        }
    }
    drop(modules);
    SOURCES.with(|s| s.borrow_mut().clear());
    let (stdout, stderr) = OUT.with(|o| std::mem::take(&mut *o.borrow_mut()));
    Js { exit, stdout, stderr }
}

/// `js.run`: the call the shell's js-exec makes.
pub(crate) fn call(v: &Value) -> R<Value> {
    let root = PathBuf::from(s(v, "root")?);
    let cwd = PathBuf::from(s(v, "cwd")?);
    let entry = match (v.get("file").and_then(Value::as_str), v.get("code").and_then(Value::as_str)) {
        (Some(f), _) => Entry::File(PathBuf::from(f)),
        (None, Some(c)) => Entry::Code { text: c.to_string(), ts: v.get("ts").and_then(Value::as_bool).unwrap_or(true), dir: cwd.clone() },
        _ => return Err("js.run: a file or code".into()),
    };
    let argv: Vec<String> = v.get("argv").and_then(Value::as_array).map(|a| a.iter().filter_map(|x| x.as_str().map(str::to_string)).collect()).unwrap_or_default();
    let env = v.get("env").and_then(Value::as_object).cloned().unwrap_or_default();
    let stdin = v.get("stdin").and_then(Value::as_str).unwrap_or("").to_string();
    let allow: Vec<String> = v.get("allow").and_then(Value::as_array).map(|a| a.iter().filter_map(|x| x.as_str().map(str::to_string)).collect()).unwrap_or_else(|| vec!["web.*".into(), "url.*".into(), "zlib.*".into(), "host.platform".into()]);
    let ms = v.get("timeoutMs").and_then(Value::as_u64).unwrap_or(30_000);
    let r = run(&root, &cwd, entry, argv, env, stdin, allow, Duration::from_millis(ms));
    Ok(json!({"exit": r.exit, "stdout": r.stdout, "stderr": r.stderr}))
}
