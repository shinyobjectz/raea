//! void's shell (apps/void/shell.feature): the header, the files on the left and the chat on the right, the same around
//! every space. They're panels like a space's (void/shell/*.tsx, added to every space's layout by apps/void/kit/load.ts and
//! laid outside its regions by space_ui::Layout::boxes); this keeps what's theirs: which are shown and how wide (in
//! ~/.raea-void/settings.json, "shell"), and the workspace's tree the file panel draws, with the Material Icon Theme's
//! icons (apps/void/assets/material-icon-theme), walked again when the folder changes.

use std::collections::{BTreeSet, HashMap};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};

use dioxus::prelude::*;
use serde::{Deserialize, Serialize};

/// Which of the shell's side panels are shown, and their widths in points; the dock's edge, size and the terminal
/// shown in it (0: the + picker).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Shown {
    pub files: bool,
    pub chat: bool,
    pub files_w: f64,
    pub chat_w: f64,
    pub dock: bool,
    pub dock_side: Side,
    pub dock_size: f64,
    pub dock_active: u64,
}

impl Default for Shown {
    fn default() -> Self {
        Shown { files: true, chat: true, files_w: 260.0, chat_w: 380.0, dock: true, dock_side: Side::Bottom, dock_size: 240.0, dock_active: 1 }
    }
}

/// The edge of the stage the dock attaches to (⇅ goes round them).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Side {
    #[default]
    Bottom,
    Left,
    Top,
    Right,
}

impl Side {
    pub fn name(self) -> &'static str {
        match self {
            Side::Bottom => "bottom",
            Side::Left => "left",
            Side::Top => "top",
            Side::Right => "right",
        }
    }
    fn next(self) -> Side {
        match self {
            Side::Bottom => Side::Left,
            Side::Left => Side::Top,
            Side::Top => Side::Right,
            Side::Right => Side::Bottom,
        }
    }
    pub fn parse(s: &str) -> Option<Side> {
        Some(match s {
            "bottom" => Side::Bottom,
            "left" => Side::Left,
            "top" => Side::Top,
            "right" => Side::Right,
            _ => return None,
        })
    }
}

pub static SHOWN: GlobalSignal<Shown> = Signal::global(|| {
    let path = crate::spaces::home().join("settings.json");
    std::fs::read_to_string(path).ok().and_then(|t| serde_json::from_str::<serde_json::Value>(&t).ok()).and_then(|v| serde_json::from_value(v["shell"].clone()).ok()).unwrap_or_default()
});

/// The narrowest and widest a side panel is let be, in points.
pub const MIN_W: f64 = 180.0;
pub const MAX_W: f64 = 640.0;
/// The dock's least and most, across the edge it's on.
pub const DOCK_MIN: f64 = 120.0;
pub const DOCK_MAX: f64 = 800.0;

/// Show or hide the files (⌘B, the cube), the chat (⌘J, the cat) or the dock (⌃`); kept for next time.
pub fn toggle(which: &str) {
    let mut s = *SHOWN.peek();
    match which {
        "files" => s.files = !s.files,
        "chat" => s.chat = !s.chat,
        "dock" => s.dock = !s.dock,
        _ => return,
    }
    set(s);
}

/// A side panel's width, as its edge is dragged; kept for next time.
pub fn width(which: &str, w: f64) {
    let mut s = *SHOWN.peek();
    let w = if which == "dock" { w.clamp(DOCK_MIN, DOCK_MAX) } else { w.clamp(MIN_W, MAX_W) }.round();
    match which {
        "files" => s.files_w = w,
        "chat" => s.chat_w = w,
        "dock" => s.dock_size = w,
        _ => return,
    }
    set(s);
}

fn set(s: Shown) {
    if s == *SHOWN.peek() {
        return;
    }
    *SHOWN.write() = s;
    crate::app::save_setting("shell", serde_json::to_value(s).unwrap_or_default());
    tell_shown();
}

