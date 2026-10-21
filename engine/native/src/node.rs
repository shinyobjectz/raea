//! Node's core for raea's own TypeScript tools on JavaScriptCore (js::run_node): what bun gave @raea/data's scripts and
//! void's parts, with no bun process and no second engine. Imports resolve as Node does (builtins, relative files,
//! packages from node_modules through package.json's exports, module and main), CommonJS files load through a
//! synchronous require, and fs, path, os, crypto, child_process, process and Buffer are node.js over one native
//! (`__raea_node`, here). Trusted code only: the files and processes it reaches are the machine's, unlike js-exec's
//! sandbox for the agent's scripts.
use std::collections::HashMap;
use std::ffi::c_void;
use std::path::{Path, PathBuf};

use base64::Engine as _;
use serde_json::{json, Value};

use crate::jsc;

/// The builtins node.js defines, by the names an import or require gives them (with or without `node:`)
pub const BUILTINS: &[&str] = &[
    "fs", "fs/promises", "path", "os", "crypto", "child_process", "process", "buffer", "url", "util", "events", "module", "perf_hooks",
    "inspector", "assert", "stream", "tty", "worker_threads", "readline",
];

/// A builtin's name, when `spec` names one
pub fn builtin(spec: &str) -> Option<&'static str> {
    let name = spec.strip_prefix("node:").unwrap_or(spec);
    BUILTINS.iter().copied().find(|b| *b == name)
}

const EXTS: &[&str] = &["", ".ts", ".tsx", ".mts", ".js", ".mjs", ".cjs", ".json"];

/// `spec` imported from a file in `dir` (`esm`: an import, else a require), to the file it names
pub fn resolve_file(spec: &str, dir: &Path, esm: bool) -> Result<PathBuf, String> {
    let raw = spec.strip_prefix("file://").unwrap_or(spec);
    if raw.starts_with("./") || raw.starts_with("../") || raw.starts_with('/') || raw == "." || raw == ".." {
        let want = if raw.starts_with('/') { PathBuf::from(raw) } else { dir.join(raw) };
        return as_file(&want).ok_or_else(|| format!("Cannot find module '{spec}' from {}", dir.display()));
    }
    // a package: its name (a scope takes two parts) and the subpath after it
    let mut parts = raw.splitn(if raw.starts_with('@') { 3 } else { 2 }, '/');
    let name = if raw.starts_with('@') { format!("{}/{}", parts.next().unwrap_or(""), parts.next().unwrap_or("")) } else { parts.next().unwrap_or("").to_string() };
    let sub = parts.next().map(|s| format!("./{s}")).unwrap_or_else(|| ".".into());
    let mut d = Some(dir);
    while let Some(here) = d {
        let pkg = here.join("node_modules").join(&name);
        if pkg.is_dir() {
            return package_entry(&pkg, &sub, esm).ok_or_else(|| format!("Cannot find module '{spec}': {} has no {sub}", pkg.display()));
        }
        d = here.parent();
    }
    Err(format!("Cannot find package '{name}' imported from {}", dir.display()))
}

/// `p` with its `.` and `..` parts taken out (without following links), as the module loader names URLs
pub(crate) fn lexical(p: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for c in p.components() {
        match c {
            std::path::Component::CurDir => {}
            std::path::Component::ParentDir => {
                out.pop();
            }
            c => out.push(c),
        }
    }
    out
}

/// A path as a file: as given, with each extension, or a folder's package.json main or index
fn as_file(p: &Path) -> Option<PathBuf> {
    as_file_raw(&lexical(p))
}

fn as_file_raw(p: &Path) -> Option<PathBuf> {
    for e in EXTS {
        let c = PathBuf::from(format!("{}{e}", p.display()));
        if c.is_file() {
            return Some(c);
        }
    }
    if p.is_dir() {
        if let Some(m) = read_json(&p.join("package.json")).and_then(|j| j["main"].as_str().map(str::to_string)) {
            if let Some(f) = as_file(&p.join(m)) {
                return Some(f);
            }
        }
        for e in &EXTS[1..] {
            let c = p.join(format!("index{e}"));
            if c.is_file() {
                return Some(c);
            }
        }
    }
    None
}

