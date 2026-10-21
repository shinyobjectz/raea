//! raea-modules: the agent's TypeScript and TSX loaded in process, for the engine's V8 module API.
//!
//! The engine (raea-react, rusty_v8 152) runs a composition and the agent's components without bun: its V8 resolve
//! callback asks this crate to `resolve` an import and `load` the module, then instantiates what comes back. Nothing
//! here touches V8. TypeScript, TSX and JSX are transpiled by deno_ast (swc), with React's automatic JSX runtime (as
//! bun's). The shared libraries (React, @raea/motion) stay one prebuilt bundle: the engine runs it first and fills
//! `globalThis.__raea_modules[name]` with each library's exports; an import of that name loads a small module this crate
//! writes, which re-exports them. This is what authoring on the iPhone needs, where there is no bun.
//!
//! What it refuses, with a sentence the agent can act on: a file outside the space, a file that isn't there, a bare
//! name the prebuilt bundle doesn't serve, a file it can't parse.
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::path::{Component, Path, PathBuf};

use deno_ast::{EmitOptions, JsxAutomaticOptions, JsxRuntime, MediaType, ModuleSpecifier, ParseParams, SourceMapOption, TranspileModuleOptions, TranspileOptions};

/// A library the prebuilt bundle serves: the bare specifier and the names it exports ("default" included when it has one).
#[derive(Clone, Debug)]
pub struct Prebuilt {
    pub specifier: String,
    pub exports: Vec<String>,
}

/// What an import resolved to.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum ModuleId {
    /// a file inside the space
    File(PathBuf),
    /// a library from the prebuilt bundle
    Prebuilt(String),
}

impl ModuleId {
    /// the URL the engine names the module by (file:///... or raea:prebuilt/<specifier>)
    pub fn url(&self) -> String {
        match self {
            ModuleId::File(p) => format!("file://{}", p.display()),
            ModuleId::Prebuilt(s) => format!("raea:prebuilt/{s}"),
        }
    }
}

/// A module ready for V8: an ES module's source and, when transpiled, its source map.
#[derive(Clone, Debug)]
pub struct Module {
    pub url: String,
    pub source: String,
    pub source_map: Option<String>,
}

/// Why an import or a load can't be done: the sentence for the agent, and the parts the engine turns into an action
/// row (an agent session, 2026-10-16): what kind of refusal, the import as written, and the module that made it.
#[derive(Clone, Debug, PartialEq)]
pub struct Refusal {
    pub message: String,
    pub kind: RefusalKind,
    /// the import as written (none for the space itself)
    pub specifier: Option<String>,
    /// the module that imported it, as its url (none for a load)
    pub referrer: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RefusalKind {
    /// the space folder can't be opened
    Space,
    /// a bare name the prebuilt bundle doesn't serve
    UnknownLibrary,
    /// a path outside the space
    OutsideSpace,
    /// a path that isn't there
    Missing,
    /// a file that can't be read
    Unreadable,
    /// a file that doesn't parse or transpile
    Syntax,
    /// a file of a kind the engine doesn't load
    Unsupported,
}

impl Refusal {
    fn new(kind: RefusalKind, message: String, specifier: Option<&str>, referrer: Option<&str>) -> Refusal {
        Refusal { message, kind, specifier: specifier.map(String::from), referrer: referrer.map(String::from) }
    }
}

impl std::fmt::Display for Refusal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

const EXTS: [&str; 5] = ["tsx", "ts", "jsx", "js", "mjs"];
const MEDIA: [&str; 12] = ["mp4", "mov", "m4v", "webm", "m4a", "mp3", "wav", "aac", "png", "jpg", "jpeg", "svg"];

pub struct Loader {
    root: PathBuf,
    prebuilt: HashMap<String, Prebuilt>,
    cache: HashMap<PathBuf, (u64, Module)>,
    /// how many files were transpiled (not served from the cache): for the engine's log and the tests
    pub transpiled: usize,
}

fn lexical(p: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for c in p.components() {
        match c {
            Component::ParentDir => {
                out.pop();
            }
            Component::CurDir => {}
            c => out.push(c),
        }
    }
    out
}

impl Loader {
    /// `root` is the space folder; `prebuilt` the libraries the prebuilt bundle serves.
    pub fn new(root: &Path, prebuilt: Vec<Prebuilt>) -> Result<Loader, Refusal> {
        let root = root.canonicalize().map_err(|e| Refusal::new(RefusalKind::Space, format!("the space {} can't be opened: {e}", root.display()), None, None))?;
        Ok(Loader { root, prebuilt: prebuilt.into_iter().map(|p| (p.specifier.clone(), p)).collect(), cache: HashMap::new(), transpiled: 0 })
    }