/// The panel thread told which shell panels are shown (one hidden isn't stepped): the dock's terminals but the one
/// shown aren't.
pub fn tell_shown() {
    let s = *SHOWN.peek();
    let Some(l) = crate::space_ui::CURRENT.peek().clone() else { return };
    for (i, (name, _)) in l.actions().iter().enumerate() {
        match name.as_str() {
            "shell-files" => crate::space_ui::show_panel(i, s.files),
            "shell-chat" => crate::space_ui::show_panel(i, s.chat),
            "shell-dock" => crate::space_ui::show_panel(i, s.dock),
            n => {
                if let Some(id) = tab_id(n) {
                    crate::space_ui::show_panel(i, s.dock && s.dock_active == id);
                }
            }
        }
    }
}

// ---- the dock ------------------------------------------------------------------------------------------------------

/// A terminal open in the dock: its id, and the terminal (a terminal.tsx's slug, or a controller's, shown alone).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Tab {
    pub id: u64,
    /// "control" before terminals laid out controllers (a tab was one control)
    #[serde(alias = "control")]
    pub terminal: String,
}

/// The dock's terminals, in tab order; kept for next time (a cli comes back as a new shell).
pub static TABS: GlobalSignal<Vec<Tab>> = Signal::global(|| {
    let path = crate::spaces::home().join("settings.json");
    std::fs::read_to_string(path).ok().and_then(|t| serde_json::from_str::<serde_json::Value>(&t).ok()).and_then(|v| serde_json::from_value(v["dock"].clone()).ok()).unwrap_or_else(|| vec![Tab { id: 1, terminal: "cli".into() }])
});

/// An audio terminal is open: the stage tells the panels what sounds (void:sounds), which wakes every one of them as
/// it changes, only then.
pub static AUDIO_OPEN: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// A terminal holds at most this many controllers: a cli's shell is numbered by its tab and its place in it
/// (tab * CELLS + k).
const CELLS: u64 = 100;

/// The name controller `k` of terminal `tab` goes by in the panels: a cli's says so (the panel thread gives it its shell).
fn cell_name(tab: u64, k: usize, slug: &str) -> String {
    if slug == "cli" { format!("cli:{tab}.{k}") } else { format!("dock:{tab}.{k}") }
}

/// The tab a panel is in, by its name.
pub fn tab_id(name: &str) -> Option<u64> {
    let rest = name.strip_prefix("cli:").or_else(|| name.strip_prefix("dock:"))?;
    rest.split('.').next()?.parse().ok()
}

/// The shell a cli panel draws, by its name.
pub fn pty_of(name: &str) -> Option<u64> {
    let (tab, k) = name.strip_prefix("cli:")?.split_once('.')?;
    Some(tab.parse::<u64>().ok()? * CELLS + k.parse::<u64>().ok()?)
}

/// A terminal's layout: a controller, or a split of them; each controller by its slug or its folder.
#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum Node {
    Controller { controller: String },
    Split { split: String, sizes: Vec<f64>, kids: Vec<Node> },
}

/// The controllers a layout holds, each with its box in the terminal's, as fractions (x0, y0, x1, y1).
fn cells(n: &Node, b: [f64; 4], out: &mut Vec<(String, [f64; 4])>) {
    match n {
        Node::Controller { controller } => out.push((controller.clone(), b)),
        Node::Split { split, sizes, kids } => {
            let total: f64 = sizes.iter().take(kids.len()).sum::<f64>().max(1e-6);
            let mut at = 0.0;
            for (k, s) in kids.iter().zip(sizes) {
                let (a, z) = (at / total, (at + s) / total);
                at += s;
                let c = if split == "column" { [b[0], b[1] + (b[3] - b[1]) * a, b[2], b[1] + (b[3] - b[1]) * z] } else { [b[0] + (b[2] - b[0]) * a, b[1], b[0] + (b[2] - b[0]) * z, b[3]] };
                cells(k, c, out);
            }
        }
    }
}

/// Terminals' layouts read from their terminal.tsx (`bun kit/load.ts --terminal`), kept by the file's modified time.
static LAYOUTS: Mutex<Option<HashMap<PathBuf, (std::time::SystemTime, Option<String>, Node)>>> = Mutex::new(None);

