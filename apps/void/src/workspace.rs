//! Workspaces: void works in a folder the person picks, the way Claude Code does. The folder is the workspace; its
//! spaces are folders inside it, a light monorepo (each space its own small React + TypeScript project, worked through
//! the `raea` CLI, with nothing shared unless it's put in a shared folder on purpose).
//!
//! ```text
//! <workspace>/
//!   .void/workspace.json     the workspace: {"void": 1, "name", "created", "settings": {}}; its presence marks the folder
//!   .void/terminal.json      the "All spaces" transcript (each space keeps its own beside its files)
//!   AGENTS.md                how the folder is laid out, for an agent working at the root (new workspaces only)
//!   spaces/<slug>/           one space each, as `raea init` lays it out:
//!     space.json               its name, hue, when made and last opened
//!     edit.tsx  cuts.ts  styles.gen.ts  AGENTS.md  components/  media/   (brand/ once `raea brand` runs)
//!     terminal.json            its transcript
//!   brand/                   optional: brand kits every space can use (brand/<name>/brand.css)
//!   shared/components/       optional: components every space can use (`raea add` could look here first)
//!   media/                   optional: a shared footage library (spaces link takes from it)
//! ```
//!
//! "All spaces" is the root itself: opening it points the agent and its shell at <workspace>/, for work across spaces
//! (a shared brand, the shared library, comparing pieces). Spaces are listed most recently opened first, from each
//! space.json, so no order is kept at the root.
//!
//! The legacy layout is the global folder void used before workspaces (~/.raea-void/spaces, or RAEA_VOID_HOME's): its
//! spaces sit directly in the root with no spaces/ folder. It opens as a workspace as it is ("Legacy spaces"):
//! nothing is moved, renamed or written into it beyond what a space always wrote (space.json, terminal.json), and
//! .void/terminal.json once "All spaces" is used there. Any other folder opened (Open folder…) gains only .void/ and
//! spaces/; a new workspace in an empty folder also gets AGENTS.md, brand/, shared/components/ and engine/media/.
//!
//! The recent workspaces are ~/.raea-void/recent.json: [{path, name, opened}], newest first. Evals don't use any of
//! this: `void eval` makes its own spaces under its run folder.

use std::path::{Path, PathBuf};
use std::sync::RwLock;

use serde::{Deserialize, Serialize};

/// The folder spaces live in: `spaces/` under the root, or the root itself (the legacy global folder).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum Layout {
    #[default]
    Spaces,
    Flat,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Meta {
    /// The layout's version.
    #[serde(default = "one")]
    pub void: u32,
    pub name: String,
    /// Unix seconds.
    #[serde(default)]
    pub created: u64,
    /// The workspace's own settings (none yet; the app's are in ~/.raea-void/settings.json).
    #[serde(default)]
    pub settings: serde_json::Value,
}

fn one() -> u32 {
    1
}

#[derive(Debug, Clone, PartialEq)]
pub struct Workspace {
    pub root: PathBuf,
    pub meta: Meta,
    pub layout: Layout,
}

pub fn now() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

/// The name of the folder (its last part), or "Workspace".
fn folder_name(root: &Path) -> String {
    root.file_name().map(|n| n.to_string_lossy().into_owned()).filter(|n| !n.is_empty()).unwrap_or_else(|| "Workspace".into())
}

/// Whether `dir` holds spaces directly (a child folder with a space.json).
fn has_spaces_in(dir: &Path) -> bool {
    std::fs::read_dir(dir).into_iter().flatten().flatten().any(|e| e.path().join("space.json").is_file())
}

/// The global folder void kept every space in before workspaces.
pub fn legacy_root() -> PathBuf {
    crate::spaces::home().join("spaces")
}

pub const LEGACY_NAME: &str = "Legacy spaces";

/// Whether `root` is laid out the legacy way: spaces directly inside it, no workspace.json and no spaces/ folder.
pub fn is_legacy(root: &Path) -> bool {
    !root.join(".void/workspace.json").is_file() && !root.join("spaces").is_dir() && !root.join("objects").is_dir() && has_spaces_in(root)
}

/// raea init's files, from raea/cli (its source of truth): the template's edit.tsx, the AGENTS.md notes from its
/// index.ts, and cuts.ts as `cutsModule([], shape)` writes it for an empty project at the default 1080x1920 at 30.
const CLI_EDIT: &str = include_str!("../../cli/template/edit.tsx");
const CLI_INDEX: &str = include_str!("../../cli/src/index.ts");

