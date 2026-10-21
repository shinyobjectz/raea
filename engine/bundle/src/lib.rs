//! A work folder's entry bundled in process by Rolldown, as `bun build ENTRY --format iife --target browser
//! --define process.env.NODE_ENV='"production"' --minify-syntax` bundled it (docs/vite-in-raea.md, A1). The entry's
//! folder is the bundle's root: its tsconfig.json sets JSX (React's automatic runtime) and `paths`, and its
//! node_modules resolves packages, as they did for bun.
//!
//! CSS (A2): `import "./x.css"` and CSS modules (`import s from "./x.module.css"`, `s.card` its class) go through
//! lightningcss: @imports inlined, nesting and newer syntax lowered for the page engine (Stylo), a module's class
//! names `<file>_<class>_<hash of its CSS>`, so they're the same from any folder. The bundle's CSS, in the order its
//! modules run, comes back beside the script: [`Bundle::css`].
use rolldown::{AttachDebugInfo, Bundler, BundlerOptions, ExperimentalOptions, InputItem, OutputFormat, Platform};
use rolldown_common::{ModuleType, Output};
use rolldown_plugin::{HookBuildEndArgs, HookLoadArgs, HookLoadOutput, HookLoadReturn, HookNoopReturn, HookUsage, Plugin, PluginContext, Pluginable, SharedLoadPluginContext};
use std::borrow::Cow;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

pub mod edit;
pub mod script;
pub mod z3;

/// A bundle: one browser script (an IIFE) and the CSS its modules imported (empty when none)
#[derive(Debug, Clone, PartialEq)]
pub struct Bundle {
    pub js: String,
    pub css: String,
}

/// How a bundle is made
#[derive(Debug, Clone, Default)]
pub struct Options {
    /// a live bundle: each TSX and JSX source outside node_modules compiled by Z3 first (z3.rs; sdk/react/js/lite), as
    /// the bun-era bundler's plugin did (RAEA_LITE_COMPILE=0 turns it off)
    pub z3: bool,
    /// a script's bundle: the engine's resolution on top (script.rs; what react/js/bundle.ts did on bun)
    pub script: Option<script::Script>,
    /// `define`s besides process.env.NODE_ENV (`--define K=V`)
    pub defines: Vec<(String, String)>,
    /// a composition's bundle: the edit's own packages (edit.rs)
    pub edit: Option<edit::Edit>,
}

/// `entry` bundled into one browser script and its CSS.
pub fn bundle(entry: &Path) -> Result<Bundle, String> {
    bundle_with(entry, Options::default())
}