/// Terminal `slug`'s name and layout: a terminal.tsx's, or one controller of that slug alone. None: there's neither.
pub fn terminal_of(slug: &str, found: &[Found]) -> Option<(String, Node)> {
    if let Some(t) = found.iter().find(|f| f.kind == "terminal" && f.slug == slug) {
        let file = t.folder.join("terminal.tsx");
        let modified = std::fs::metadata(&file).and_then(|m| m.modified()).ok()?;
        if let Some((at, name, node)) = LAYOUTS.lock().unwrap().as_ref().and_then(|c| c.get(&file)) {
            if *at == modified {
                return Some((name.clone().unwrap_or(t.name.clone()), node.clone()));
            }
        }
        // read here, the once a file changes: a terminal opened shows at once, not a frame later
        let out = std::process::Command::new("bun").arg(crate::space_ui::here().join("kit/load.ts")).arg("--terminal").arg(&t.folder).output().ok()?;
        let v: serde_json::Value = serde_json::from_slice(&out.stdout).ok()?;
        if let Some(e) = v.get("error") {
            crate::mcp::log(format!("dock: terminal {slug}: {e}"));
            return None;
        }
        let node: Node = serde_json::from_value(v["root"].clone()).map_err(|e| crate::mcp::log(format!("dock: terminal {slug}: {e}"))).ok()?;
        let name = v["name"].as_str().map(str::to_string);
        LAYOUTS.lock().unwrap().get_or_insert_with(HashMap::new).insert(file, (modified, name.clone(), node.clone()));
        return Some((name.unwrap_or(t.name.clone()), node));
    }
    let c = found.iter().find(|f| f.kind == "control" && f.slug == slug)?;
    Some((c.name.clone(), Node::Controller { controller: c.slug.clone() }))
}

/// A controller's file: a slug among the controllers there are, or a folder (one beside its terminal.tsx).
fn control_file(c: &str, found: &[Found]) -> Option<PathBuf> {
    let p = Path::new(c);
    if p.is_absolute() {
        return Some(p.join("control.tsx")).filter(|f| f.is_file());
    }
    found.iter().find(|f| f.kind == "control" && f.slug == c).map(|f| f.folder.join("control.tsx"))
}

/// A controller's slug: its folder's name.
fn slug_of(c: &str) -> &str {
    Path::new(c).file_name().and_then(|n| n.to_str()).unwrap_or(c)
}

/// The space's layout with the dock after its panels: the dock's bar (apps/void/shell/dock.tsx: tabs, +, ⇅), then each
/// terminal's controllers, each in its box of the terminal; what was docked before taken out.
pub fn docked(mut l: crate::space_ui::Layout) -> crate::space_ui::Layout {
    use crate::space_ui::Region;
    l.regions.retain(|r| !matches!(r, Region::Panel { name: Some(n), .. } if n == "shell-dock" || tab_id(n).is_some()));
    // a space laid out without the shell (RAEA_VOID_NO_SHELL) has no dock
    if !l.regions.iter().any(|r| matches!(r, Region::Panel { shell: true, .. })) {
        return l;
    }
    let side = SHOWN.peek().dock_side.name().to_string();
    let panel = |src: PathBuf, name: String, cell: Option<[f64; 4]>| {
        let actions = actions_of(&src, &name);
        Region::Panel { src, side: side.clone(), size: 0.0, name: Some(name), actions, terminal: false, height: 0.0, shell: true, cell }
    };
    l.regions.push(panel(crate::space_ui::here().join("shell/dock.tsx"), "shell-dock".into(), None));
    let found = found(root_path().as_deref());
    let mut audio = false;
    for t in TABS.peek().iter() {
        let Some((_, node)) = terminal_of(&t.terminal, &found) else {
            crate::mcp::log(format!("dock: no terminal or controller called {}", t.terminal));
            continue;
        };
        let mut cs = Vec::new();
        cells(&node, [0.0, 0.0, 1.0, 1.0], &mut cs);
        for (k, (c, cell)) in cs.into_iter().take(CELLS as usize).enumerate() {
            let slug = slug_of(&c);
            audio |= slug == "audio";
            match control_file(&c, &found) {
                Some(src) => l.regions.push(panel(src, cell_name(t.id, k, slug), Some(cell))),
                None => crate::mcp::log(format!("dock: terminal {}: no controller called {c}", t.terminal)),
            }
        }
    }
    AUDIO_OPEN.store(audio, Ordering::Relaxed);
    l
}