/// The AGENTS.md raea init writes (the NOTES template literal in raea/cli/src/index.ts, unescaped).
pub fn cli_notes() -> Option<String> {
    let start = CLI_INDEX.find("const NOTES = `")? + "const NOTES = `".len();
    let len = CLI_INDEX[start..].find("\n`;")? + 1;
    let body = &CLI_INDEX[start..start + len];
    // a template literal: \` and \\ are its only escapes here, and it must interpolate nothing
    if body.contains("${") {
        return None;
    }
    Some(body.replace("\\`", "`").replace("\\\\", "\\"))
}

/// cuts.ts for an empty project (raea/cli's cutsModule with no clips).
pub fn empty_cuts(width: u32, height: u32, fps: u32) -> String {
    format!(
        "// written by `raea cut`: the clips, in order, in each take's own seconds. Re-run raea cut to change them, or edit
// by hand (the edit reads only this file). cuts.json beside it is the same list for raea's checks.


export const edit = {{\"width\":{width},\"height\":{height},\"fps\":{fps}}};
export type Cut = {{ take: string; src: string; from: number; to: number; line: string }};
export const cuts: Cut[] = [

];
/** a clip's length in the edit's frames */
export const frames = (c: Cut) => Math.max(1, Math.round((c.to - c.from) * edit.fps));
/** the cuts end to end, in frames */
export const total = cuts.reduce((n, c) => n + frames(c), 0);
"
    )
}

/// A space's project files where they're missing, as `raea init` makes them (it never overwrites, and neither does
/// this), plus media/ for its takes. What it made, by name.
pub fn scaffold_space(dir: &Path) -> anyhow::Result<Vec<String>> {
    std::fs::create_dir_all(dir.join("components"))?;
    std::fs::create_dir_all(dir.join("media"))?;
    let mut made = Vec::new();
    let notes = cli_notes().unwrap_or_else(|| "# This project\n\nA video edit as a React + TypeScript project, rendered by raea. `raea help` lists the commands.\n".into());
    // the shape cuts.json says (a new object's kind sets it: spaces::shape_for), else a vertical 1080x1920 at 30
    let sh = crate::host::shape(dir);
    for (name, body) in [("edit.tsx", CLI_EDIT.to_string()), ("cuts.ts", empty_cuts(sh.width, sh.height, sh.fps.round() as u32)), ("styles.gen.ts", "export const css = \"\";\n".into()), ("AGENTS.md", notes)] {
        let p = dir.join(name);
        if !p.exists() {
            std::fs::write(&p, body)?;
            made.push(name.to_string());
        }
    }
    Ok(made)
}

/// The notes at a new project's root, for an agent working at it.
const ROOT_NOTES: &str = "# This project

A raea void project: a folder of objects, each anything that renders (an ad, a game, a deck), each a small React +
TypeScript project rendered by raea.

- `objects/<name>/`: one object each (edit.tsx, cuts.ts, components/, media/, brand/, AGENTS.md). Work on one inside
  its folder: `cd objects/<name>` then `raea help`.
- `brand/`: brand kits any object can use (brand/<name>/brand.css). Copy one into an object's brand/ to use it there.
- `shared/components/`: components any object can use; copy into an object's components/ (objects don't import across).
- `media/`: a shared footage library. An object links the takes it uses into its own engine/media/.
- `.raea/`: the project's record (tasks, questions, notes: `raea status`). `.void/`: void's own (workspace.json, the
  chats). Leave them alone.

Objects stay separate: an object reads only its own folder, so it renders the same wherever it's moved.
";

impl Workspace {
    /// The folder its objects live in: objects/ (the founder, 2026-10-17: spaces went, the project holds objects), or an
    /// older workspace's spaces/ while it has one and no objects/.
    /// Where its objects are made: objects/ (void.feature: A space is a setup). A workspace from before keeps its
    /// objects in spaces/, read where they are (legacy_dir) until /tidy moves them.
    pub fn spaces_dir(&self) -> PathBuf {
        match self.layout {
            Layout::Spaces => self.root.join("objects"),
            Layout::Flat => self.root.clone(),
        }
    }

    /// The objects from before this rule: spaces/<slug>/ with a space.json (a folder there with a space.tsx and no
    /// space.json is an interface space, not an object), listed in place.
    pub fn legacy_dir(&self) -> Option<PathBuf> {
        let d = self.root.join("spaces");
        (matches!(self.layout, Layout::Spaces) && d.is_dir()).then_some(d)
    }

    pub fn name(&self) -> &str {
        &self.meta.name
    }

    /// The workspace at `root`, as it is: its workspace.json, or the legacy layout, or (a folder void hasn't seen) None.
    pub fn read(root: &Path) -> Option<Workspace> {
        let root = root.to_path_buf();
        if let Some(meta) = std::fs::read_to_string(root.join(".void/workspace.json")).ok().and_then(|t| serde_json::from_str::<Meta>(&t).ok()) {
            return Some(Workspace { root, meta, layout: Layout::Spaces });
        }
        if is_legacy(&root) {
            let name = if root == legacy_root() { LEGACY_NAME.to_string() } else { folder_name(&root) };
            return Some(Workspace { root, meta: Meta { void: 1, name, created: 0, settings: serde_json::Value::Null }, layout: Layout::Flat });
        }
        None
    }

    /// Open any folder as a workspace (Open folder…): as it is when void knows it, else marked as one (.void/ and
    /// spaces/ added; nothing else in it is touched). The folder must exist.
    pub fn open(root: &Path) -> anyhow::Result<Workspace> {
        anyhow::ensure!(root.is_dir(), "no folder at {}", root.display());
        if let Some(ws) = Workspace::read(root) {
            return Ok(ws);
        }
        Workspace::mark(root, false)
    }

    /// A new workspace (New workspace…): the folder made if it's missing, marked, and when it was empty, laid out in
    /// full (AGENTS.md, brand/, shared/components/, media/). A folder that's already a workspace opens as it is.
    pub fn create(root: &Path) -> anyhow::Result<Workspace> {
        std::fs::create_dir_all(root)?;
        if let Some(ws) = Workspace::read(root) {
            return Ok(ws);
        }
        let empty = std::fs::read_dir(root)?.flatten().all(|e| e.file_name().to_string_lossy().starts_with('.'));
        Workspace::mark(root, empty)
    }

    fn mark(root: &Path, full: bool) -> anyhow::Result<Workspace> {
        let meta = Meta { void: 1, name: folder_name(root), created: now(), settings: serde_json::json!({}) };
        std::fs::create_dir_all(root.join(".void"))?;
        if !root.join("spaces").is_dir() {
            std::fs::create_dir_all(root.join("objects"))?;
        }
        let file = root.join(".void/workspace.json");
        if !file.exists() {
            std::fs::write(&file, serde_json::to_string_pretty(&meta)? + "\n")?;
        }
        if full {
            for d in ["brand", "shared/components", "media"] {
                std::fs::create_dir_all(root.join(d))?;
            }
            if !root.join("AGENTS.md").exists() {
                std::fs::write(root.join("AGENTS.md"), ROOT_NOTES)?;
            }
        }
        Ok(Workspace { root: root.to_path_buf(), meta, layout: Layout::Spaces })
    }

    /// Its spaces, most recently opened first.
    pub fn spaces(&self) -> Vec<crate::spaces::Space> {
        crate::spaces::list_dirs(&[Some(self.spaces_dir()), self.legacy_dir()].into_iter().flatten().collect::<Vec<_>>())
    }
}

/// The workspace open in the window. `spaces::dir()` follows it, so every space call lists, finds and makes spaces in
/// it; with none open (evals, `void tool`) spaces fall back to the legacy global folder.
static CURRENT: RwLock<Option<Workspace>> = RwLock::new(None);

pub fn current() -> Option<Workspace> {
    CURRENT.read().ok().and_then(|c| c.clone())
}

pub fn set_current(ws: Option<Workspace>) {
    if let Ok(mut c) = CURRENT.write() {
        *c = ws;
    }
}

/// The folder given at launch (`void PATH`), opened instead of the last workspace.
pub static LAUNCH: std::sync::OnceLock<PathBuf> = std::sync::OnceLock::new();

// ── recent workspaces ───────────────────────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Recent {
    pub path: PathBuf,
    pub name: String,
    /// Unix seconds; 0 for the legacy folder offered before it was ever opened.
    #[serde(default)]
    pub opened: u64,
}

