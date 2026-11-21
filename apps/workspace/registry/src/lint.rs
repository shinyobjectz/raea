//! What in a project isn't connected to raea's project system (docs/projects.feature PJ8), each with its file and line,
//! what's wrong and the fix; `raea doctor --fix` applies the ones it can (`apply`).
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use raea_core_facts::{Finding, finding};

use super::{File, How, Prop, Registry, is_path, resolve, stem};

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Level {
    /// not connected: something reads it wrong or not at all
    Warn,
    /// said so the person knows (a size only the render knows), not wrong
    Info,
}

/// What `--fix` does.
#[derive(Debug, Clone)]
pub enum Fix {
    /// registers a default export: an import line at the top (when one is missing) and a registerRoot at the end
    Register { file: PathBuf, import: Option<String>, root: String },
}

#[derive(Debug, Clone)]
pub struct Lint {
    /// relative to the scanned folder ("" for the folder itself)
    pub file: String,
    pub line: usize,
    pub rule: &'static str,
    pub level: Level,
    pub says: String,
    pub fix: String,
    pub action: Option<Fix>,
}

impl Lint {
    /// As raea's one Finding (docs/direction.feature DR3): about its file, keyed file:line; a warn, or a note for
    /// what only the render knows. A fix `raea doctor --fix` applies starts by saying so.
    pub fn finding(&self) -> Finding {
        let level = if self.level == Level::Warn { finding::Level::Warn } else { finding::Level::Note };
        let name = if self.file.is_empty() { "." } else { self.file.as_str() };
        let key = (self.line > 0).then(|| format!("{name}:{}", self.line));
        let fix = if self.action.is_some() && !self.fix.starts_with("raea doctor --fix") { format!("raea doctor --fix does it: {}", self.fix) } else { self.fix.clone() };
        Finding::new(self.rule, level, &self.says, "raea project scan").about(name, key).fix(fix)
    }

    /// file:line: what (the fix)
    pub fn line(&self) -> String {
        let at = if self.file.is_empty() { ".".to_string() } else if self.line > 0 { format!("{}:{}", self.file, self.line) } else { self.file.clone() };
        format!("{at}: {} ({})", self.says, self.fix)
    }
}

/// A path with the home folder as ~, as a person reads it.
fn tilde(p: &Path) -> String {
    match raea_io_home::user_home() {
        Some(h) if p.starts_with(&h) => format!("~/{}", p.strip_prefix(&h).unwrap_or(p).display()),
        _ => p.display().to_string(),
    }
}

fn warn(file: &str, line: usize, rule: &'static str, says: String, fix: impl Into<String>) -> Lint {
    Lint { file: file.into(), line, rule, level: Level::Warn, says, fix: fix.into(), action: None }
}