/// Each dock panel's file's actions (its `export const actions`, as apps/void/kit/load.ts reads them: the agent's tools while
/// it's open), kept by the file's modified time.
static ACTS: Mutex<Option<HashMap<PathBuf, (std::time::SystemTime, Vec<crate::space_ui::ActionSpec>)>>> = Mutex::new(None);

/// Panel `name`'s actions from its file `src`: as kept, or none yet while bun reads them off the UI thread (handed to
/// the open space's agent list when they land).
fn actions_of(src: &Path, name: &str) -> Vec<crate::space_ui::ActionSpec> {
    let modified = std::fs::metadata(src).and_then(|m| m.modified()).ok();
    if let Some((at, acts)) = ACTS.lock().unwrap().as_ref().and_then(|c| c.get(src)) {
        if Some(*at) == modified {
            return acts.clone();
        }
    }
    let (src, name) = (src.to_path_buf(), name.to_string());
    std::thread::spawn(move || {
        let out = std::process::Command::new("bun").arg(crate::space_ui::here().join("kit/load.ts")).arg("--actions").arg(&src).output();
        let v: Option<serde_json::Value> = out.ok().and_then(|o| serde_json::from_slice(&o.stdout).ok());
        let acts: Vec<crate::space_ui::ActionSpec> = match v.as_ref().map(|v| (v.get(src.to_string_lossy().as_ref()), v.get("error"))) {
            Some((Some(a), _)) => serde_json::from_value(a.clone()).unwrap_or_default(),
            other => {
                crate::mcp::log(format!("dock: {}'s actions: {}", src.display(), other.and_then(|o| o.1).map(|e| e.to_string()).unwrap_or("unread".into())));
                return;
            }
        };
        if let Some(m) = modified {
            ACTS.lock().unwrap().get_or_insert_with(HashMap::new).insert(src, (m, acts.clone()));
        }
        crate::space_ui::set_actions(&name, &acts);
    });
    Vec::new()
}

/// The open space laid out again with the dock as it is now (a terminal opened or closed): its panels that stay are
/// kept as they are (space_ui::open reuses them).
fn redock() {
    let Some(l) = crate::space_ui::CURRENT.peek().clone() else { return };
    let l = docked(l);
    crate::space_ui::open(Some(&l));
    *crate::space_ui::CURRENT.write() = Some(l);
    tell_shown();
}

fn save_tabs() {
    crate::app::save_setting("dock", serde_json::to_value(&*TABS.peek()).unwrap_or_default());
}

/// Terminal `slug` opened in the dock (a controller's alone in one; each cli in it a new shell), and shown.
pub fn open_tab(slug: &str) {
    let known = found(root_path().as_deref()).iter().any(|f| (f.kind == "terminal" || f.kind == "control") && f.slug == slug);
    if !known {
        crate::mcp::log(format!("dock: no terminal or controller called {slug}"));
        return;
    }
    let id = TABS.peek().iter().map(|t| t.id).max().unwrap_or(0).max(crate::pty::open().into_iter().max().unwrap_or(0) / CELLS) + 1;
    TABS.write().push(Tab { id, terminal: slug.to_string() });
    save_tabs();
    let mut s = *SHOWN.peek();
    s.dock = true;
    s.dock_active = id;
    *SHOWN.write() = s;
    crate::app::save_setting("shell", serde_json::to_value(s).unwrap_or_default());
    redock();
}

/// Terminal `id` closed (its clis' shells with it); the one beside it shown.
pub fn close_tab(id: u64) {
    let mut tabs = TABS.peek().clone();
    let Some(at) = tabs.iter().position(|t| t.id == id) else { return };
    tabs.remove(at);
    for pty in crate::pty::open().into_iter().filter(|p| p / CELLS == id) {
        crate::pty::close(pty);
    }
    *TABS.write() = tabs.clone();
    save_tabs();
    let mut s = *SHOWN.peek();
    if s.dock_active == id {
        s.dock_active = tabs.get(at.min(tabs.len().saturating_sub(1))).map(|t| t.id).unwrap_or(0);
    }
    *SHOWN.write() = s;
    crate::app::save_setting("shell", serde_json::to_value(s).unwrap_or_default());
    redock();
}

