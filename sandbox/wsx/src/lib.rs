//! The agent's sandbox (docs/media.feature M5): WASI modules compiled ahead of time for Pulley, run by Wasmtime's
//! interpreter (the iPhone allows no JIT), with only the project folder preopened, a fuel limit, a memory cap, and every
//! call logged as an action row.

use std::collections::HashMap;
use std::ffi::{CStr, CString, c_char};
use std::path::Path;
use std::time::Instant;

use serde_json::{Value, json};
use wasmtime::{Config, Engine, Linker, Module, Store, StoreLimits, StoreLimitsBuilder, Trap};
use wasmtime_wasi::p1::{self, WasiP1Ctx};
use wasmtime_wasi::p2::pipe::MemoryOutputPipe;
use wasmtime_wasi::{FsPerms, WasiCtxBuilder};

struct Ctx {
    wasi: WasiP1Ctx,
    limits: Limits,
}

/// The memory cap, remembering that it refused (so a run that hit it says so, not "unreachable executed" from the
/// guest's own abort when its allocation failed).
struct Limits {
    inner: StoreLimits,
    refused: bool,
}

impl wasmtime::ResourceLimiter for Limits {
    fn memory_growing(&mut self, current: usize, desired: usize, maximum: Option<usize>) -> wasmtime::Result<bool> {
        let ok = self.inner.memory_growing(current, desired, maximum)?;
        self.refused |= !ok;
        Ok(ok)
    }
    fn table_growing(&mut self, current: usize, desired: usize, maximum: Option<usize>) -> wasmtime::Result<bool> {
        self.inner.table_growing(current, desired, maximum)
    }
}

/// The engine's settings, the same for wsxc (which compiles) and the phone (which runs): Pulley, fuel, and no address
/// maps (they doubled a module's size; a trap still says what stopped it, without the wasm offset).
pub fn config() -> wasmtime::Result<Config> {
    let mut cfg = Config::new();
    cfg.target("pulley64")?;
    cfg.consume_fuel(true);
    cfg.generate_address_map(false);
    Ok(cfg)
}

pub struct Sandbox {
    engine: Engine,
    modules: HashMap<String, Module>,
    linker: Linker<Ctx>,
    pub load_ms: f64,
}

/// What one call did: the action row.
pub struct Run {
    pub exit: i32,
    pub stdout: String,
    pub stderr: String,
    pub fuel: u64,
    pub ms: f64,
    pub stopped: Option<String>,
}

impl Sandbox {
    /// The engine (Pulley) and every `NAME.cwasm` in `tools`.
    pub fn new(tools: &Path) -> wasmtime::Result<Sandbox> {
        let t = Instant::now();
        let engine = Engine::new(&config()?)?;
        let mut modules = HashMap::new();
        for e in std::fs::read_dir(tools)? {
            let p = e?.path();
            if p.extension().is_some_and(|x| x == "cwasm") {
                // SAFETY: the modules are the app's own, compiled by wsxc for this engine's settings and signed with it
                let m = unsafe { Module::deserialize_file(&engine, &p)? };
                modules.insert(p.file_stem().unwrap().to_string_lossy().to_string(), m);
            }
        }
        let mut linker: Linker<Ctx> = Linker::new(&engine);
        p1::add_to_linker_sync(&mut linker, |c: &mut Ctx| &mut c.wasi)?;
        Ok(Sandbox { engine, modules, linker, load_ms: t.elapsed().as_secs_f64() * 1e3 })
    }

    /// `tool args...` with `project` as its only folder ("."), `fuel` instructions at most and `mem` bytes of memory.
    pub fn run(&self, tool: &str, args: &[&str], project: &Path, fuel: u64, mem: usize) -> wasmtime::Result<Run> {
        let module = self.modules.get(tool).ok_or_else(|| wasmtime::Error::msg(format!("no tool {tool}")))?;
        let (out, err) = (MemoryOutputPipe::new(4 << 20), MemoryOutputPipe::new(1 << 20));
        let mut argv = vec![tool];
        argv.extend_from_slice(args);
        let wasi = WasiCtxBuilder::new().args(&argv).stdout(out.clone()).stderr(err.clone()).preopened_dir(project, ".", FsPerms::ReadWrite)?.build_p1();
        let mut store = Store::new(&self.engine, Ctx { wasi, limits: Limits { inner: StoreLimitsBuilder::new().memory_size(mem).build(), refused: false } });
        store.limiter(|c| &mut c.limits);
        store.set_fuel(fuel)?;
        let t = Instant::now();
        let r = self.linker.instantiate(&mut store, module).and_then(|i| i.get_typed_func::<(), ()>(&mut store, "_start")?.call(&mut store, ()));
        let ms = t.elapsed().as_secs_f64() * 1e3;
        let used = fuel - store.get_fuel().unwrap_or(0);
        let refused = store.data().limits.refused;
        let (exit, stopped) = match &r {
            Ok(()) => (0, None),
            Err(_) if refused => (-3, Some(format!("stopped: past its memory ({} MB)", mem >> 20))),
            Err(e) => match e.downcast_ref::<wasmtime_wasi::I32Exit>() {
                Some(x) => (x.0, None),
                None if e.downcast_ref::<Trap>() == Some(&Trap::OutOfFuel) => (-2, Some(format!("stopped: past its fuel ({fuel} instructions)"))),
                None => (-1, Some(format!("stopped: {}", e.root_cause()))),
            },
        };
        Ok(Run { exit, stdout: String::from_utf8_lossy(&out.contents()).into(), stderr: String::from_utf8_lossy(&err.contents()).into(), fuel: used, ms, stopped })
    }
}