/// `entry` bundled into one browser script and its CSS, as `o` says.
pub fn bundle_with(entry: &Path, o: Options) -> Result<Bundle, String> {
    let entry = std::path::absolute(entry).map_err(|e| format!("{}: {e}", entry.display()))?;
    let cwd = entry.parent().ok_or("the entry has no folder")?.to_path_buf();
    let opts = BundlerOptions {
        input: Some(vec![InputItem { name: Some("bundle".into()), import: entry.to_string_lossy().into_owned() }]),
        cwd: Some(cwd.clone()),
        format: Some(OutputFormat::Iife),
        platform: Some(Platform::Browser),
        define: Some([("process.env.NODE_ENV".to_string(), "\"production\"".to_string())].into_iter().chain(o.defines.iter().cloned()).collect()),
        // no `//#region <path>` before each module: its path names the work folder (raea-motion-b<pid>), so every
        // process's bundle differed and the incremental bake saw every function change (lightspeed T3)
        experimental: Some(ExperimentalOptions { attach_debug_info: Some(AttachDebugInfo::None), ..Default::default() }),
        ..Default::default()
    };
    let sheets: Sheets = Default::default();
    let order: Arc<Mutex<Vec<String>>> = Default::default();
    let css = Css { root: cwd, sheets: sheets.clone(), order: order.clone() };
    // the process's runtime for bundles, entered from a thread of its own: a caller may already be inside a runtime
    // (void's tokio), where block_on can't be called
    static RT: std::sync::OnceLock<Result<tokio::runtime::Runtime, String>> = std::sync::OnceLock::new();
    let rt = RT
        .get_or_init(|| tokio::runtime::Builder::new_multi_thread().thread_name("raea-bundle").enable_all().build().map_err(|e| e.to_string()))
        .as_ref()
        .map_err(|e| e.clone())?;
    let js = std::thread::scope(|s| {
        s.spawn(move || {
            rt.block_on(async move {
                let mut plugins = vec![Pluginable::new_shared(css)];
                if o.z3 && std::env::var("RAEA_LITE_COMPILE").as_deref() != Ok("0") {
                    plugins.push(Pluginable::new_shared(Z3));
                }
                if let Some(s) = o.script {
                    plugins.push(Pluginable::new_shared(script::ScriptResolve(s)));
                }
                if let Some(e) = o.edit {
                    plugins.push(Pluginable::new_shared(edit::EditResolve::new(e)));
                }
                let mut b = Bundler::with_plugins(opts, plugins).map_err(|e| e.to_string())?;
                let out = b.generate().await.map_err(|e| e.to_string())?;
                out.assets
                    .iter()
                    .find_map(|a| match a {
                        Output::Chunk(c) if c.is_entry => Some(c.code.to_string()),
                        _ => None,
                    })
                    .ok_or_else(|| "the bundle has no entry chunk".to_string())
            })
        })
        .join()
        .map_err(|_| "the bundler panicked".to_string())?
    })?;
    // each stylesheet once, in the order its module runs
    let sheets = sheets.lock().unwrap();
    let css: Vec<&str> = order.lock().unwrap().iter().filter_map(|id| sheets.get(id).map(String::as_str)).filter(|c| !c.is_empty()).collect();
    Ok(Bundle { js, css: css.join("\n") })
}

/// compiled CSS by module id
type Sheets = Arc<Mutex<HashMap<String, String>>>;

/// The CSS plugin: a .css module loads as JS (a CSS module's class names, else nothing) and its compiled CSS is kept
#[derive(Debug)]
struct Css {
    root: PathBuf,
    sheets: Sheets,
    /// every module in the order it runs (build_end)
    order: Arc<Mutex<Vec<String>>>,
}

impl Plugin for Css {
    fn name(&self) -> Cow<'static, str> {
        Cow::Borrowed("raea:css")
    }

    fn register_hook_usage(&self) -> HookUsage {
        HookUsage::Load | HookUsage::BuildEnd
    }

    /// The modules in the order they run: depth first from the entry, each after its imports in source order (ESM
    /// evaluation). A CSS module renders no code, so the chunk's own module list leaves it out.
    async fn build_end(&self, ctx: &PluginContext, _args: Option<&HookBuildEndArgs<'_>>) -> HookNoopReturn {
        fn visit(ctx: &PluginContext, id: &str, seen: &mut std::collections::HashSet<String>, out: &mut Vec<String>) {
            if !seen.insert(id.to_string()) {
                return;
            }
            if let Some(info) = ctx.get_module_info(id) {
                for dep in &info.imported_ids {
                    visit(ctx, dep.as_str(), seen, out);
                }
            }
            out.push(id.to_string());
        }
        let mut ids: Vec<_> = ctx.get_module_ids().into_iter().filter(|id| ctx.get_module_info(id).is_some_and(|i| i.is_entry)).collect();
        ids.sort();
        let (mut seen, mut out) = (Default::default(), Vec::new());
        for id in ids {
            visit(ctx, &id, &mut seen, &mut out);
        }
        *self.order.lock().unwrap() = out;
        Ok(())
    }

    async fn load(&self, _ctx: SharedLoadPluginContext, args: &HookLoadArgs<'_>) -> HookLoadReturn {
        let path = args.id.split(['?', '#']).next().unwrap_or(args.id);
        if !path.ends_with(".css") {
            return Ok(None);
        }
        let (css, classes) = compile_css(Path::new(path), &self.root).map_err(anyhow::Error::msg)?;
        self.sheets.lock().unwrap().insert(args.id.to_string(), css);
        let mut names: Vec<_> = classes.into_iter().collect();
        names.sort();
        let fields: Vec<String> = names.iter().map(|(k, v)| format!("  {}: {}", js_string(k), js_string(v))).collect();
        let code = format!("export default {{\n{}\n}};\n", fields.join(",\n"));
        Ok(Some(HookLoadOutput {
            code: code.into(),
            module_type: Some(ModuleType::Js),
            // kept when imported for its effect alone (`import "./x.css"`): its CSS is collected by module order
            side_effects: Some(rolldown_common::side_effects::HookSideEffects::True),
            ..Default::default()
        }))
    }
}