fn read_json(p: &Path) -> Option<Value> {
    serde_json::from_str(&std::fs::read_to_string(p).ok()?).ok()
}

/// A package's file for `sub` ("." or "./x"): its exports (conditions in Node's order for an import or a require),
/// else module or main, else the path itself
fn package_entry(pkg: &Path, sub: &str, esm: bool) -> Option<PathBuf> {
    let json = read_json(&pkg.join("package.json")).unwrap_or(Value::Null);
    let conditions: &[&str] = if esm { &["import", "module", "node", "default", "require"] } else { &["require", "node", "default", "import"] };
    if let Some(exports) = json.get("exports") {
        let target = match exports {
            Value::String(_) if sub == "." => Some(exports.clone()),
            Value::Object(m) if m.keys().any(|k| k.starts_with('.')) => m.get(sub).cloned().or_else(|| {
                // a pattern ("./*")
                m.iter().find_map(|(k, v)| {
                    let (pre, post) = k.split_once('*')?;
                    let mid = sub.strip_prefix(pre)?.strip_suffix(post)?;
                    Some(replace_star(v, mid))
                })
            }),
            Value::Object(_) if sub == "." => Some(exports.clone()),
            _ => None,
        };
        if let Some(t) = target.and_then(|t| pick(&t, conditions)) {
            return as_file(&pkg.join(t));
        }
    }
    if sub == "." {
        for key in if esm { ["module", "main"] } else { ["main", "main"] } {
            if let Some(m) = json.get(key).and_then(Value::as_str) {
                if let Some(f) = as_file(&pkg.join(m)) {
                    return Some(f);
                }
            }
        }
        return as_file(&pkg.join("index"));
    }
    as_file(&pkg.join(sub))
}

fn replace_star(v: &Value, mid: &str) -> Value {
    match v {
        Value::String(s) => Value::String(s.replace('*', mid)),
        Value::Object(m) => Value::Object(m.iter().map(|(k, x)| (k.clone(), replace_star(x, mid))).collect()),
        Value::Array(a) => Value::Array(a.iter().map(|x| replace_star(x, mid)).collect()),
        other => other.clone(),
    }
}

/// An exports target's path for the first condition it has
fn pick(t: &Value, conditions: &[&str]) -> Option<String> {
    match t {
        Value::String(s) => Some(s.clone()),
        Value::Array(a) => a.iter().find_map(|x| pick(x, conditions)),
        Value::Object(m) => conditions.iter().find_map(|c| m.get(*c).and_then(|x| pick(x, conditions))),
        _ => None,
    }
}

/// Whether a .js file is an ES module: .mjs, or the nearest package.json says "type": "module"
fn is_esm(p: &Path) -> bool {
    match p.extension().and_then(|e| e.to_str()) {
        Some("mjs" | "mts" | "ts" | "tsx") => true,
        Some("cjs") => false,
        _ => {
            let mut d = p.parent();
            while let Some(here) = d {
                if let Some(j) = read_json(&here.join("package.json")) {
                    return j["type"].as_str() == Some("module");
                }
                d = here.parent();
            }
            false
        }
    }
}

fn js_string(s: &str) -> String {
    serde_json::to_string(s).unwrap_or_default()
}

/// A file's ES module source: TypeScript transpiled, JSON as its default export, an ES module as it is, a CommonJS
/// file through node.js's require (its module.exports the default export)
pub fn load(path: &Path) -> Result<String, String> {
    // import.meta.main (Bun's and Node 24's): false in an imported module (js::run_node makes it true in the entry);
    // JavaScriptCore's embedding API has no hook for import.meta, so it's written into the text
    load_module(path).map(|s| s.replace("import.meta.main", "false"))
}