    /// An import (`specifier`) from a module (`referrer`: a ModuleId's url, or a path for the entry) to the module it names.
    pub fn resolve(&self, specifier: &str, referrer: &str) -> Result<ModuleId, Refusal> {
        if !(specifier.starts_with("./") || specifier.starts_with("../") || specifier.starts_with('/') || specifier.starts_with("file://")) {
            if self.prebuilt.contains_key(specifier) {
                return Ok(ModuleId::Prebuilt(specifier.to_string()));
            }
            let mut have: Vec<&str> = self.prebuilt.keys().map(String::as_str).collect();
            have.sort();
            return Err(Refusal::new(RefusalKind::UnknownLibrary, format!("`{specifier}` (imported by {referrer}) isn't a file in the space or a library the engine has: it has {}", have.join(", ")), Some(specifier), Some(referrer)));
        }
        let from = referrer.strip_prefix("file://").unwrap_or(referrer);
        // the importing file's folder by its real path (macOS's /var is /private/var; spaces link folders in), and the
        // import joined to it without following links, so a linked media/ inside the space stays inside it
        let base = if referrer.starts_with("raea:") { self.root.clone() } else { Path::new(from).parent().map(Path::to_path_buf).unwrap_or_else(|| self.root.clone()) };
        let base = base.canonicalize().unwrap_or(base);
        let raw = specifier.strip_prefix("file://").unwrap_or(specifier);
        let want = lexical(&if raw.starts_with('/') { PathBuf::from(raw) } else { base.join(raw) });
        if !want.starts_with(&self.root) {
            return Err(Refusal::new(RefusalKind::OutsideSpace, format!("`{specifier}` (imported by {referrer}) is outside the space: import files inside {}", self.root.display()), Some(specifier), Some(referrer)));
        }
        // as written, then with each extension, then as a folder's index
        let mut tries = vec![want.clone()];
        tries.extend(EXTS.iter().map(|e| want.with_extension(e)));
        tries.extend(EXTS.iter().map(|e| want.join(format!("index.{e}"))));
        tries
            .into_iter()
            .find(|p| p.is_file())
            .map(ModuleId::File)
            .ok_or_else(|| Refusal::new(RefusalKind::Missing, format!("`{specifier}` (imported by {referrer}) isn't there: no {} (tried .tsx, .ts, .jsx, .js, .mjs)", want.display()), Some(specifier), Some(referrer)))
    }