/// Terminal `id` shown in the dock (0: the + picker).
pub fn show_tab(id: u64) {
    let mut s = *SHOWN.peek();
    s.dock = true;
    s.dock_active = id;
    set(s);
}

/// The dock moved to the next edge round (⇅), or to `side`.
pub fn dock_side(side: Option<Side>) {
    let mut s = *SHOWN.peek();
    s.dock_side = side.unwrap_or(s.dock_side.next());
    set(s);
    redock();
}

/// What the dock's bar is told: its edge, its terminals (each with its name) and what + can open: the terminals there
/// are, then the controllers.
pub fn dock_json() -> serde_json::Value {
    let s = *SHOWN.peek();
    let found = found(root_path().as_deref());
    let tabs: Vec<serde_json::Value> = TABS.read().iter().map(|t| {
        let name = terminal_of(&t.terminal, &found).map(|(n, _)| n).unwrap_or(t.terminal.clone());
        serde_json::json!({"id": t.id, "terminal": t.terminal, "name": name})
    }).collect();
    let list: Vec<serde_json::Value> = found.iter().filter(|f| f.kind == "terminal").chain(found.iter().filter(|f| f.kind == "control")).map(|c| serde_json::json!({"slug": c.slug, "name": c.name, "own": c.own, "kind": c.kind})).collect();
    serde_json::json!({"side": s.dock_side.name(), "active": s.dock_active, "tabs": tabs, "controls": list})
}

// ---- the project (apps/void/shell.feature: A project is a folder with a raea.json) -------------------------------------

/// raea.json: what a person says about the project. Every field but "raea" may be left out.
#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct ProjectFile {
    pub raea: u32,
    pub name: Option<String>,
    /// where to look for compositions, controls and spaces: folders (relative, "x/**" as "x"), "**" for all
    pub find: Vec<String>,
    /// names or relative paths left out, beyond hidden folders and SKIP's
    pub ignore: Vec<String>,
    /// the space each kind of composition opens in
    pub spaces: HashMap<String, String>,
}

pub fn project_file(root: &Path) -> Option<ProjectFile> {
    let text = std::fs::read_to_string(root.join("raea.json")).ok()?;
    serde_json::from_str(&text).map_err(|e| crate::mcp::log(format!("raea.json: {e}"))).ok()
}

/// A thing found in the project by its marker file (composition.json, control.tsx, space.tsx), or one of void's own.
#[derive(Debug, Clone, PartialEq)]
pub struct Found {
    pub kind: &'static str,
    pub slug: String,
    pub name: String,
    pub folder: PathBuf,
    /// the project's (not void's)
    pub own: bool,
}

const MARKERS: [(&str, &str); 4] = [("composition.json", "composition"), ("control.tsx", "control"), ("terminal.tsx", "terminal"), ("space.tsx", "space")];