/// The entry's source: import.meta.main true
pub fn load_entry(path: &Path) -> Result<String, String> {
    load_module(path).map(|s| s.replace("import.meta.main", "true"))
}

fn load_module(path: &Path) -> Result<String, String> {
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
    if ext == "json" {
        let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
        return Ok(format!("export default {};\n", text.trim()));
    }
    if matches!(ext, "ts" | "tsx" | "mts") {
        let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
        return transpile(path, text);
    }
    if is_esm(path) {
        return std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()));
    }
    Ok(format!("export default globalThis.__raea_node_require({})({});\n", js_string(&path.parent().unwrap_or(Path::new("/")).display().to_string()), js_string(&path.display().to_string())))
}

#[cfg(feature = "modules")]
fn transpile(path: &Path, text: String) -> Result<String, String> {
    raea_modules::transpile(path, text).map(|m| m.source).map_err(|r| r.to_string())
}

#[cfg(not(feature = "modules"))]
fn transpile(path: &Path, _text: String) -> Result<String, String> {
    Err(format!("{}: TypeScript needs raea-native's modules feature", path.display()))
}

// ---- the native ---------------------------------------------------------------------------------------------------

fn err(code: &str, e: impl std::fmt::Display) -> Value {
    json!({"error": {"code": code, "message": e.to_string()}})
}

fn io_err(e: std::io::Error, what: &str, p: &str) -> Value {
    let code = match e.kind() {
        std::io::ErrorKind::NotFound => "ENOENT",
        std::io::ErrorKind::AlreadyExists => "EEXIST",
        std::io::ErrorKind::PermissionDenied => "EACCES",
        std::io::ErrorKind::DirectoryNotEmpty => "ENOTEMPTY",
        _ if e.raw_os_error() == Some(libc::ENOTDIR) => "ENOTDIR",
        _ if e.raw_os_error() == Some(libc::EISDIR) => "EISDIR",
        _ => "EIO",
    };
    err(code, format!("{code}: {e}, {what} '{p}'"))
}

fn b64(b: &[u8]) -> String {
    base64::engine::general_purpose::STANDARD.encode(b)
}

fn unb64(s: &str) -> Vec<u8> {
    base64::engine::general_purpose::STANDARD.decode(s).unwrap_or_default()
}

/// data as node.js hands it over: {"s": text} or {"b": base64}
fn bytes_of(v: &Value) -> Vec<u8> {
    match (v.get("s").and_then(Value::as_str), v.get("b").and_then(Value::as_str)) {
        (Some(s), _) => s.as_bytes().to_vec(),
        (_, Some(b)) => unb64(b),
        _ => Vec::new(),
    }
}

fn millis(t: std::io::Result<std::time::SystemTime>) -> f64 {
    t.ok().and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map(|d| d.as_secs_f64() * 1e3).unwrap_or(0.0)
}

