//! A composition's bundle (raea-motion's bundle_fresh), with the edit's own packages: its sources are copied into a
//! work folder whose node_modules is the engine's, so `import { motion } from "motion/react"` found nothing the edit
//! had installed. A bare import from the edit's sources resolves from the edit's own folder first (its node_modules,
//! then its parents', as Node and Vite would), then the engine's; the packages the engine carries (react, mediabunny,
//! bevy-react...) resolve to its copies from any package, so a bundle has one React however the edit installed its own.
use rolldown_plugin::{HookResolveIdArgs, HookResolveIdOutput, HookResolveIdReturn, HookUsage, Plugin, PluginContext, PluginContextResolveOptions};
use std::borrow::Cow;
use std::path::{Path, PathBuf};

/// Where a composition's packages come from
#[derive(Debug, Clone, Default)]
pub struct Edit {
    /// the edit's own folder (not the work folder's copy)
    pub dir: PathBuf,
    /// sdk/react/js: the engine's node_modules
    pub js: PathBuf,
    /// the work folder (the entry's): the edit's copied sources and the SDK's
    pub work: PathBuf,
}

#[derive(Debug)]
pub struct EditResolve(Edit);

impl EditResolve {
    /// its folders as the resolver names modules: symlinks resolved (the Mac's temp folder is /var, a link to /private/var)
    pub fn new(e: Edit) -> Self {
        let real = |p: PathBuf| p.canonicalize().unwrap_or(p);
        EditResolve(Edit { dir: real(e.dir), js: real(e.js), work: real(e.work) })
    }
}

/// `spec` resolved by Rolldown's resolver as if imported from a file in `dir`
async fn resolve_from(ctx: &PluginContext, spec: &str, dir: &Path) -> anyhow::Result<Option<HookResolveIdOutput>> {
    let importer = dir.join("_raea.js");
    let opts = PluginContextResolveOptions { skip_self: true, ..Default::default() };
    match ctx.resolve(spec, Some(&importer.to_string_lossy()), Some(opts)).await? {
        Ok(r) => Ok(Some(HookResolveIdOutput::from_id(r.id.as_str()))),
        Err(_) => Ok(None),
    }
}

impl Plugin for EditResolve {
    fn name(&self) -> Cow<'static, str> {
        Cow::Borrowed("raea:edit")
    }

    fn register_hook_usage(&self) -> HookUsage {
        HookUsage::ResolveId
    }

    async fn resolve_id(&self, ctx: &PluginContext, args: &HookResolveIdArgs<'_>) -> HookResolveIdReturn {
        let e = &self.0;
        let spec = args.specifier;
        let Some(imp) = args.importer else { return Ok(None) };
        let bare = spec.starts_with('@') || spec.starts_with(|c: char| c.is_ascii_lowercase());
        if !bare || spec.starts_with("@raea/") || spec.contains(':') {
            return Ok(None);
        }
        let pkg = if spec.starts_with('@') { spec.splitn(3, '/').take(2).collect::<Vec<_>>().join("/") } else { spec.split('/').next().unwrap_or(spec).to_string() };
        let engine = crate::script::SINGLETONS.contains(&pkg.as_str()) && e.js.join("node_modules").join(&pkg).exists();
        let from_edit = Path::new(imp).starts_with(&e.work) && !imp.contains("/node_modules/");
        match (from_edit, engine) {
            // the edit's sources: the engine's copy by the work folder's own resolution (its tsconfig paths point a live
            // bundle's react at the live runtime)
            (true, true) => Ok(None),
            (true, false) => resolve_from(ctx, spec, &e.dir).await,
            // a package (the edit's or the engine's) importing one the engine carries: the engine's
            (false, true) => resolve_from(ctx, spec, &e.js).await,
            (false, false) => Ok(None),
        }
    }
}