/// What the project holds, by marker file, under what raea.json's "find" covers (all of it without one), leaving out
/// hidden folders, SKIP's and its "ignore". A marked folder isn't looked into further.
pub fn scan(root: &Path) -> Vec<Found> {
    let pf = project_file(root).unwrap_or_default();
    let ignore: Vec<String> = pf.ignore.iter().map(|i| i.trim_end_matches('/').trim_end_matches("/**").to_string()).collect();
    let starts: Vec<PathBuf> = if pf.find.is_empty() || pf.find.iter().any(|f| f == "**" || f == "." || f.is_empty()) {
        vec![root.to_path_buf()]
    } else {
        pf.find.iter().map(|f| root.join(f.trim_end_matches("/**").trim_end_matches('/'))).collect()
    };
    fn walk(root: &Path, dir: &Path, depth: usize, ignore: &[String], out: &mut Vec<Found>) {
        if depth > 8 {
            return;
        }
        for (file, kind) in MARKERS {
            if dir.join(file).is_file() {
                let slug = dir.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
                let name = if kind == "composition" {
                    std::fs::read_to_string(dir.join(file)).ok().and_then(|t| serde_json::from_str::<serde_json::Value>(&t).ok()).and_then(|v| v["name"].as_str().map(str::to_string))
                } else {
                    None
                };
                out.push(Found { kind, name: name.unwrap_or_else(|| slug.clone()), slug, folder: dir.to_path_buf(), own: true });
                return;
            }
        }
        let Ok(rd) = std::fs::read_dir(dir) else { return };
        let mut dirs: Vec<PathBuf> = rd.flatten().filter(|e| e.file_type().is_ok_and(|t| t.is_dir())).map(|e| e.path()).collect();
        dirs.sort();
        for d in dirs {
            let name = d.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
            let rel = d.strip_prefix(root).unwrap_or(&d).to_string_lossy().into_owned();
            if name.starts_with('.') || SKIP.contains(&name.as_str()) || ignore.iter().any(|i| *i == name || *i == rel) {
                continue;
            }
            walk(root, &d, depth + 1, ignore, out);
        }
    }
    let mut out = Vec::new();
    for s in starts {
        walk(root, &s, 0, &ignore, &mut out);
    }
    out
}

/// The terminals and controllers there are: void's own (apps/void/terminals/: edit, shell; apps/void/controls/: the
/// cli, the timeline, audio), then the project's, one of the same kind and slug standing in for void's.
pub fn found(root: Option<&Path>) -> Vec<Found> {
    let mut out: Vec<Found> = Vec::new();
    for (dir, file, kind) in [("terminals", "terminal.tsx", "terminal"), ("controls", "control.tsx", "control")] {
        let Ok(rd) = std::fs::read_dir(crate::space_ui::here().join(dir)) else { continue };
        let mut dirs: Vec<PathBuf> = rd.flatten().map(|e| e.path()).filter(|p| p.join(file).is_file()).collect();
        dirs.sort();
        for d in dirs {
            let slug = d.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
            out.push(Found { kind, name: slug.clone(), slug, folder: d, own: false });
        }
    }
    for f in root.map(scan).unwrap_or_default().into_iter().filter(|f| f.kind == "control" || f.kind == "terminal") {
        match out.iter_mut().find(|c| c.kind == f.kind && c.slug == f.slug) {
            Some(c) => *c = f,
            None => out.push(f),
        }
    }
    out
}

/// The open project's folder (the workspace's root).
pub fn root_path() -> Option<PathBuf> {
    TREE.lock().unwrap().root.clone()
}

// ---- the tree ------------------------------------------------------------------------------------------------------

/// What the tree leaves out: hidden names, and the folders builds and package managers fill.
const SKIP: [&str; 4] = ["node_modules", "target", "dist", "__pycache__"];
/// The most rows the tree sends (a folder of thousands shows its first ones).
const MAX_ROWS: usize = 4000;

struct Tree {
    root: Option<PathBuf>,
    /// folders opened, relative to the root
    open: BTreeSet<String>,
    watcher: Option<notify::RecommendedWatcher>,
}

static TREE: Mutex<Tree> = Mutex::new(Tree { root: None, open: BTreeSet::new(), watcher: None });
/// Bumped when the folder changes on disk or a folder is opened or closed; the tree is sent again when it moves.
static GEN: AtomicU64 = AtomicU64::new(1);

/// The workspace whose tree is shown; watched (for files the agent writes) from now on.
pub fn root(r: Option<&Path>) {
    let mut t = TREE.lock().unwrap();
    if t.root.as_deref() == r {
        return;
    }
    t.root = r.map(Path::to_path_buf);
    t.open.clear();
    t.watcher = r.and_then(|r| {
        use notify::Watcher;
        let mut w = notify::recommended_watcher(|e: notify::Result<notify::Event>| {
            // what changed inside a hidden or skipped folder (.git, target, ...) leaves the tree as it is
            if let Ok(e) = e {
                if e.paths.iter().any(|p| !p.components().any(|c| c.as_os_str().to_str().is_some_and(|n| n.starts_with('.') || SKIP.contains(&n)))) {
                    GEN.fetch_add(1, Ordering::Relaxed);
                }
            }
        })
        .ok()?;
        w.watch(r, notify::RecursiveMode::Recursive).ok()?;
        Some(w)
    });
    GEN.fetch_add(1, Ordering::Relaxed);
}