/// `file` compiled: its @imports inlined, lowered for Stylo; a CSS module's classes (name -> compiled name)
fn compile_css(file: &Path, root: &Path) -> Result<(String, HashMap<String, String>), String> {
    use lightningcss::bundler::{Bundler, FileProvider};
    use lightningcss::css_modules::{Config, Pattern};
    use lightningcss::stylesheet::{ParserOptions, PrinterOptions};
    use lightningcss::targets::{Browsers, Targets};
    let module = file.file_name().and_then(|n| n.to_str()).is_some_and(|n| n.ends_with(".module.css"));
    let rel = file.strip_prefix(root).unwrap_or(file).to_string_lossy().into_owned();
    let fs = FileProvider::new();
    let options = ParserOptions {
        filename: rel.clone(),
        css_modules: module.then(|| Config {
            // `[name]` is the file's stem without .module; `[content-hash]` its CSS's, so no folder enters a name
            pattern: Pattern::parse("[name]_[local]_[content-hash]").expect("a valid pattern"),
            ..Default::default()
        }),
        ..Default::default()
    };
    let mut b = Bundler::new(&fs, None, options);
    let mut sheet = b.bundle(file).map_err(|e| format!("{rel}: {e}"))?;
    // Safari 13's syntax: nesting, color-mix(), oklch() and the like written out as Stylo reads them (values are
    // lowered by minify, which keeps the formatting; rules by the printer)
    let targets = Targets::from(Browsers { safari: Some(13 << 16), ..Default::default() });
    sheet.minify(lightningcss::stylesheet::MinifyOptions { targets, ..Default::default() }).map_err(|e| format!("{rel}: {e}"))?;
    let out = sheet.to_css(PrinterOptions { targets, ..Default::default() }).map_err(|e| format!("{rel}: {e}"))?;
    let classes = out.exports.unwrap_or_default().into_iter().map(|(k, v)| {
        let composed: Vec<String> = v.composes.iter().filter_map(|c| match c {
            lightningcss::css_modules::CssModuleReference::Local { name } => Some(name.clone()),
            lightningcss::css_modules::CssModuleReference::Global { name } => Some(name.clone()),
            _ => None,
        }).collect();
        (k, std::iter::once(v.name).chain(composed).collect::<Vec<_>>().join(" "))
    }).collect();
    Ok((out.code, classes))
}

fn js_string(s: &str) -> String {
    let mut o = String::from("\"");
    for c in s.chars() {
        match c {
            '"' => o.push_str("\\\""),
            '\\' => o.push_str("\\\\"),
            '\n' => o.push_str("\\n"),
            c => o.push(c),
        }
    }
    o.push('"');
    o
}

/// The Z3 plugin: a TSX or JSX source outside node_modules loads compiled (z3::compile), each compile kept for the
/// process by its text (void's session: motion's ~30 modules compile once, as serve.ts kept them)
#[derive(Debug)]
struct Z3;