impl Recent {
    pub fn exists(&self) -> bool {
        self.path.is_dir()
    }
}

/// How many recent workspaces are kept.
const KEEP: usize = 12;

pub fn recent_file() -> PathBuf {
    crate::spaces::home().join("recent.json")
}

/// The recent list in `file`, newest first (missing or unreadable: empty).
pub fn recent_in(file: &Path) -> Vec<Recent> {
    let mut v: Vec<Recent> = std::fs::read_to_string(file).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default();
    v.sort_by(|a, b| b.opened.cmp(&a.opened));
    v
}

/// The chooser's list: the recent workspaces that still exist, newest first, and the legacy folder at the end when it
/// holds spaces and isn't listed already (so the old spaces are always one click away).
pub fn choices(file: &Path, legacy: &Path) -> Vec<Recent> {
    let mut v: Vec<Recent> = recent_in(file).into_iter().filter(Recent::exists).collect();
    if has_spaces_in(legacy) && !v.iter().any(|r| r.path == legacy) {
        v.push(Recent { path: legacy.to_path_buf(), name: LEGACY_NAME.into(), opened: 0 });
    }
    v
}

/// `ws` opened now: first in the recent list in `file`.
pub fn remember_in(file: &Path, ws: &Workspace) -> anyhow::Result<()> {
    let mut v: Vec<Recent> = recent_in(file).into_iter().filter(|r| r.path != ws.root).collect();
    v.insert(0, Recent { path: ws.root.clone(), name: ws.meta.name.clone(), opened: now() });
    v.truncate(KEEP);
    if let Some(d) = file.parent() {
        std::fs::create_dir_all(d)?;
    }
    std::fs::write(file, serde_json::to_string_pretty(&v)? + "\n")?;
    Ok(())
}