/// A folder in the tree opened or closed.
pub fn open(rel: &str, on: bool) {
    let mut t = TREE.lock().unwrap();
    let changed = if on { t.open.insert(rel.to_string()) } else { t.open.remove(rel) };
    if changed {
        GEN.fetch_add(1, Ordering::Relaxed);
    }
}

/// The tree as of now when it changed since `seen` (its generation, to pass next time), else None.
pub fn tree_since(seen: u64) -> Option<(u64, serde_json::Value)> {
    let g = GEN.load(Ordering::Relaxed);
    if g == seen {
        return None;
    }
    let t = TREE.lock().unwrap();
    let mut rows = Vec::new();
    if let Some(r) = &t.root {
        walk(r, r, 0, &t.open, &mut rows);
    }
    let name = t.root.as_ref().and_then(|r| r.file_name()).map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    Some((g, serde_json::json!({"root": name, "icons": icons_dir(), "rows": rows, "more": rows.len() >= MAX_ROWS})))
}

fn walk(root: &Path, dir: &Path, depth: usize, open: &BTreeSet<String>, rows: &mut Vec<serde_json::Value>) {
    let Ok(rd) = std::fs::read_dir(dir) else { return };
    let mut items: Vec<(bool, String, PathBuf)> = rd
        .flatten()
        .filter_map(|e| {
            let name = e.file_name().to_string_lossy().into_owned();
            if name.starts_with('.') || SKIP.contains(&name.as_str()) {
                return None;
            }
            Some((e.path().is_dir(), name, e.path()))
        })
        .collect();
    // folders first, then by name as a person sorts (case aside)
    items.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.to_lowercase().cmp(&b.1.to_lowercase())));
    for (is_dir, name, path) in items {
        if rows.len() >= MAX_ROWS {
            return;
        }
        let rel = path.strip_prefix(root).unwrap_or(&path).to_string_lossy().into_owned();
        let is_open = is_dir && open.contains(&rel);
        rows.push(serde_json::json!({"rel": rel, "name": name, "dir": is_dir, "depth": depth, "open": is_open, "icon": icon(&name, is_dir, is_open)}));
        if is_open {
            walk(root, &path, depth + 1, open, rows);
        }
    }
}

// ---- the icons -----------------------------------------------------------------------------------------------------

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Manifest {
    file_names: HashMap<String, String>,
    file_extensions: HashMap<String, String>,
    folder_names: HashMap<String, String>,
    folder_names_expanded: HashMap<String, String>,
    language_ids: HashMap<String, String>,
}

fn theme() -> PathBuf {
    crate::space_ui::here().join("assets/material-icon-theme")
}

/// Where the icons are: `{icons}/{icon}.svg` (the panel shows them as images).
fn icons_dir() -> String {
    theme().join("icons").to_string_lossy().into_owned()
}

fn manifest() -> Option<&'static Manifest> {
    static M: OnceLock<Option<Manifest>> = OnceLock::new();
    M.get_or_init(|| {
        let text = std::fs::read_to_string(theme().join("dist/material-icons.json")).ok()?;
        serde_json::from_str(&text).map_err(|e| crate::mcp::log(format!("shell: the icon theme's manifest: {e}"))).ok()
    })
    .as_ref()
}

/// VS Code's language for an extension the theme names no icon for by extension (it maps those by language).
fn language(ext: &str) -> Option<&'static str> {
    Some(match ext {
        "ts" | "mts" | "cts" => "typescript",
        "js" | "mjs" | "cjs" => "javascript",
        "json" | "jsonc" => "json",
        "md" | "markdown" => "markdown",
        "rs" => "rust",
        "py" => "python",
        "txt" => "plaintext",
        "yml" | "yaml" => "yaml",
        "toml" => "toml",
        "sh" | "bash" | "zsh" => "shellscript",
        "css" => "css",
        "html" | "htm" => "html",
        "c" | "h" => "c",
        "cpp" | "cc" | "hpp" => "cpp",
        "swift" => "swift",
        "go" => "go",
        "java" => "java",
        "rb" => "ruby",
        "lua" => "lua",
        "sql" => "sql",
        "xml" => "xml",
        "glsl" | "frag" | "vert" => "glsl",
        "wgsl" => "wgsl",
        _ => return None,
    })
}