/// One call from node.js: `op` with its arguments (a JSON array), its result as JSON ({"error": ...} when it fails)
pub fn call(op: &str, a: &[Value]) -> Value {
    let s = |i: usize| a.get(i).and_then(Value::as_str).unwrap_or("").to_string();
    let b = |i: usize| a.get(i).and_then(Value::as_bool).unwrap_or(false);
    match op {
        "exists" => json!(Path::new(&s(0)).exists()),
        "readFile" => {
            let p = s(0);
            match std::fs::read(&p) {
                Ok(bytes) if b(1) => json!({"s": String::from_utf8_lossy(&bytes)}),
                Ok(bytes) => json!({"b": b64(&bytes)}),
                Err(e) => io_err(e, "open", &p),
            }
        }
        "writeFile" => {
            let p = s(0);
            let data = bytes_of(a.get(1).unwrap_or(&Value::Null));
            let r = if b(2) {
                use std::io::Write;
                std::fs::OpenOptions::new().create(true).append(true).open(&p).and_then(|mut f| f.write_all(&data))
            } else {
                std::fs::write(&p, data)
            };
            r.map(|_| Value::Null).unwrap_or_else(|e| io_err(e, "open", &p))
        }
        "mkdir" => {
            let p = s(0);
            let r = if b(1) { std::fs::create_dir_all(&p) } else { std::fs::create_dir(&p) };
            r.map(|_| Value::Null).unwrap_or_else(|e| io_err(e, "mkdir", &p))
        }
        "readdir" => {
            let p = s(0);
            match std::fs::read_dir(&p) {
                Ok(rd) => {
                    let out: Vec<Value> = rd
                        .flatten()
                        .map(|e| {
                            let t = e.file_type().ok();
                            json!({"name": e.file_name().to_string_lossy(), "dir": t.is_some_and(|t| t.is_dir()), "file": t.is_some_and(|t| t.is_file()), "link": t.is_some_and(|t| t.is_symlink())})
                        })
                        .collect();
                    // in the directory's own order, as Node and bun list it (readdir(3), unsorted)
                    Value::Array(out)
                }
                Err(e) => io_err(e, "scandir", &p),
            }
        }
        "stat" => {
            let p = s(0);
            let m = if b(1) { std::fs::symlink_metadata(&p) } else { std::fs::metadata(&p) };
            match m {
                Ok(m) => {
                    use std::os::unix::fs::MetadataExt;
                    json!({"dir": m.is_dir(), "file": m.is_file(), "link": m.file_type().is_symlink(), "size": m.len(), "mode": m.mode(),
                        "mtimeMs": millis(m.modified()), "atimeMs": millis(m.accessed()), "birthtimeMs": millis(m.created()), "ctimeMs": m.ctime() as f64 * 1e3 + m.ctime_nsec() as f64 / 1e6})
                }
                Err(e) => io_err(e, if b(1) { "lstat" } else { "stat" }, &p),
            }
        }
        "rm" => {
            let p = s(0);
            let path = Path::new(&p);
            let r = match std::fs::symlink_metadata(path) {
                Ok(m) if m.is_dir() && b(1) => std::fs::remove_dir_all(path),
                Ok(m) if m.is_dir() => std::fs::remove_dir(path),
                Ok(_) => std::fs::remove_file(path),
                Err(e) if b(2) && e.kind() == std::io::ErrorKind::NotFound => Ok(()),
                Err(e) => Err(e),
            };
            r.map(|_| Value::Null).unwrap_or_else(|e| io_err(e, "rm", &p))
        }
        "rename" => std::fs::rename(s(0), s(1)).map(|_| Value::Null).unwrap_or_else(|e| io_err(e, "rename", &s(0))),
        "copyFile" => std::fs::copy(s(0), s(1)).map(|_| Value::Null).unwrap_or_else(|e| io_err(e, "copyfile", &s(0))),
        "realpath" => std::fs::canonicalize(s(0)).map(|p| json!(p.display().to_string())).unwrap_or_else(|e| io_err(e, "realpath", &s(0))),
        "mkdtemp" => {
            let prefix = s(0);
            for _ in 0..100 {
                let mut r = [0u8; 6];
                let _ = getrandom::fill(&mut r);
                let tail: String = r.iter().map(|x| (b'a' + x % 26) as char).collect();
                let p = format!("{prefix}{tail}");
                match std::fs::create_dir(&p) {
                    Ok(()) => return json!(p),
                    Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
                    Err(e) => return io_err(e, "mkdtemp", &prefix),
                }
            }
            err("EEXIST", "mkdtemp: no free name")
        }
        "hash" => {
            use sha2::Digest;
            let data = unb64(&s(1));
            let out: Vec<u8> = match s(0).to_ascii_lowercase().as_str() {
                "sha256" => sha2::Sha256::digest(&data).to_vec(),
                "sha512" => sha2::Sha512::digest(&data).to_vec(),
                "sha384" => sha2::Sha384::digest(&data).to_vec(),
                "sha1" => sha1::Sha1::digest(&data).to_vec(),
                other => return err("ERR_OSSL_EVP_UNSUPPORTED", format!("Digest method not supported: {other}")),
            };
            json!(b64(&out))
        }
        "random" => {
            let mut v = vec![0u8; a.first().and_then(Value::as_u64).unwrap_or(0) as usize];
            let _ = getrandom::fill(&mut v);
            json!(b64(&v))
        }
        "spawn" => spawn(&s(0), a.get(1), a.get(2).unwrap_or(&Value::Null)),
        "tmpdir" => json!(std::env::temp_dir().display().to_string().trim_end_matches('/')),
        "homedir" => json!(std::env::var("HOME").unwrap_or_default()),
        "cwd" => json!(std::env::current_dir().map(|p| p.display().to_string()).unwrap_or_default()),
        "resolve" => resolve_file(&s(0), Path::new(&s(1)), false).map(|p| json!(p.display().to_string())).unwrap_or_else(|e| err("MODULE_NOT_FOUND", e)),
        // a CommonJS file's source for require (JSON as an expression, TypeScript transpiled to CommonJS)
        "source" => {
            let p = PathBuf::from(s(0));
            let text = match std::fs::read_to_string(&p) {
                Ok(t) => t,
                Err(e) => return io_err(e, "open", &s(0)),
            };
            match p.extension().and_then(|e| e.to_str()) {
                Some("json") => json!({"json": text}),
                Some("ts" | "tsx" | "mts") => match transpile_cjs(&p, text) {
                    Ok(js) => json!({"js": js}),
                    Err(e) => err("ERR_TRANSPILE", e),
                },
                _ => json!({"js": text}),
            }
        }
        other => err("ERR_UNKNOWN_OP", format!("node: no op {other}")),
    }
}