pub(super) fn lint(reg: &Registry, files: &[File], used_by: &BTreeMap<String, BTreeSet<String>>) -> Vec<Lint> {
    let mut out = Vec::new();
    if !reg.project && !reg.compositions.is_empty() {
        out.push(warn("", 0, "not-a-project", "this folder isn't in a raea project (no .raea/project at or above it)".into(), "raea project new NAME and move the files in, or raea workspace adopt the folder"));
    }
    let comp_files: BTreeSet<&str> = reg.compositions.iter().map(|c| c.file.as_str()).collect();
    // the workspace's shared/, which a project reaches as @shared/
    let shared = super::up(&reg.dir, ".raea/workspace").map(|w| {
        let s = w.join("shared");
        dunce::canonicalize(&s).unwrap_or(s)
    });

    for f in files {
        if let Some(why) = &f.facts.broken {
            out.push(warn(&f.rel, 1, "broken", format!("doesn't parse: {why}"), "fix the syntax: nothing reads a file that doesn't parse"));
            continue;
        }
        // a registerRoot whose compositions a helper makes (`<>{shotComps(SHOTS)}</>`): registered, but not listed here
        if f.facts.roots && f.facts.registered.is_empty() {
            out.push(Lint { file: f.rel.clone(), line: 1, rule: "computed-root", level: Level::Info, says: "registers its root through code the scan doesn't follow (a helper that makes its <Composition>s), so its compositions aren't listed".into(), fix: "known when it renders; write each <Composition> inside registerRoot to have them listed before".into(), action: None });
        }
        // a .tsx that is neither a composition nor reached from one
        if f.rel.ends_with(".tsx") && !f.facts.roots && !comp_files.contains(f.rel.as_str()) && !used_by.contains_key(&f.rel) {
            out.push(warn(&f.rel, 1, "orphan", "isn't a composition, and no composition imports it".into(), "register it (a default export or a registerRoot <Composition>), import it from a composition, or delete it"));
        }
        for (spec, line) in f.facts.imports.iter().filter(|(s, _)| is_path(s)) {
            match resolve(&f.path, spec) {
                None => out.push(warn(&f.rel, *line, "unresolved", format!("imports {spec}, which isn't there"), "fix the path, or add the file")),
                // the workspace's shared/ by its name is how a project reaches it (docs/layout.feature LY2)
                Some(_) if spec.starts_with(super::SHARED) => {}
                Some(to) if !to.starts_with(&reg.dir) => {
                    let in_shared = shared.as_ref().and_then(|s| to.strip_prefix(s).ok());
                    out.push(warn(
                        &f.rel,
                        *line,
                        "outside",
                        format!("imports {} from outside the project", tilde(&to)),
                        &match in_shared {
                            Some(r) => format!("import it by the workspace's name for it: \"@shared/{}\"", r.with_extension("").to_string_lossy().replace(std::path::MAIN_SEPARATOR, "/")),
                            None => "move it to the workspace's shared/ and import it as @shared/<kind>/<name>, or copy it into the project (raea component add, for raea's sets)".into(),
                        },
                    ))
                }
                _ => {}
            }
        }
    }

    // where each kind is kept (docs/layout.feature LY3): a project in the layout has a folder per kind, and a file
    // elsewhere is said once with the folder it belongs in; a project from before the layout is said once, as a whole
    if reg.project && reg.dir.join(".raea/project").is_file() {
        let laid = reg.dir.join("compositions").is_dir() || reg.dir.join("components").is_dir();
        let home = |rel: &str, kind: &str| rel.starts_with(&format!("{kind}/"));
        let components: BTreeSet<&str> = used_by.keys().map(String::as_str).filter(|f| f.ends_with(".tsx") && !comp_files.contains(f)).collect();
        let stray: Vec<(&str, &str)> = comp_files
            .iter()
            .filter(|f| !home(f, "compositions"))
            .map(|f| (*f, "compositions"))
            .chain(components.iter().filter(|f| !home(f, "components")).map(|f| (*f, "components")))
            .collect();
        if !laid && !stray.is_empty() {
            out.push(Lint { file: String::new(), line: 0, rule: "old-layout", level: Level::Info, says: format!("keeps {} files loose, from before compositions/ and components/", stray.len()), fix: "`raea project tidy --dry-run` shows where each goes, then `raea project tidy` moves them".into(), action: None });
        } else {
            for (f, kind) in stray {
                out.push(Lint { file: f.to_string(), line: 1, rule: "misplaced", level: Level::Info, says: format!("a {} outside {kind}/", &kind[..kind.len() - 1]), fix: format!("move it into {kind}/ (`raea project tidy` moves it and its imports)"), action: None });
            }
        }
    }

    // one id, two compositions (a composition's versions share theirs)
    let mut by_id: BTreeMap<&str, BTreeSet<String>> = BTreeMap::new();
    for c in &reg.compositions {
        by_id.entry(c.id.as_str()).or_default().insert(stem(&c.file));
    }
    for c in &reg.compositions {
        let names = &by_id[c.id.as_str()];
        if names.len() > 1 {
            let others: Vec<&str> = names.iter().map(String::as_str).filter(|n| *n != stem(&c.file)).collect();
            let who = match others.as_slice() {
                [one] => format!("{one} does"),
                [rest @ .., last] => format!("{} and {last} do", rest.join(", ")),
                [] => continue,
            };
            out.push(warn(&c.file, c.line, "duplicate-id", format!("registers \"{}\", as {who}", c.id), "give one of them its own id: `raea render --composition ID` picks by it"));
        }
    }

    for c in &reg.compositions {
        let f = files.iter().find(|f| f.rel == c.file).expect("a composition's file was read");
        match c.how {
            How::Registered => {
                let computed: Vec<String> = [("width", &c.width), ("height", &c.height), ("fps", &c.fps), ("durationInFrames", &c.frames)]
                    .into_iter()
                    .filter_map(|(k, v)| match v {
                        Some(Prop::Computed(e)) => Some(format!("{k} = {e}")),
                        None => Some(format!("{k} missing")),
                        _ => None,
                    })
                    .collect();
                if !computed.is_empty() {
                    out.push(Lint { file: c.file.clone(), line: c.line, rule: "computed", level: Level::Info, says: format!("<Composition id=\"{}\">: {}", c.id, computed.join(", ")), fix: "known when it renders; write them out to have them listed before".into(), action: None });
                }
            }
            How::Default => {
                let known = [&c.width, &c.height, &c.fps, &c.frames].iter().all(|v| matches!(v, Some(Prop::Num(_))));
                let action = match (&c.component, known) {
                    (Some(name), true) => {
                        let n = |v: &Option<Prop>| match v {
                            Some(Prop::Num(n)) => *n,
                            _ => 0.0,
                        };
                        let missing: Vec<&str> = ["Composition", "registerRoot"].into_iter().filter(|x| !f.facts.raea_names.iter().any(|h| h == x)).collect();
                        Some(Fix::Register {
                            file: f.path.clone(),
                            import: (!missing.is_empty()).then(|| format!("import {{ {} }} from \"raea\";\n", missing.join(", "))),
                            root: format!("\nregisterRoot(() => <Composition id=\"{}\" component={{{name}}} width={{{}}} height={{{}}} fps={{{}}} durationInFrames={{{}}} />);\n", c.id, n(&c.width), n(&c.height), n(&c.fps), n(&c.frames)),
                        })
                    }
                    _ => None,
                };
                let fix = match (&c.component, known) {
                    (Some(_), true) => "raea doctor --fix registers it with its last render's size, fps and length".to_string(),
                    (None, _) => "name the default export (export default function Name) and register it: registerRoot(() => <Composition id component width height fps durationInFrames />)".to_string(),
                    (Some(_), false) => "render it once (raea render), then raea doctor --fix registers it with that size, fps and length; or write its registerRoot".to_string(),
                };
                out.push(Lint { file: c.file.clone(), line: 1, rule: "unregistered", level: Level::Info, says: format!("registered from its default export as \"{}\": raea renders it at 1080x1920, 30 fps unless told", c.id), fix, action });
            }
        }
    }

    out
}

/// Applies a fix; what it did, for doctor's output.
pub fn apply(fix: &Fix) -> Result<String, String> {
    match fix {
        Fix::Register { file, import, root } => {
            let src = std::fs::read_to_string(file).map_err(|e| format!("{}: {e}", file.display()))?;
            let text = format!("{}{}{root}", import.clone().unwrap_or_default(), src.trim_end_matches('\n'));
            std::fs::write(file, text).map_err(|e| format!("{}: {e}", file.display()))?;
            Ok(format!("registered {}", file.display()))
        }
    }
}