/// Take a workspace off the recent list (its folder stays where it is).
pub fn forget_in(file: &Path, path: &Path) -> anyhow::Result<()> {
    let v: Vec<Recent> = recent_in(file).into_iter().filter(|r| r.path != path).collect();
    std::fs::write(file, serde_json::to_string_pretty(&v)? + "\n")?;
    Ok(())
}

/// What to open at launch without asking: the folder given on the command line, else the last workspace if it still
/// exists and was opened in the last two weeks. None shows the chooser.
pub fn reopen(file: &Path) -> Option<PathBuf> {
    if let Some(p) = LAUNCH.get() {
        return Some(p.clone());
    }
    let last = recent_in(file).into_iter().next()?;
    (last.exists() && now().saturating_sub(last.opened) < 14 * 86400).then_some(last.path)
}

/// Ask macOS for a folder (the system's own folder picker, through osascript so it never blocks the window's event
/// loop). None when cancelled.
pub async fn pick_folder(prompt: &str) -> Option<PathBuf> {
    let script = format!("POSIX path of (choose folder with prompt \"{}\")", prompt.replace('"', "'"));
    let out = tokio::process::Command::new("osascript").arg("-e").arg(&script).output().await.ok()?;
    if !out.status.success() {
        return None;
    }
    let p = String::from_utf8_lossy(&out.stdout).trim().trim_end_matches('/').to_string();
    (!p.is_empty()).then(|| PathBuf::from(p))
}

/// Ask macOS where a new workspace goes: a save dialog, so the person names a folder and picks where it lives (it's
/// made when opened). None when cancelled.
pub async fn pick_new(prompt: &str) -> Option<PathBuf> {
    let script = format!("POSIX path of (choose file name with prompt \"{}\" default name \"New workspace\")", prompt.replace('"', "'"));
    let out = tokio::process::Command::new("osascript").arg("-e").arg(&script).output().await.ok()?;
    if !out.status.success() {
        return None;
    }
    let p = String::from_utf8_lossy(&out.stdout).trim().trim_end_matches('/').to_string();
    (!p.is_empty()).then(|| PathBuf::from(p))
}