#[cfg(feature = "modules")]
fn transpile_cjs(path: &Path, text: String) -> Result<String, String> {
    let m = raea_modules::transpile(path, text).map_err(|r| r.to_string())?;
    raea_modules::to_commonjs(&format!("file://{}", path.display()), m.source).map_err(|r| r.to_string())
}

#[cfg(not(feature = "modules"))]
fn transpile_cjs(path: &Path, _text: String) -> Result<String, String> {
    Err(format!("{}: TypeScript needs raea-native's modules feature", path.display()))
}

/// child_process.spawnSync: the program run to its end, its output whole
fn spawn(cmd: &str, args: Option<&Value>, o: &Value) -> Value {
    let mut c = std::process::Command::new(cmd);
    if let Some(args) = args.and_then(Value::as_array) {
        c.args(args.iter().map(|x| x.as_str().map(str::to_string).unwrap_or_else(|| x.to_string())));
    }
    if let Some(cwd) = o.get("cwd").and_then(Value::as_str) {
        c.current_dir(cwd);
    }
    if let Some(env) = o.get("env").and_then(Value::as_object) {
        c.env_clear();
        c.envs(env.iter().filter_map(|(k, v)| v.as_str().map(|v| (k.clone(), v.to_string()))));
    }
    let input = o.get("input").map(bytes_of).unwrap_or_default();
    c.stdin(if input.is_empty() { std::process::Stdio::null() } else { std::process::Stdio::piped() });
    c.stdout(std::process::Stdio::piped());
    c.stderr(std::process::Stdio::piped());
    let mut child = match c.spawn() {
        Ok(ch) => ch,
        Err(e) => return json!({"error": {"code": if e.kind() == std::io::ErrorKind::NotFound { "ENOENT" } else { "EIO" }, "message": format!("spawnSync {cmd} {e}")}, "status": Value::Null}),
    };
    if !input.is_empty() {
        if let Some(mut stdin) = child.stdin.take() {
            use std::io::Write;
            let _ = std::thread::spawn(move || stdin.write_all(&input));
        }
    }
    match child.wait_with_output() {
        Ok(out) => {
            use std::os::unix::process::ExitStatusExt;
            json!({"status": out.status.code(), "signal": out.status.signal().map(signal_name), "stdout": b64(&out.stdout), "stderr": b64(&out.stderr)})
        }
        Err(e) => json!({"error": {"code": "EIO", "message": e.to_string()}, "status": Value::Null}),
    }
}