    /// The module's source for V8: transpiled when it's TypeScript or JSX, re-exports for a prebuilt library, a path
    /// for media. Files are cached by their content, so an edit re-transpiles only what changed.
    pub fn load(&mut self, id: &ModuleId) -> Result<Module, Refusal> {
        match id {
            ModuleId::Prebuilt(s) => {
                let p = &self.prebuilt[s];
                let mut src = format!("const m = globalThis.__raea_modules[{}];\n", json(s));
                let named: Vec<&String> = p.exports.iter().filter(|e| *e != "default").collect();
                if p.exports.iter().any(|e| e == "default") {
                    src += "export default m.default;\n";
                }
                if !named.is_empty() {
                    src += &format!("export const {{ {} }} = m;\n", named.iter().map(|s| s.as_str()).collect::<Vec<_>>().join(", "));
                }
                Ok(Module { url: id.url(), source: src, source_map: None })
            }
            ModuleId::File(path) => {
                let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("").to_lowercase();
                if MEDIA.contains(&ext.as_str()) {
                    // a media import names its file, as the motion SDK's golden cases import their takes
                    return Ok(Module { url: id.url(), source: format!("export default {};\n", json(&path.display().to_string())), source_map: None });
                }
                let text = std::fs::read_to_string(path).map_err(|e| Refusal::new(RefusalKind::Unreadable, format!("{} can't be read: {e}", path.display()), Some(&id.url()), None))?;
                let mut h = std::collections::hash_map::DefaultHasher::new();
                text.hash(&mut h);
                let key = h.finish();
                if let Some((k, m)) = self.cache.get(path) {
                    if *k == key {
                        return Ok(m.clone());
                    }
                }
                let module = match ext.as_str() {
                    "json" => Module { url: id.url(), source: format!("export default {};\n", text.trim()), source_map: None },
                    "js" | "mjs" => Module { url: id.url(), source: text, source_map: None },
                    "ts" | "tsx" | "jsx" | "mts" => {
                        self.transpiled += 1;
                        transpile(path, text)?
                    }
                    _ => return Err(Refusal::new(RefusalKind::Unsupported, format!("{} isn't a module the engine loads (.tsx, .ts, .jsx, .js, .mjs, .json, or media)", path.display()), Some(&id.url()), None)),
                };
                self.cache.insert(path.clone(), (key, module.clone()));
                Ok(module)
            }
        }
    }
}

/// TypeScript, TSX or JSX to an ES module, with React's automatic JSX runtime (imports from react/jsx-runtime, as bun's)
pub fn transpile(path: &Path, text: String) -> Result<Module, Refusal> {
    let spec = ModuleSpecifier::from_file_path(path).map_err(|_| Refusal::new(RefusalKind::Unsupported, format!("{} isn't an absolute path", path.display()), None, None))?;
    let parsed = deno_ast::parse_module(ParseParams { specifier: spec.clone(), text: text.into(), media_type: MediaType::from_path(path), capture_tokens: false, scope_analysis: false, maybe_syntax: None })
        .map_err(|e| Refusal::new(RefusalKind::Syntax, format!("{} can't be parsed: {e}", path.display()), Some(spec.as_str()), None))?;
    let out = parsed
        .transpile(
            &TranspileOptions {
                jsx: Some(JsxRuntime::Automatic(JsxAutomaticOptions { development: false, import_source: Some("react".into()) })),
                imports_not_used_as_values: deno_ast::ImportsNotUsedAsValues::Remove,
                ..Default::default()
            },
            &TranspileModuleOptions { module_kind: None },
            &EmitOptions { source_map: SourceMapOption::Separate, inline_sources: true, ..Default::default() },
        )
        .map_err(|e| Refusal::new(RefusalKind::Syntax, format!("{} can't be transpiled: {e}", path.display()), Some(spec.as_str()), None))?
        .into_source();
    Ok(Module { url: spec.to_string(), source: out.text, source_map: out.source_map.map(|m| String::from_utf8_lossy(&m.into_bytes()).into_owned()) })
}

/// A module (JavaScript or TypeScript, by `url`'s extension) as CommonJS: its imports become `require(...)`, its exports
/// properties of `exports`, `import()` a promise of `require`. What JavaScriptCore runs, as its C API loads no ES modules
/// (blitz-v8-script's runtime wraps it as `function (require, module, exports) { ... }`). Top-level await doesn't fit.
pub fn to_commonjs(url: &str, text: String) -> Result<String, Refusal> {
    let spec = ModuleSpecifier::parse(url).map_err(|e| Refusal::new(RefusalKind::Unsupported, format!("{url}: {e}"), None, None))?;
    let media = match MediaType::from_specifier(&spec) {
        MediaType::Unknown => MediaType::JavaScript,
        m => m,
    };
    let parsed = deno_ast::parse_module(ParseParams { specifier: spec.clone(), text: text.into(), media_type: media, capture_tokens: false, scope_analysis: false, maybe_syntax: None })
        .map_err(|e| Refusal::new(RefusalKind::Syntax, format!("{url} can't be parsed: {e}"), Some(spec.as_str()), None))?;
    let out = parsed
        .transpile(
            &TranspileOptions { imports_not_used_as_values: deno_ast::ImportsNotUsedAsValues::Remove, ..Default::default() },
            &TranspileModuleOptions { module_kind: Some(deno_ast::ModuleKind::Cjs) },
            &EmitOptions { source_map: SourceMapOption::None, ..Default::default() },
        )
        .map_err(|e| Refusal::new(RefusalKind::Syntax, format!("{url} can't be transpiled: {e}"), Some(spec.as_str()), None))?
        .into_source();
    Ok(out.text)
}

fn json(s: &str) -> String {
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

/// `text` (a JavaScript module at `url`) with each import specifier `map` gives a new one for replaced: static imports,
/// re-exports and `import("...")` of a string. What JavaScriptCore's module loader needs, as it resolves relative and
/// absolute URLs itself but refuses bare names (`react`, an import map's keys) before asking its delegate.
pub fn rewrite_specifiers(url: &str, text: &str, mut map: impl FnMut(&str) -> Option<String>) -> Result<String, Refusal> {
    use deno_ast::swc::ast::{CallExpr, Callee, ExportAll, Expr, ImportDecl, Lit, NamedExport, Str};
    use deno_ast::swc::ecma_visit::{Visit, VisitWith};
    let spec = ModuleSpecifier::parse(url).unwrap_or_else(|_| ModuleSpecifier::parse("file:///inline.js").unwrap());
    let parsed = deno_ast::parse_module(ParseParams { specifier: spec.clone(), text: text.into(), media_type: MediaType::JavaScript, capture_tokens: false, scope_analysis: false, maybe_syntax: None })
        .map_err(|e| Refusal::new(RefusalKind::Syntax, format!("{url} can't be parsed: {e}"), Some(spec.as_str()), None))?;
    struct V(Vec<(u32, u32, String)>);
    impl V {
        fn str(&mut self, s: &Str) {
            self.0.push((s.span.lo.0, s.span.hi.0, s.value.to_string_lossy().into_owned()));
        }
    }
    impl Visit for V {
        fn visit_import_decl(&mut self, n: &ImportDecl) {
            self.str(&n.src);
        }
        fn visit_export_all(&mut self, n: &ExportAll) {
            self.str(&n.src);
        }
        fn visit_named_export(&mut self, n: &NamedExport) {
            if let Some(s) = &n.src {
                self.str(s);
            }
        }
        fn visit_call_expr(&mut self, n: &CallExpr) {
            if let (Callee::Import(_), Some(a)) = (&n.callee, n.args.first()) {
                if let Expr::Lit(Lit::Str(s)) = &*a.expr {
                    self.str(s);
                }
            }
            n.visit_children_with(self);
        }
    }
    let mut v = V(Vec::new());
    parsed.program_ref().visit_with(&mut v);
    let base = parsed.text_info_lazy().range().start.as_byte_pos().0;
    let mut out = text.to_string();
    v.0.sort_by_key(|s| std::cmp::Reverse(s.0));
    for (lo, hi, value) in v.0 {
        if let Some(new) = map(&value) {
            out.replace_range((lo - base) as usize..(hi - base) as usize, &json(&new));
        }
    }
    Ok(out)
}

/// Every function in a script (declarations, expressions, arrows; methods left out), as byte ranges of `text` with the
/// index of the function enclosing it: what each one's `Function.prototype.toString()` gives back, for telling which
/// functions an edit changed (sdk/react/src/bake.rs: a timeline re-stepped only where a changed component renders).
pub fn function_spans(text: &str) -> Result<Vec<(usize, usize, Option<usize>)>, Refusal> {
    use deno_ast::swc::ast::{ArrowExpr, FnDecl, FnExpr};
    use deno_ast::swc::ecma_visit::{Visit, VisitWith};
    let spec = ModuleSpecifier::parse("file:///bundle.js").unwrap();
    let parsed = deno_ast::parse_module(ParseParams { specifier: spec.clone(), text: text.into(), media_type: MediaType::JavaScript, capture_tokens: false, scope_analysis: false, maybe_syntax: None })
        .map_err(|e| Refusal::new(RefusalKind::Syntax, format!("the bundle can't be parsed: {e}"), Some(spec.as_str()), None))?;
    let base = parsed.text_info_lazy().range().start.as_byte_pos().0;
    struct V {
        base: u32,
        out: Vec<(usize, usize, Option<usize>)>,
        stack: Vec<usize>,
    }
    impl V {
        fn enter(&mut self, lo: u32, hi: u32) -> usize {
            self.out.push(((lo - self.base) as usize, (hi - self.base) as usize, self.stack.last().copied()));
            self.out.len() - 1
        }
    }
    impl Visit for V {
        fn visit_fn_decl(&mut self, n: &FnDecl) {
            let sp = n.function.span;
            let i = self.enter(sp.lo.0, sp.hi.0);
            self.stack.push(i);
            n.function.visit_children_with(self);
            self.stack.pop();
        }
        fn visit_fn_expr(&mut self, n: &FnExpr) {
            let sp = n.function.span;
            let i = self.enter(sp.lo.0, sp.hi.0);
            self.stack.push(i);
            n.function.visit_children_with(self);
            self.stack.pop();
        }
        fn visit_arrow_expr(&mut self, n: &ArrowExpr) {
            let i = self.enter(n.span.lo.0, n.span.hi.0);
            self.stack.push(i);
            n.visit_children_with(self);
            self.stack.pop();
        }
    }
    let mut v = V { base, out: Vec::new(), stack: Vec::new() };
    parsed.program_ref().visit_with(&mut v);
    Ok(v.out)
}

#[cfg(test)]
mod rewrite_tests {
    #[test]
    fn bare_names_rewritten() {
        let src = "// import 'no'\nimport a, { b } from 'three';\nexport * from \"./x.js\";\nexport { c } from 'lib/c';\nconst s = 'three';\nconst m = import('three');\n";
        let out = super::rewrite_specifiers("https://h/a.js", src, |s| (!s.starts_with('.')).then(|| format!("https://cdn/{s}"))).unwrap();
        assert_eq!(out, "// import 'no'\nimport a, { b } from \"https://cdn/three\";\nexport * from \"./x.js\";\nexport { c } from \"https://cdn/lib/c\";\nconst s = 'three';\nconst m = import(\"https://cdn/three\");\n");
    }
}