fn row(cmd: &str, r: &Run) -> Value {
    json!({"action": "exec", "cmd": cmd, "exit": r.exit, "fuel": r.fuel, "ms": (r.ms * 10.0).round() / 10.0,
        "stdout_bytes": r.stdout.len(), "stdout_head": r.stdout.lines().take(4).collect::<Vec<_>>(),
        "stderr_head": r.stderr.lines().next().unwrap_or(""), "stopped": r.stopped})
}

/// The suite: everyday commands, a write, escapes that must fail, and limits that must stop a run. Each case says
/// whether it went as it must; the report says how many did.
pub fn suite(tools: &Path, work: &Path) -> Value {
    let project = work.join("project");
    let _ = std::fs::remove_dir_all(&project);
    let _ = std::fs::create_dir_all(project.join("edits"));
    let notes: String = (1..=200).map(|i| format!("take {:03} {}\n", (i * 37) % 200, if i % 7 == 0 { "mesh sign" } else { "talking head" })).collect();
    let _ = std::fs::write(project.join("notes.txt"), notes);
    let _ = std::fs::write(project.join("edits/sign.rrl"), "mesh sign {shape: \"minus(rounded_box(2.4, 1.1, 0.25, 0.08), text3d(\\\"SALE\\\", 0.62, 0.6))\"}\n");
    let _ = std::fs::write(project.join("clips.json"), r#"{"clips":[{"id":"a","from":0,"to":77},{"id":"b","from":77,"to":793},{"id":"c","from":793,"to":922}]}"#);
    let sb = match Sandbox::new(tools) {
        Ok(s) => s,
        Err(e) => return json!({"error": format!("{e:#}")}),
    };
    let fuel = 20_000_000_000u64;
    let mem = 256 << 20;
    // (label, tool, args, what must happen): the lean set (rtools: one small module for the file and text commands)
    // when it's bundled, else uutils coreutils and ripgrep
    let lean = sb.modules.contains_key("rtools");
    let (cu, grep): (&str, Vec<&str>) = if lean { ("rtools", vec!["grep", "-rn", "mesh", "."]) } else { ("coreutils", vec!["-n", "--no-heading", "mesh", "."]) };
    let gtool = if lean { "rtools" } else { "rg" };
    let mut cases: Vec<(&str, &str, Vec<&str>, &dyn Fn(&Run) -> bool)> = vec![
        ("ls", cu, vec!["ls", "-la", "."], &|r| r.exit == 0 && r.stdout.contains("notes.txt")),
        ("wc", cu, vec!["wc", "-l", "notes.txt"], &|r| r.exit == 0 && r.stdout.trim_start().starts_with("200")),
        ("sort", cu, vec!["sort", "-r", "notes.txt"], &|r| r.exit == 0 && r.stdout.lines().count() == 200),
        ("head", cu, vec!["head", "-n", "3", "notes.txt"], &|r| r.exit == 0 && r.stdout.lines().count() == 3),
        ("grep", gtool, grep, &|r| r.exit == 0 && r.stdout.lines().count() == 29),
        ("jq", "jaq", vec![".clips | map(.to - .from) | add", "clips.json"], &|r| r.exit == 0 && r.stdout.trim() == "922"),
        ("write", cu, vec!["cp", "notes.txt", "edits/notes-copy.txt"], &|r| r.exit == 0),
        ("read back", cu, vec!["wc", "-c", "edits/notes-copy.txt"], &|r| r.exit == 0 && !r.stdout.trim_start().starts_with('0')),
        ("escape: an absolute path", cu, vec!["cat", "/etc/hosts"], &|r| r.exit != 0 && r.stdout.is_empty()),
        ("escape: up and out", cu, vec!["cat", "../../../etc/hosts"], &|r| r.exit != 0 && r.stdout.is_empty()),
    ];
    if lean {
        cases.push(("find", "rtools", vec!["find", ".", "-name", "*.rrl"], &|r| r.exit == 0 && r.stdout.trim() == "./edits/sign.rrl"));
        cases.push(("sed", "rtools", vec!["sed", "s/talking head/TALKING/g", "notes.txt"], &|r| r.exit == 0 && r.stdout.matches("TALKING").count() == 172));
    }
    let mut rows = Vec::new();
    let mut ok = 0;
    let n = cases.len() + 2;
    for (label, tool, args, must) in &cases {
        match sb.run(tool, args, &project, fuel, mem) {
            Ok(r) => {
                let good = must(&r);
                ok += good as usize;
                let mut v = row(&format!("{tool} {}", args.join(" ")), &r);
                v["case"] = json!(label);
                v["as_it_must"] = json!(good);
                rows.push(v);
            }
            Err(e) => rows.push(json!({"case": label, "error": format!("{e:#}"), "as_it_must": false})),
        }
    }
    // limits: a search with too little fuel, and a list too big for its memory, are stopped and reported
    for (label, tool, args, f, m) in [
        ("fuel", gtool, if lean { vec!["grep", "-rn", "mesh", "."] } else { vec!["-n", "mesh", "."] }, 50_000u64, mem),
        ("memory", "jaq", vec!["-n", "[range(100000000)] | length"], fuel, 64usize << 20),
    ] {
        match sb.run(tool, &args, &project, f, m) {
            Ok(r) => {
                let good = r.stopped.is_some() || r.exit != 0;
                ok += good as usize;
                let mut v = row(&format!("{tool} {}", args.join(" ")), &r);
                v["case"] = json!(label);
                v["as_it_must"] = json!(good);
                rows.push(v);
            }
            Err(e) => rows.push(json!({"case": label, "error": format!("{e:#}"), "as_it_must": false})),
        }
    }
    json!({"engine": "pulley64", "load_ms": (sb.load_ms * 10.0).round() / 10.0, "tools": sb.modules.keys().collect::<Vec<_>>(), "as_they_must": format!("{ok}/{n}"), "rows": rows})
}

/// The probe app's entry: the suite as JSON (free it with wsx_free).
///
/// # Safety
/// `tools` and `work` are NUL-terminated paths.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn wsx_suite(tools: *const c_char, work: *const c_char) -> *mut c_char {
    let (t, w) = unsafe { (CStr::from_ptr(tools).to_string_lossy().to_string(), CStr::from_ptr(work).to_string_lossy().to_string()) };
    let v = suite(Path::new(&t), Path::new(&w));
    CString::new(v.to_string()).unwrap_or_default().into_raw()
}