fn signal_name(n: i32) -> String {
    match n {
        libc::SIGTERM => "SIGTERM".into(),
        libc::SIGKILL => "SIGKILL".into(),
        libc::SIGINT => "SIGINT".into(),
        libc::SIGSEGV => "SIGSEGV".into(),
        libc::SIGABRT => "SIGABRT".into(),
        n => format!("SIG{n}"),
    }
}

/// `__raea_node(op, argsJson)`: node.js's one native
pub(crate) unsafe extern "C" fn native(ctx: *const c_void, _f: *mut c_void, _this: *mut c_void, argc: usize, argv: *const *const c_void, _exc: *mut *const c_void) -> *const c_void {
    let op = unsafe { jsc::arg(ctx, argc, argv, 0) };
    let args: Vec<Value> = serde_json::from_str(&unsafe { jsc::arg(ctx, argc, argv, 1) }).unwrap_or_default();
    let out = call(&op, &args);
    jsc::string(ctx, &out.to_string())
}

/// node.js: the builtins, installed before the entry runs
pub const NODE_JS: &str = include_str!("node.js");

/// each builtin's ES module: its exports from node.js's object (its keys as named exports, the object as default)
pub fn builtin_module(name: &str) -> String {
    let keys: &[&str] = match name {
        "fs" => &["existsSync", "readFileSync", "writeFileSync", "appendFileSync", "mkdirSync", "readdirSync", "statSync", "lstatSync", "rmSync", "unlinkSync", "rmdirSync", "renameSync", "copyFileSync", "realpathSync", "mkdtempSync", "promises", "constants", "createWriteStream"],
        "fs/promises" => &["readFile", "writeFile", "appendFile", "mkdir", "readdir", "stat", "lstat", "rm", "unlink", "rename", "copyFile", "realpath", "mkdtemp", "access"],
        "path" => &["join", "resolve", "dirname", "basename", "extname", "relative", "normalize", "isAbsolute", "parse", "format", "sep", "delimiter", "posix"],
        "os" => &["tmpdir", "homedir", "platform", "arch", "EOL", "cpus", "type", "release", "hostname", "endianness"],
        "crypto" => &["createHash", "randomUUID", "randomBytes", "getRandomValues", "webcrypto"],
        "child_process" => &["spawnSync", "execSync", "execFileSync"],
        "process" => &["argv", "env", "exit", "cwd", "platform", "stdout", "stderr", "nextTick", "hrtime", "versions", "version"],
        "buffer" => &["Buffer"],
        "url" => &["fileURLToPath", "pathToFileURL", "URL", "URLSearchParams"],
        "util" => &["inspect", "format", "promisify", "inherits", "types", "TextEncoder", "TextDecoder"],
        "events" => &["EventEmitter"],
        "module" => &["createRequire", "builtinModules"],
        "perf_hooks" => &["performance"],
        _ => &[],
    };
    let mut src = format!("const m = globalThis.__raea_node_builtins[{}];\nexport default m;\n", js_string(name));
    if !keys.is_empty() {
        src += &format!("export const {{ {} }} = m;\n", keys.join(", "));
    }
    src
}

/// a builtin's URL (`raea:node/<name>`) as rewrite hands it to the module loader
pub fn builtin_url(name: &str) -> String {
    format!("raea:node/{name}")
}

/// the module loaded for each URL so far (sources by URL), kept by js.rs
pub type Sources = HashMap<String, String>;