impl Plugin for Z3 {
    fn name(&self) -> Cow<'static, str> {
        Cow::Borrowed("raea:z3")
    }

    fn register_hook_usage(&self) -> HookUsage {
        HookUsage::Load
    }

    async fn load(&self, _ctx: SharedLoadPluginContext, args: &HookLoadArgs<'_>) -> HookLoadReturn {
        let path = args.id.split(['?', '#']).next().unwrap_or(args.id);
        let jsx = path.ends_with(".jsx");
        if !(jsx || path.ends_with(".tsx")) || path.contains("/node_modules/") {
            return Ok(None);
        }
        let src = std::fs::read_to_string(path)?;
        static KEPT: std::sync::LazyLock<Mutex<HashMap<(bool, String), Arc<str>>>> = std::sync::LazyLock::new(Default::default);
        let key = (jsx, src);
        let kept = KEPT.lock().unwrap().get(&key).cloned();
        let code = match kept {
            Some(c) => c,
            None => {
                let c: Arc<str> = z3::compile(&key.1, jsx).map_err(|e| anyhow::anyhow!("{path}: {e}"))?.into();
                let mut k = KEPT.lock().unwrap();
                if k.len() > 512 {
                    k.clear();
                }
                k.insert(key, c.clone());
                c
            }
        };
        Ok(Some(HookLoadOutput { code: code.as_ref().into(), module_type: Some(ModuleType::Js), ..Default::default() }))
    }
}

/// The C API raea-motion loads (libraea_bundle.dylib), so raea links none of Rolldown's crates.
pub mod ffi {
    use std::ffi::{c_char, CStr, CString};

    /// The version of this API: raea-motion refuses a library with another
    #[no_mangle]
    pub extern "C" fn raea_bundle_api() -> u32 {
        1
    }

    /// `entry` (a path, UTF-8) bundled with `opts` (JSON: {"z3": bool, "edit": {"dir", "js"}, "script": {..}, "defines"}); returns JSON, {"js","css"} or {"error"}, to
    /// be freed with raea_bundle_free
    ///
    /// # Safety
    /// `entry` and `opts` are NUL-terminated strings (or `opts` null).
    #[no_mangle]
    pub unsafe extern "C" fn raea_bundle(entry: *const c_char, opts: *const c_char) -> *mut c_char {
        let reply = std::panic::catch_unwind(|| {
            let entry = unsafe { CStr::from_ptr(entry) }.to_string_lossy().into_owned();
            let opts: serde_json::Value = if opts.is_null() { serde_json::Value::Null } else { serde_json::from_slice(unsafe { CStr::from_ptr(opts) }.to_bytes()).unwrap_or_default() };
            let path = |v: &serde_json::Value| std::path::PathBuf::from(v.as_str().unwrap_or(""));
            let script = opts.get("script").filter(|s| s.is_object()).map(|s| super::script::Script {
                js: path(&s["js"]),
                motion: path(&s["motion"]),
                browser: path(&s["browser"]),
                shell: path(&s["shell"]),
                aliases: s["aliases"].as_object().map(|m| m.iter().map(|(k, v)| (k.clone(), path(v))).collect()).unwrap_or_default(),
                stubs: s["stubs"].as_array().map(|a| a.iter().filter_map(|x| x.as_str().map(str::to_string)).collect()).unwrap_or_default(),
            });
            let defines = opts["defines"].as_object().map(|m| m.iter().map(|(k, v)| (k.clone(), v.as_str().unwrap_or("").to_string())).collect()).unwrap_or_default();
            let edit = opts.get("edit").filter(|e| e.is_object()).map(|e| super::edit::Edit {
                dir: path(&e["dir"]),
                js: path(&e["js"]),
                work: std::path::absolute(&entry).ok().and_then(|p| p.parent().map(|d| d.to_path_buf())).unwrap_or_default(),
            });
            let o = super::Options { z3: opts["z3"].as_bool().unwrap_or(false), script, defines, edit };
            match super::bundle_with(std::path::Path::new(&entry), o) {
                Ok(b) => serde_json::json!({"js": b.js, "css": b.css}),
                Err(e) => serde_json::json!({"error": e}),
            }
        })
        .unwrap_or_else(|_| serde_json::json!({"error": "the bundler panicked"}));
        CString::new(reply.to_string()).map(CString::into_raw).unwrap_or(std::ptr::null_mut())
    }

    /// # Safety
    /// `s` came from raea_bundle (or is null).
    #[no_mangle]
    pub unsafe extern "C" fn raea_bundle_free(s: *mut c_char) {
        if !s.is_null() {
            drop(unsafe { CString::from_raw(s) });
        }
    }
}