/// # Safety
/// `p` came from wsx_suite.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn wsx_free(p: *mut c_char) {
    if !p.is_null() {
        drop(unsafe { CString::from_raw(p) });
    }
}

/// The sandbox's commands in raea-native's registry, so TypeScript on raea's V8 (just-bash's commands, the agent's
/// tools) runs them through the same `__raea_call` as everything else: `wsx.run` runs one tool in `project` (only that
/// folder visible) under fuel and memory limits and answers its action row's facts; `wsx.tools` lists the tools.
pub fn register(sandbox: Sandbox, project: std::path::PathBuf) {
    let sb = std::sync::Arc::new(sandbox);
    let names: Vec<String> = { let mut n: Vec<String> = sb.modules.keys().cloned().collect(); n.sort(); n };
    raea_native::register("wsx.tools", "{}", "string[]", "the sandbox's WASI tools", move |_| Ok(json!(names)));
    let run_sb = sb.clone();
    raea_native::register(
        "wsx.run",
        "{ tool: string; args: string[]; project?: string; fuel?: number; memMb?: number }",
        "{ exit: number; stdout: string; stderr: string; fuel: number; ms: number; stopped: string | null }",
        "one WASI tool run in the project folder under fuel and memory limits",
        move |v| {
            let tool = raea_native::s(v, "tool")?;
            let args: Vec<String> = v.get("args").and_then(Value::as_array).map(|a| a.iter().filter_map(|x| x.as_str().map(str::to_string)).collect()).unwrap_or_default();
            let argv: Vec<&str> = args.iter().map(String::as_str).collect();
            let fuel = v.get("fuel").and_then(Value::as_u64).unwrap_or(2_000_000_000);
            let mem = (v.get("memMb").and_then(Value::as_u64).unwrap_or(256) as usize) << 20;
            // the project folder given (the shell's root), else the host's default
            let proj = v.get("project").and_then(Value::as_str).map(std::path::PathBuf::from).unwrap_or_else(|| project.clone());
            let r = run_sb.run(tool, &argv, &proj, fuel, mem).map_err(|e| e.to_string())?;
            Ok(json!({"exit": r.exit, "stdout": r.stdout, "stderr": r.stderr, "fuel": r.fuel, "ms": r.ms, "stopped": r.stopped}))
        },
    );
}