/// A file's or folder's icon, as the theme's manifest says: by its name, then (a file) by its extensions longest
/// first ("d.ts" before "ts"), then by its language; else the theme's plain file or folder.
pub fn icon(name: &str, dir: bool, open: bool) -> String {
    let lower = name.to_lowercase();
    let Some(m) = manifest() else { return if dir { "folder".into() } else { "file".into() } };
    if dir {
        let names = if open { &m.folder_names_expanded } else { &m.folder_names };
        return names.get(&lower).cloned().unwrap_or_else(|| if open { "folder-open".into() } else { "folder".into() });
    }
    if let Some(i) = m.file_names.get(&lower) {
        return i.clone();
    }
    let mut rest = lower.as_str();
    while let Some(dot) = rest.find('.') {
        rest = &rest[dot + 1..];
        if let Some(i) = m.file_extensions.get(rest) {
            return i.clone();
        }
    }
    let ext = lower.rsplit('.').next().unwrap_or("");
    language(ext).and_then(|l| m.language_ids.get(l)).cloned().unwrap_or_else(|| "file".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn icons_follow_the_manifest() {
        assert_eq!(icon("edit.tsx", false, false), "react_ts");
        assert_eq!(icon("index.d.ts", false, false), "typescript-def");
        assert_eq!(icon("lib.ts", false, false), "typescript");
        assert_eq!(icon("package.json", false, false), "nodejs");
        assert_eq!(icon("take.MP4", false, false), "video");
        assert_eq!(icon("notes.unknownext", false, false), "file");
        assert_eq!(icon("src", true, false), "folder-src");
        assert_eq!(icon("src", true, true), "folder-src-open");
        assert_eq!(icon("whatever", true, false), "folder");
        // every icon named is a file there
        for n in ["react_ts", "typescript", "video", "folder", "folder-open", "file"] {
            assert!(theme().join("icons").join(format!("{n}.svg")).is_file(), "{n}");
        }
    }

    #[test]
    fn a_terminal_shares_its_box_among_its_controllers() {
        // edit's layout: the timeline over audio, 3 to 2, as kit/load.ts --terminal prints it
        let edit: Node = serde_json::from_str(r#"{"split":"column","sizes":[3,2],"kids":[{"controller":"timeline","name":null},{"controller":"audio","name":null}]}"#).unwrap();
        let mut out = Vec::new();
        cells(&edit, [0.0, 0.0, 1.0, 1.0], &mut out);
        assert_eq!(out, vec![("timeline".to_string(), [0.0, 0.0, 1.0, 0.6]), ("audio".to_string(), [0.0, 0.6, 1.0, 1.0])]);
        // a row inside a column: halves of the lower part
        let nested: Node = serde_json::from_str(r#"{"split":"column","sizes":[1,1],"kids":[{"controller":"a"},{"split":"row","sizes":[1,1],"kids":[{"controller":"b"},{"controller":"c"}]}]}"#).unwrap();
        let mut out = Vec::new();
        cells(&nested, [0.0, 0.0, 1.0, 1.0], &mut out);
        assert_eq!(out.iter().map(|(_, b)| *b).collect::<Vec<_>>(), vec![[0.0, 0.0, 1.0, 0.5], [0.0, 0.5, 0.5, 1.0], [0.5, 0.5, 1.0, 1.0]]);
        // a cli's shell by its tab and place; names back to their tab
        assert_eq!(pty_of(&cell_name(3, 1, "cli")), Some(301));
        assert_eq!(tab_id(&cell_name(3, 1, "timeline")), Some(3));
        assert_eq!(pty_of("dock:3.1"), None);
    }
}