/// A path for showing: the home folder as ~.
pub fn tilde(p: &Path) -> String {
    let home = std::env::var_os("HOME").map(PathBuf::from).unwrap_or_default();
    match p.strip_prefix(&home) {
        Ok(rest) if !home.as_os_str().is_empty() => format!("~/{}", rest.display()),
        _ => p.display().to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A fresh scratch folder under the system temp folder, per test.
    fn scratch(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("void-ws-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn new_workspace_is_laid_out() {
        let d = scratch("new");
        let root = d.join("Coffee Ads");
        let ws = Workspace::create(&root).unwrap();
        assert_eq!(ws.name(), "Coffee Ads");
        assert_eq!(ws.layout, Layout::Spaces);
        assert_eq!(ws.spaces_dir(), root.join("objects"));
        for p in [".void/workspace.json", "objects", "brand", "shared/components", "media", "AGENTS.md"] {
            assert!(root.join(p).exists(), "{p} missing");
        }
        // read back the same
        assert_eq!(Workspace::read(&root).unwrap().meta, ws.meta);
        assert!(ws.spaces().is_empty());
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn opening_a_folder_touches_only_void_and_objects() {
        let d = scratch("open");
        std::fs::write(d.join("notes.txt"), "mine").unwrap();
        let ws = Workspace::open(&d).unwrap();
        assert_eq!(ws.layout, Layout::Spaces);
        let mut names: Vec<String> = std::fs::read_dir(&d).unwrap().flatten().map(|e| e.file_name().to_string_lossy().into_owned()).collect();
        names.sort();
        assert_eq!(names, [".void", "notes.txt", "objects"]);
        assert_eq!(std::fs::read_to_string(d.join("notes.txt")).unwrap(), "mine");
        // a non-empty folder made "new" isn't filled in either
        let ws2 = Workspace::create(&d).unwrap();
        assert_eq!(ws2.meta, ws.meta);
        assert!(!d.join("AGENTS.md").exists());
        assert!(Workspace::open(&d.join("nope")).is_err());
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn spaces_in_a_workspace() {
        let d = scratch("spaces");
        let ws = Workspace::create(&d.join("w")).unwrap();
        let sp = crate::spaces::create_in(&ws.spaces_dir(), "Morning routine").unwrap();
        scaffold_space(&sp.folder()).unwrap();
        assert_eq!(sp.folder(), ws.root.join("objects/morning-routine"));
        for f in ["space.json", "edit.tsx", "cuts.ts", "styles.gen.ts", "AGENTS.md", "components", "media"] {
            assert!(sp.folder().join(f).exists(), "{f} missing");
        }
        let all = ws.spaces();
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].folder(), sp.folder());
        // an older workspace's objects stay in its spaces/ folder, listed where they are; new ones go in objects/, and an
        // interface space beside them (a space.tsx, no space.json) isn't an object
        let old = scratch("old-layout");
        std::fs::create_dir_all(old.join("spaces")).unwrap();
        let before = crate::spaces::create_in(&old.join("spaces"), "From before").unwrap();
        std::fs::create_dir_all(old.join("spaces/my-ui")).unwrap();
        std::fs::write(old.join("spaces/my-ui/space.tsx"), "export default () => null").unwrap();
        let ow = Workspace::open(&old).unwrap();
        assert_eq!(ow.spaces_dir(), old.join("objects"));
        assert_eq!(ow.legacy_dir(), Some(old.join("spaces")));
        let fresh = crate::spaces::create_in(&ow.spaces_dir(), "Made now").unwrap();
        let listed: Vec<PathBuf> = ow.spaces().iter().map(|s| s.folder()).collect();
        assert!(listed.contains(&before.folder()) && listed.contains(&fresh.folder()) && listed.len() == 2, "{listed:?}");
        // /tidy moves the object from before into objects/, and leaves the interface space
        assert_eq!(crate::app::tidy(&ow), vec!["from-before".to_string()]);
        assert!(old.join("objects/from-before/space.json").is_file() && old.join("spaces/my-ui/space.tsx").is_file());
        assert_eq!(ow.spaces().len(), 2);
        // scaffolding again changes nothing
        std::fs::write(sp.folder().join("edit.tsx"), "mine").unwrap();
        assert!(scaffold_space(&sp.folder()).unwrap().is_empty());
        assert_eq!(std::fs::read_to_string(sp.folder().join("edit.tsx")).unwrap(), "mine");
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn raea_init_template_comes_from_the_cli() {
        let notes = cli_notes().expect("NOTES found in raea/cli/src/index.ts");
        assert!(notes.starts_with("# This project"));
        assert!(notes.contains("`raea check`") && !notes.contains("\\`"));
        assert!(CLI_EDIT.contains("registerRoot"));
        assert!(empty_cuts(1080, 1920, 30).contains("export const edit = {\"width\":1080,\"height\":1920,\"fps\":30};"));
    }

    /// The files match what raea/cli's own `raea init` writes, byte for byte (raea/cli run in process on raea's
    /// JavaScriptCore with Node's core: raea-native js::run_node).
    #[test]
    fn scaffold_matches_raea_init() {
        let d = scratch("parity");
        let cli = Path::new(env!("CARGO_MANIFEST_DIR")).join("../cli/src/index.ts");
        let js = format!(
            r#"import {{ raeaHandler }} from {cli:?}; import * as f from "fs";
const fs = {{ exists: async (p) => f.existsSync(p), readFile: async (p) => f.readFileSync(p, "utf8"), writeFile: async (p, x) => f.writeFileSync(p, x), mkdir: async (p, o) => {{ f.mkdirSync(p, o); }} }};
await raeaHandler(() => ({{ width: 1080, height: 1920, fps: 30 }}))(["init"], {{ cwd: {dir:?}, fs }});"#,
            cli = cli.display().to_string(),
            dir = d.join("cli").display().to_string()
        );
        std::fs::create_dir_all(d.join("cli")).unwrap();
        let script = d.join("init.mjs");
        std::fs::write(&script, &js).unwrap();
        let out = raea_native::js::run_node(&script, &[], String::new(), std::time::Duration::from_secs(60));
        assert!(out.exit == 0, "{}", out.stderr);
        scaffold_space(&d.join("rust")).unwrap();
        for f in ["edit.tsx", "cuts.ts", "styles.gen.ts", "AGENTS.md"] {
            assert_eq!(std::fs::read_to_string(d.join("rust").join(f)).unwrap(), std::fs::read_to_string(d.join("cli").join(f)).unwrap(), "{f} differs from raea init's");
        }
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn legacy_folder_is_a_workspace_as_it_is() {
        let d = scratch("legacy");
        let sp = crate::spaces::create_in(&d, "Hook").unwrap();
        std::fs::write(sp.folder().join("edit.rrl"), "-- old\n").unwrap();
        assert!(is_legacy(&d));
        let before: Vec<_> = std::fs::read_dir(&d).unwrap().flatten().map(|e| e.file_name()).collect();
        let ws = Workspace::open(&d).unwrap();
        assert_eq!(ws.layout, Layout::Flat);
        assert_eq!(ws.spaces_dir(), d);
        assert_eq!(ws.spaces().len(), 1);
        // an rrl-only space is listed, with nothing to show until a turn runs `raea init` in it
        assert!(!ws.spaces()[0].has_program());
        // nothing written into it
        let after: Vec<_> = std::fs::read_dir(&d).unwrap().flatten().map(|e| e.file_name()).collect();
        assert_eq!(before, after);
        // a workspace with spaces/ isn't legacy, nor is an empty folder
        let w = Workspace::create(&d.join("w")).unwrap();
        assert!(!is_legacy(&w.root));
        assert!(!is_legacy(&scratch("empty")));
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn recent_list() {
        let d = scratch("recent");
        let file = d.join("home/recent.json");
        let legacy = d.join("legacy");
        assert!(recent_in(&file).is_empty());
        assert!(choices(&file, &legacy).is_empty());
        let a = Workspace::create(&d.join("a")).unwrap();
        let b = Workspace::create(&d.join("b")).unwrap();
        remember_in(&file, &a).unwrap();
        remember_in(&file, &b).unwrap();
        // b's newer; reopening a moves it first and keeps one entry each
        let mut v = recent_in(&file);
        v[0].opened = now();
        v[1].opened = now() - 100;
        std::fs::write(&file, serde_json::to_string(&v).unwrap()).unwrap();
        assert_eq!(recent_in(&file).iter().map(|r| r.name.as_str()).collect::<Vec<_>>(), ["b", "a"]);
        assert_eq!(reopen(&file), Some(b.root.clone()));
        // a gone folder isn't offered (but stays in the file until forgotten)
        std::fs::remove_dir_all(&b.root).unwrap();
        assert_eq!(choices(&file, &legacy).iter().map(|r| r.name.as_str()).collect::<Vec<_>>(), ["a"]);
        assert_eq!(reopen(&file), None);
        forget_in(&file, &b.root).unwrap();
        assert_eq!(recent_in(&file).len(), 1);
        // the legacy folder joins the list once it has spaces
        crate::spaces::create_in(&legacy, "Hook").unwrap();
        let c = choices(&file, &legacy);
        assert_eq!(c.last().map(|r| (r.name.as_str(), r.opened)), Some((LEGACY_NAME, 0)));
        // stale: not reopened without asking
        let mut v = recent_in(&file);
        v[0].opened = now() - 30 * 86400;
        std::fs::write(&file, serde_json::to_string(&v).unwrap()).unwrap();
        assert_eq!(reopen(&file), None);
        let _ = std::fs::remove_dir_all(&d);
    }
}
