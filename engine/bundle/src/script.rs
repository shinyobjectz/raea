//! A script's bundle (raea-motion's bundle_entry: tests, renders' helper scripts, the shell's), with the engine's
//! resolution on top of Rolldown's own, as react/js/bundle.ts did on bun's: `@raea/motion` (and its subpaths) is the
//! SDK's source in sdk/motion/src; `bun:test`, `vitest` and `@jest/globals` are the engine's test API (sdk/react/js/shims/
//! test.ts); `@raea/browser` and `@raea/shell` are raea's own; Node's built-ins raea carries (sdk/react/js/shims/node) resolve
//! there, one listed as a stub is a module that throws when used, any other fails the bundle with its importer named;
//! the packages the engine carries (react, mediabunny, bevy-react...) resolve to its copies from anywhere, so a bundle
//! has one of each, and any other bare import the importer's own folders can't satisfy falls back to them.
use rolldown_common::ModuleType;
use rolldown_plugin::{
    HookLoadArgs, HookLoadOutput, HookLoadReturn, HookResolveIdArgs, HookResolveIdOutput, HookResolveIdReturn, HookUsage, Plugin, PluginContext,
    SharedLoadPluginContext,
};
use std::borrow::Cow;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// What a script bundle resolves with
#[derive(Debug, Clone, Default)]
pub struct Script {
    /// sdk/react/js: the test shim, the Node shims and the engine's node_modules
    pub js: PathBuf,
    /// sdk/motion/src
    pub motion: PathBuf,
    /// engine/browser/ts/index.ts and sdk/shell/src/index.ts
    pub browser: PathBuf,
    pub shell: PathBuf,
    /// `--alias SPEC=PATH`
    pub aliases: HashMap<String, PathBuf>,
    /// `--node-stub a,b`
    pub stubs: Vec<String>,
}

const SUBPATHS: &[(&str, &str)] = &[("", "index.tsx"), ("test", "test.tsx"), ("3d", "3d.tsx"), ("scene", "scene.tsx"), ("room", "room.tsx"), ("daylight", "daylight.tsx"), ("ecs", "ecs.ts"), ("native", "native.ts"), ("codecs", "codecs.ts"), ("ml", "ml.ts"), ("speech", "speech.tsx")];
pub(crate) const SINGLETONS: &[&str] = &["react", "react-dom", "react-reconciler", "scheduler", "mediabunny", "bevy-react"];
const NODE: &[&str] = &[
    "assert", "buffer", "child_process", "crypto", "events", "fs", "fs/promises", "os", "path", "process", "stream", "url", "util", "worker_threads", "zlib", "module",
    "net", "http", "https", "tty", "readline", "string_decoder", "perf_hooks", "async_hooks", "vm",
];
const STUB: &str = "\0raea-node-stub:";

#[derive(Debug)]
pub struct ScriptResolve(pub Script);

impl ScriptResolve {
    /// `spec` as the engine's node_modules resolves it (Rolldown's resolver, from a file in sdk/react/js)
    async fn from_engine(&self, ctx: &PluginContext, spec: &str) -> anyhow::Result<Option<HookResolveIdOutput>> {
        let importer = self.0.js.join("_raea.js");
        let opts = rolldown_plugin::PluginContextResolveOptions { skip_self: true, ..Default::default() };
        match ctx.resolve(spec, Some(&importer.to_string_lossy()), Some(opts)).await? {
            Ok(r) => Ok(Some(HookResolveIdOutput::from_id(r.id.as_str()))),
            Err(_) => Ok(None),
        }
    }
}

fn path_id(p: &Path) -> Option<HookResolveIdOutput> {
    Some(HookResolveIdOutput::from_id(p.to_string_lossy().into_owned()))
}

impl Plugin for ScriptResolve {
    fn name(&self) -> Cow<'static, str> {
        Cow::Borrowed("raea:script")
    }

    fn register_hook_usage(&self) -> HookUsage {
        HookUsage::ResolveId | HookUsage::Load
    }

    async fn resolve_id(&self, ctx: &PluginContext, args: &HookResolveIdArgs<'_>) -> HookResolveIdReturn {
        let s = &self.0;
        let spec = args.specifier;
        let importer = args.importer.unwrap_or("(the entry)");
        if let Some(p) = s.aliases.get(spec) {
            return Ok(path_id(p));
        }
        match spec {
            "bun:test" | "vitest" | "@jest/globals" => return Ok(path_id(&s.js.join("shims/test.ts"))),
            "@raea/browser" => return Ok(path_id(&s.browser)),
            "@raea/shell" => return Ok(path_id(&s.shell)),
            _ => {}
        }
        if let Some(rest) = spec.strip_prefix("@raea/motion").filter(|r| r.is_empty() || r.starts_with('/')) {
            let sub = rest.trim_start_matches('/');
            let Some((_, f)) = SUBPATHS.iter().find(|(k, _)| *k == sub) else {
                anyhow::bail!("@raea/motion has no subpath \"{sub}\" (imported by {importer})");
            };
            return Ok(path_id(&s.motion.join(f)));
        }
        let node_like = spec.strip_prefix("node:").unwrap_or(spec);
        if node_like.chars().all(|c| c.is_ascii_lowercase() || c == '_' || c == '/') && !node_like.is_empty() {
            let name = node_like;
            if NODE.contains(&name) {
                let shim = s.js.join("shims/node").join(format!("{name}.ts"));
                if shim.is_file() {
                    return Ok(path_id(&shim));
                }
                if s.stubs.iter().any(|x| x == name) {
                    return Ok(Some(HookResolveIdOutput::from_id(format!("{STUB}{name}"))));
                }
                anyhow::bail!("\"{spec}\" is a Node built-in, which raea's V8 doesn't have (imported by {importer}); use the package's browser build, or --node-stub {name}");
            }
        }
        // packages the engine itself carries resolve to its copies from anywhere (one React, one mediabunny); other
        // bare imports resolve from the importer's own folders, then fall back to the engine's node_modules
        let bare = spec.starts_with('@') || spec.starts_with(|c: char| c.is_ascii_lowercase());
        let (Some(imp), true) = (args.importer, bare) else { return Ok(None) };
        if spec.starts_with("@raea/") {
            return Ok(None);
        }
        let pkg = if spec.starts_with('@') { spec.splitn(3, '/').take(2).collect::<Vec<_>>().join("/") } else { spec.split('/').next().unwrap_or(spec).to_string() };
        let fallback = s.js.join("node_modules");
        if SINGLETONS.contains(&pkg.as_str()) && fallback.join(&pkg).exists() {
            return self.from_engine(ctx, spec).await;
        }
        let mut d = Path::new(imp).parent();
        while let Some(here) = d {
            if here.join("node_modules").join(&pkg).exists() {
                return Ok(None);
            }
            d = here.parent();
        }
        if fallback.join(&pkg).exists() {
            return self.from_engine(ctx, spec).await;
        }
        Ok(None)
    }

    async fn load(&self, _ctx: SharedLoadPluginContext, args: &HookLoadArgs<'_>) -> HookLoadReturn {
        let Some(name) = args.id.strip_prefix(STUB) else { return Ok(None) };
        let code = format!(
            "const m = new Proxy({{}}, {{ get: (_t, k) => k === \"__esModule\" ? true : k === \"default\" ? m : () => {{ throw new Error(\"node:{name}.\" + String(k) + \" isn't available in raea's V8\"); }} }}); export default m;\n"
        );
        Ok(Some(HookLoadOutput { code: code.into(), module_type: Some(ModuleType::Js), ..Default::default() }))
    }
}
