//! Spaces as the interface (void.feature: A space is the interface; the work is objects). A space is a folder with a
//! space.tsx (written with @raea/space, sdk/space) laying out regions around the stage: panels and toolbars, each
//! a React component played live like a game, and where the terminal docks. void ships its own (apps/void/spaces/); a
//! workspace's spaces/ adds to them, one with the same slug replacing void's. A folder under spaces/ holding an
//! edit.tsx is an object from before this rule, not a space.
//!
//! The layout is read with bun (apps/void/kit/load.ts: space.tsx's elements walked, nothing rendered). Panels run on a
//! thread of their own beside the stage's: each a raea_react::live::Live laid out at its box in CSS pixels (a wrapper
//! scales it by the screen's factor), stepped at 60 fps with its pointer and keys, painted into a texture of its own
//! that the chrome shows with `<img data-raea-texture>` as it shows the stage's.

use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use dioxus::prelude::*;
use serde::Deserialize;

/// A space's layout, as apps/void/kit/load.ts reads it from space.tsx.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct Layout {
    pub name: Option<String>,
    #[serde(default)]
    pub agent: Agent,
    pub regions: Vec<Region>,
    #[serde(skip)]
    pub folder: PathBuf,
    #[serde(skip)]
    pub slug: String,
}

#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
pub struct Agent {
    #[serde(default)]
    pub files: Vec<String>,
    #[serde(default)]
    pub skills: Vec<String>,
}

/// One of a panel's actions (its file's `export const actions`): what a click does there and the agent can do too.
#[derive(Debug, Clone, PartialEq, Deserialize, serde::Serialize)]
pub struct ActionSpec {
    pub name: String,
    #[serde(default)]
    pub about: String,
    #[serde(default)]
    pub args: serde_json::Value,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum Region {
    Stage,
    Panel {
        src: PathBuf,
        side: String,
        size: f64,
        name: Option<String>,
        #[serde(default)]
        actions: Vec<ActionSpec>,
        /// a terminal of the space's own (<Terminal src>): void's terminal pane folds away while it's open
        #[serde(default)]
        terminal: bool,
        /// a sheet's height (<Sheet>: side "sheet", size its width)
        #[serde(default)]
        height: f64,
        /// one of void's shell (the header, files, chat: apps/void/shell.feature), laid out around the space's regions
        #[serde(default)]
        shell: bool,
        /// a controller's box in its dock terminal, as fractions of the terminal's (x0, y0, x1, y1: shell_ui::docked)
        #[serde(default)]
        cell: Option<[f64; 4]>,
    },
    Terminal { side: Option<String> },
}

impl Layout {
    pub fn title(&self) -> String {
        self.name.clone().unwrap_or_else(|| self.slug.clone())
    }
    /// The panels in order, with their sides and sizes (points).
    pub fn panels(&self) -> Vec<(PathBuf, String, f64)> {
        self.regions.iter().filter_map(|r| match r { Region::Panel { src, side, size, .. } => Some((src.clone(), side.clone(), *size)), _ => None }).collect()
    }
    /// Each panel's height if it's a sheet (0 otherwise), in panel order.
    fn heights(&self) -> Vec<f64> {
        self.regions.iter().filter_map(|r| match r { Region::Panel { height, .. } => Some(*height), _ => None }).collect()
    }
    /// The panel that is the sheet called `name`, by its index among the panels.
    pub fn sheet(&self, name: &str) -> Option<usize> {
        let names = self.actions();
        self.panels().iter().enumerate().position(|(i, (_, side, _))| side == "sheet" && names.get(i).is_some_and(|n| n.0 == name))
    }
    /// Whether a panel of its own, named header, stands in for void's row over the stage (the cube, the object's name).
    pub fn own_header(&self) -> bool {
        self.actions().iter().any(|(name, _)| name == "header" || name == "shell-header")
    }
    /// Whether a panel of its own, named player, stands in for void's player bar.
    pub fn own_player(&self) -> bool {
        self.actions().iter().any(|(name, _)| name == "player")
    }
    /// Each panel's name (its own, else its file's stem) and actions, in panel order.
    pub fn actions(&self) -> Vec<(String, Vec<ActionSpec>)> {
        self.regions
            .iter()
            .filter_map(|r| match r {
                Region::Panel { src, name, actions, .. } => Some((name.clone().unwrap_or_else(|| src.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default()), actions.clone())),
                _ => None,
            })
            .collect()
    }
    /// What the agent reads in this space before its turns: the space's agent files, and the skills it names.
    pub fn agent_text(&self) -> String {
        let mut out = format!("You're working in the space \"{}\" (void's interface around the stage: {}).", self.title(), self.folder.display());
        for f in &self.agent.files {
            if let Ok(text) = std::fs::read_to_string(self.folder.join(f)) {
                out.push_str(&format!("\n\n{f}:\n{}", text.trim()));
            }
        }
        if !self.agent.skills.is_empty() {
            out.push_str(&format!("\n\nThe skills this space works with: {}.", self.agent.skills.join(", ")));
        }
        let acts: Vec<String> = self.actions().into_iter().flat_map(|(p, a)| a.into_iter().map(move |a| format!("{p}.{}: {}", a.name, a.about))).collect();
        if !acts.is_empty() {
            out.push_str(&format!("\n\nIts panels' actions (the panel_action tool runs one, as a click would): {}", acts.join("; ")));
        }
        out
    }
    /// Whether the space brings its own terminal (a panel in void's terminal's place).
    pub fn own_terminal(&self) -> bool {
        self.regions.iter().any(|r| matches!(r, Region::Panel { terminal: true, .. }))
    }
    /// The panel standing in for the terminal, by its index among the panels.
    pub fn terminal_panel(&self) -> Option<usize> {
        self.regions.iter().filter(|r| matches!(r, Region::Panel { .. })).position(|r| matches!(r, Region::Panel { terminal: true, .. }))
    }
    /// Where the space docks the terminal, if it says.
    pub fn terminal(&self) -> Option<String> {
        self.regions.iter().find_map(|r| match r { Region::Terminal { side } => side.clone(), _ => None })
    }
    /// Whether each panel is one of void's shell's, in panel order.
    fn cells(&self) -> Vec<Option<[f64; 4]>> {
        self.regions.iter().filter_map(|r| match r { Region::Panel { cell, .. } => Some(*cell), _ => None }).collect()
    }
    fn shells(&self) -> Vec<bool> {
        self.regions.iter().filter_map(|r| match r { Region::Panel { shell, .. } => Some(*shell), _ => None }).collect()
    }
    /// The shell's edges around the space (top, right, left) in points: its header, and its chat and files while shown.
    fn shell_edges(&self, sh: crate::shell_ui::Shown) -> (f64, f64, f64) {
        let mut e = (0.0, 0.0, 0.0);
        for ((name, _), ((_, side, size), shell)) in self.actions().into_iter().zip(self.panels().into_iter().zip(self.shells())) {
            match (shell, name.as_str(), side.as_str()) {
                (false, ..) => {}
                (_, n, _) if n == "shell-dock" || crate::shell_ui::tab_id(n).is_some() => {}
                (_, "shell-files", _) => e.2 += if sh.files { sh.files_w } else { 0.0 },
                (_, "shell-chat", _) => e.1 += if sh.chat { sh.chat_w } else { 0.0 },
                (_, _, "top") => e.0 += size,
                (_, _, "right") => e.1 += size,
                _ => e.2 += size,
            }
        }
        e
    }
    /// The dock's edge and depth while it's shown (the shell's dock is in this layout: shell_ui::docked).
    fn dock(&self, sh: crate::shell_ui::Shown) -> Option<(crate::shell_ui::Side, f64)> {
        (sh.dock && self.actions().iter().any(|(n, _)| n == "shell-dock")).then_some((sh.dock_side, sh.dock_size))
    }
    /// The box the shell leaves the space (inside its header, files, chat and dock): (top, right, bottom, left).
    fn frame(&self, sh: crate::shell_ui::Shown) -> (f64, f64, f64, f64) {
        use crate::shell_ui::Side;
        let (st, sr, sl) = self.shell_edges(sh);
        let mut f = (st, sr, 0.0, sl);
        match self.dock(sh) {
            Some((Side::Top, d)) => f.0 += d,
            Some((Side::Right, d)) => f.1 += d,
            Some((Side::Bottom, d)) => f.2 += d,
            Some((Side::Left, d)) => f.3 += d,
            None => {}
        }
        f
    }
    /// The dock's box as CSS (its bar and the terminal under it: `under` points down from its top), or None hidden;
    /// with `cell`, a controller's part of the terminal (fractions x0, y0, x1, y1), a point between it and the next.
    fn dock_box(&self, top: f64, sh: crate::shell_ui::Shown, under: f64, cell: [f64; 4]) -> Option<String> {
        use crate::shell_ui::Side;
        let (st, sr, sl) = self.shell_edges(sh);
        let (side, d) = self.dock(sh)?;
        let y = top + st + under;
        // left, top, width and height, each a share of .stage's and points: (percent, px)
        let (l, t, w, h) = match side {
            Side::Bottom => ((0.0, sl), (100.0, under - d), (100.0, -sl - sr), (0.0, d - under)),
            Side::Top => ((0.0, sl), (0.0, y), (100.0, -sl - sr), (0.0, d - under)),
            Side::Left => ((0.0, sl), (0.0, y), (0.0, d), (100.0, -y)),
            Side::Right => ((100.0, -sr - d), (0.0, y), (0.0, d), (100.0, -y)),
        };
        let css = |(p, px): (f64, f64)| if p == 0.0 { format!("{px}px") } else { format!("calc({p}% + {px}px)") };
        let at = |(p, px): (f64, f64), (dp, dpx): (f64, f64), f: f64| (p + dp * f, px + dpx * f);
        let gap = |end: f64| if end < 1.0 { 1.0 } else { 0.0 };
        let (x0, y0, x1, y1) = (cell[0], cell[1], cell[2], cell[3]);
        let width = (w.0 * (x1 - x0), w.1 * (x1 - x0) - gap(x1));
        let height = (h.0 * (y1 - y0), h.1 * (y1 - y0) - gap(y1));
        Some(format!("left: {}; top: {}; width: {}; height: {}", css(at(l, w, x0)), css(at(t, h, y0)), css(width), css(height)))
    }
    /// The dock's inner edge, dragged to size it, as CSS; and whether it runs across (the dock top or bottom).
    pub fn dock_edge(&self, top: f64, sh: crate::shell_ui::Shown) -> Option<(String, bool)> {
        use crate::shell_ui::Side;
        let (st, sr, sl) = self.shell_edges(sh);
        let (side, d) = self.dock(sh)?;
        Some(match side {
            Side::Bottom => (format!("left: {sl}px; right: {sr}px; bottom: {}px", d - 3.0), true),
            Side::Top => (format!("left: {sl}px; right: {sr}px; top: {}px", top + st + d - 3.0), true),
            Side::Left => (format!("left: {}px; top: {}px; bottom: 0", sl + d - 3.0, top + st), false),
            Side::Right => (format!("right: {}px; top: {}px; bottom: 0", sr + d - 3.0, top + st), false),
        })
    }
    /// The stage's box inside .stage, after the shell and the space's panels: (top, right, bottom, left) in points.
    pub fn insets(&self, sh: crate::shell_ui::Shown) -> (f64, f64, f64, f64) {
        let mut e = self.frame(sh);
        for ((_, side, size), shell) in self.panels().into_iter().zip(self.shells()) {
            match (shell, side.as_str()) {
                (true, _) | (_, "sheet") => {}
                (_, "top") => e.0 += size + GAP,
                (_, "right") => e.1 += size + GAP,
                (_, "bottom") => e.2 += size + GAP,
                _ => e.3 += size + GAP,
            }
        }
        e
    }
    /// Each panel's box as CSS inside .stage, its regions starting `top` points down (under the cube's row). The shell
    /// is outermost: its header across the top, the files and the chat down the sides under it (a hidden one: not
    /// shown). The space's toolbars span the width the shell leaves; its side panels run between them.
    pub fn boxes(&self, top: f64, sh: crate::shell_ui::Shown) -> Vec<String> {
        let (st, _, _) = self.shell_edges(sh);
        let (ft, fr, fb, fl) = self.frame(sh);
        let (t, _, b, _) = self.insets(sh);
        let mut at = (0.0, 0.0, 0.0, 0.0);
        let mut shell_at = (0.0, 0.0, 0.0);
        self.panels()
            .into_iter()
            .zip(self.heights())
            .zip(self.shells())
            .zip(self.actions())
            .zip(self.cells())
            .map(|(((((_, side, size), height), shell), (name, _)), cell)| match (shell, name.as_str(), side.as_str()) {
                // the dock: its bar, and under it the terminal shown, each controller in its part (the others, and all
                // of it hidden: not shown)
                (true, "shell-dock", _) => self.dock_box(top, sh, 0.0, [0.0, 0.0, 1.0, 1.0]).map(|b| format!("{b}; {FLAT}")).unwrap_or_else(|| HIDDEN.into()),
                (true, n, _) if crate::shell_ui::tab_id(n).is_some() => match self.dock_box(top, sh, DOCK_BAR, cell.unwrap_or([0.0, 0.0, 1.0, 1.0])) {
                    Some(b) if crate::shell_ui::tab_id(n) == Some(sh.dock_active) => format!("{b}; {FLAT}; background: transparent"),
                    _ => HIDDEN.into(),
                },
                (true, "shell-files", _) => if sh.files { format!("left: 0; width: {}px; top: {}px; bottom: 0; {FLAT}", sh.files_w, top + st) } else { "left: 0; width: 0; top: 0; bottom: 0; display: none".into() },
                (true, "shell-chat", _) => if sh.chat { format!("right: 0; width: {}px; top: {}px; bottom: 0; {FLAT}", sh.chat_w, top + st) } else { "right: 0; width: 0; top: 0; bottom: 0; display: none".into() },
                (true, _, "top") => { let y = top + shell_at.0; shell_at.0 += size; format!("left: 0; right: 0; top: {y}px; height: {size}px; {FLAT}") }
                (true, _, "right") => { let x = shell_at.1; shell_at.1 += size; format!("right: {x}px; width: {size}px; top: {}px; bottom: 0", top + st) }
                (true, ..) => { let x = shell_at.2; shell_at.2 += size; format!("left: {x}px; width: {size}px; top: {}px; bottom: 0", top + st) }
                // over the stage, centred in the window's content (shown only while open)
                (_, _, "sheet") => format!("left: calc(50% - {}px); top: calc(50% - {}px); width: {size}px; height: {height}px; z-index: 20; box-shadow: 0 18px 60px rgba(0,0,0,0.6)", size / 2.0, height / 2.0),
                (_, _, "top") => { let y = top + ft + at.0; at.0 += size + GAP; format!("left: {fl}px; right: {fr}px; top: {y}px; height: {size}px") }
                (_, _, "bottom") => { let y = fb + at.2; at.2 += size + GAP; format!("left: {fl}px; right: {fr}px; bottom: {y}px; height: {size}px") }
                (_, _, "right") => { let x = fr + at.1; at.1 += size + GAP; format!("right: {x}px; width: {size}px; top: {}px; bottom: {b}px", top + t) }
                _ => { let x = fl + at.3; at.3 += size + GAP; format!("left: {x}px; width: {size}px; top: {}px; bottom: {b}px", top + t) }
            })
            .collect()
    }
}

/// A box not shown.
const HIDDEN: &str = "left: 0; width: 0; top: 0; height: 0; display: none";
/// The dock's bar (its tabs, +, ⇅) over the terminal shown, in points.
pub const DOCK_BAR: f64 = 30.0;

/// Steps in a row that left a panel's page as it was, after which it's settled (not stepped till something reaches it).
const SETTLED: u32 = 3;

/// Every shown panel stepped and painted every frame, as before panels settled (RAEA_VOID_PANELS_EVERY_FRAME=1: for
/// an A/B of what settling saves, or a panel that settles wrongly).
fn every_frame() -> bool {
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ON.get_or_init(|| std::env::var_os("RAEA_VOID_PANELS_EVERY_FRAME").is_some_and(|v| v == "1"))
}

/// A shell panel's box: square, edge to edge with the window.
const FLAT: &str = "border-radius: 0; box-shadow: none";

/// The gap between regions, in points.
const GAP: f64 = 8.0;

/// The open space's layout. void always runs one (spaces/void unless another was chosen); None only while the first
/// is on its way at launch, or when it failed to load.
pub static CURRENT: GlobalSignal<Option<Layout>> = Signal::global(|| None);

/// The launch's space still loading. Until it lands (or fails), the app's own parts that a space stands in for (its
/// terminal, the cube's row, the player bar) aren't drawn, so they don't flash up before the space's own; a space that
/// fails to load leaves them, so the person can still type.
pub static LOADING: GlobalSignal<bool> = Signal::global(|| true);

/// No space open yet, and the launch's on its way.
pub fn waiting() -> bool {
    CURRENT.read().is_none() && *LOADING.read()
}

/// The open space as the agent sees it (the brain runs off the UI thread, so not CURRENT): its agent text and its
/// panels' actions.
static AGENT: Mutex<Option<(String, Vec<(String, Vec<ActionSpec>)>)>> = Mutex::new(None);

/// What the agent reads about the open space, if one is open.
pub fn agent_text() -> Option<String> {
    AGENT.lock().unwrap().as_ref().map(|a| a.0.clone())
}

/// Panel `name`'s actions, read after it opened (the dock's: shell_ui.rs reads them off the UI thread).
pub fn set_actions(name: &str, acts: &[ActionSpec]) {
    if let Some(a) = AGENT.lock().unwrap().as_mut() {
        a.1.iter_mut().filter(|(n, _)| n == name).for_each(|(_, x)| *x = acts.to_vec());
    }
}

/// The open space's panels' actions: (panel, action).
pub fn agent_actions() -> Vec<(String, ActionSpec)> {
    AGENT.lock().unwrap().as_ref().map(|a| a.1.iter().flat_map(|(p, acts)| acts.iter().map(move |x| (p.clone(), x.clone()))).collect()).unwrap_or_default()
}

/// What void tells every panel (`void:transcript JSON`, `void:object JSON`; @raea/motion voidIn reads them): handed to
/// each as keys for one frame when it changes and when the panel opens, never held (a live panel's input is recorded
/// every frame, and a transcript held would be kilobytes a frame), so a panel keeps what it was told in its state.
static OPEN: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Whether a space's panels are playing.
pub fn running() -> bool {
    OPEN.load(std::sync::atomic::Ordering::Relaxed)
}

/// What the object on the stage publishes, handed to the panels as `void:stage JSON` once per change (a layers panel
/// listing the object's shapes reads it with @raea/motion voidIn(input, "stage")).
pub fn stage(published: Option<String>) {
    static LAST: Mutex<Option<String>> = Mutex::new(None);
    let mut last = LAST.lock().unwrap();
    if *last != published {
        *last = published.clone();
        send(Cmd::Stage(published));
    }
}

pub fn context(keys: Vec<String>) {
    // kept, by name, for a panel thread started later: the stage opens before the panels do as void starts, and the
    // timeline it sends then was lost (an edit's timeline showed no clips till it was baked again)
    {
        let mut kept = CONTEXT.lock().unwrap();
        for k in &keys {
            let name = k.split(' ').next().unwrap_or("");
            match kept.iter_mut().find(|c| c.split(' ').next() == Some(name)) {
                Some(c) => c.clone_from(k),
                None => kept.push(k.clone()),
            }
        }
    }
    if ENGINE.lock().unwrap().is_some() {
        send(Cmd::Context(keys));
    }
}

/// What void has told the panels (void:timeline, void:sounds), each by its latest value: where a panel thread starts.
static CONTEXT: Mutex<Vec<String>> = Mutex::new(Vec::new());

/// What panels asked void's chrome to do (publishing { do: { n, what, x?, y?, arg? } }): (panel, what, x, y, arg), for
/// the app to run (ui.rs: stop, listen, chats, settings, objects, open:SHEET, close, open-object, ...); taken once.
static DOES: Mutex<Vec<(usize, String, f64, f64, String)>> = Mutex::new(Vec::new());

pub fn take_does() -> Vec<(usize, String, f64, f64, String)> {
    std::mem::take(&mut *DOES.lock().unwrap())
}

/// Messages a panel sent (a terminal of the space's own publishing { send: { n, text } }), for the app to submit as if
/// typed; taken once.
static SENDS: Mutex<Vec<String>> = Mutex::new(Vec::new());

pub fn take_sends() -> Vec<String> {
    std::mem::take(&mut *SENDS.lock().unwrap())
}

/// Run a panel's action, as a click there would: handed to the panel as input on its next frame. The panel by name (or
/// None: the only panel with an action of that name).
pub fn act(panel: Option<&str>, action: &str, args: &serde_json::Value) -> Result<String, String> {
    let g = AGENT.lock().unwrap();
    let panels = &g.as_ref().ok_or("no space is open (only void's own; /space switches)")?.1;
    let hits: Vec<usize> = panels.iter().enumerate().filter(|(_, (p, acts))| panel.is_none_or(|w| w == p) && acts.iter().any(|a| a.name == action)).map(|(i, _)| i).collect();
    let i = match hits.as_slice() {
        [i] => *i,
        [] => return Err(format!("no panel has an action {action}{}", panel.map(|p| format!(" (looked in {p})")).unwrap_or_default())),
        _ => return Err(format!("several panels have {action}: say which ({})", hits.iter().map(|i| panels[*i].0.clone()).collect::<Vec<_>>().join(", "))),
    };
    let name = panels[i].0.clone();
    drop(g);
    send(Cmd::Action(i, format!("action:{action} {args}")));
    Ok(format!("{name}.{action} handed in"))
}

/// The sheets open now, by panel index (<Sheet>: shown over the stage while open).
pub static SHEETS: GlobalSignal<Vec<usize>> = Signal::global(Vec::new);

/// Sheet i opened or closed: shown, stepped, and given the keys while it's open (back to the space's terminal after).
pub fn show_sheet(i: usize, on: bool) {
    let mut open = SHEETS.write();
    open.retain(|&j| j != i);
    if on {
        open.push(i);
    }
    drop(open);
    crate::mcp::log(format!("panels: sheet {i} {}", if on { "opened" } else { "closed" }));
    send(Cmd::Shown(i, on));
    *FOCUS.write() = if on { Some(i) } else { CURRENT.peek().as_ref().and_then(|l| l.terminal_panel()) };
}

/// Panel i shown or hidden (a shell panel: apps/void/src/shell_ui.rs), stepped only while shown.
pub fn show_panel(i: usize, on: bool) {
    send(Cmd::Shown(i, on));
}

/// Where each panel said a press moves the window (publishing { drag: [[x0, y0, x1, y1], ...] } in its points): a
/// space's header is the window's title bar.
static DRAGS: Mutex<Vec<(usize, Vec<[f64; 4]>)>> = Mutex::new(Vec::new());

/// Whether a press at (x, y) on panel i moves the window.
pub fn drags_at(i: usize, x: f64, y: f64) -> bool {
    DRAGS.lock().unwrap().iter().any(|(j, areas)| *j == i && areas.iter().any(|a| x >= a[0] && y >= a[1] && x < a[2] && y < a[3]))
}

/// The panel that last took a click: keys typed go to it rather than the stage, until the stage is clicked.
pub static FOCUS: GlobalSignal<Option<usize>> = Signal::global(|| None);

pub fn here() -> PathBuf {
    std::env::var_os("RAEA_VOID_DIR").map(PathBuf::from).unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")))
}

/// The spaces there are: (slug, folder), void's own first, a workspace's of the same slug in their place.
pub fn list(root: Option<&Path>) -> Vec<(String, PathBuf)> {
    let mut out: Vec<(String, PathBuf)> = Vec::new();
    let mut dirs = vec![here().join("spaces")];
    if let Some(r) = root {
        dirs.push(r.join("spaces"));
    }
    for d in dirs {
        let Ok(rd) = std::fs::read_dir(&d) else { continue };
        let mut found: Vec<(String, PathBuf)> = rd
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.join("space.tsx").is_file())
            .filter_map(|p| Some((p.file_name()?.to_string_lossy().into_owned(), p)))
            .collect();
        found.sort();
        for (slug, p) in found {
            match out.iter_mut().find(|(s, _)| *s == slug) {
                Some(x) => x.1 = p,
                None => out.push((slug, p)),
            }
        }
    }
    out
}

/// The space by slug or name (case and punctuation aside).
pub fn find(root: Option<&Path>, name: &str) -> Option<(String, PathBuf)> {
    let want = crate::spaces::slug(name);
    list(root).into_iter().find(|(s, _)| *s == want)
}

/// A space made yours: `from` (a space's slug or name, or any folder with a space.tsx someone passed on) copied into
/// the workspace's spaces/ as `to` (its own slug by default, so the copy stands in for void's), generated files and
/// node_modules left behind. Sharing a space is passing that folder on. Its (slug, folder).
pub fn fork(root: &Path, from: &str, to: Option<&str>) -> Result<(String, PathBuf), String> {
    let given = PathBuf::from(shellexpand(from));
    let (slug, src) = if given.join("space.tsx").is_file() {
        (crate::spaces::slug(&given.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()), given)
    } else {
        find(Some(root), from).ok_or_else(|| format!("no space called “{from}”"))?
    };
    let slug = to.map(crate::spaces::slug).filter(|s| !s.is_empty()).unwrap_or(slug);
    let dest = root.join("spaces").join(&slug);
    if dest.exists() {
        return Err(format!("{} is there already: fork it as another name (/space fork {from} as NEW)", dest.display()));
    }
    copy_space(&src, &dest).map_err(|e| format!("copying {} to {}: {e}", src.display(), dest.display()))?;
    Ok((slug, dest))
}

fn shellexpand(s: &str) -> String {
    match (s.strip_prefix("~/"), std::env::var("HOME")) {
        (Some(rest), Ok(home)) => format!("{home}/{rest}"),
        _ => s.to_string(),
    }
}

fn copy_space(src: &Path, dest: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(dest)?;
    for e in std::fs::read_dir(src)?.flatten() {
        let name = e.file_name().to_string_lossy().into_owned();
        if name.starts_with(".panel-") || name == "node_modules" || name == ".DS_Store" {
            continue;
        }
        let (from, to) = (e.path(), dest.join(&name));
        if e.file_type()?.is_dir() {
            copy_space(&from, &to)?;
        } else {
            std::fs::copy(&from, &to)?;
        }
    }
    Ok(())
}

/// The newest change to a space's own files (its space.tsx, panels, sheets, agent files; not what void writes there,
/// dotfiles, or node_modules): a change reloads the space in place.
pub fn stamp(folder: &Path) -> std::time::SystemTime {
    fn walk(d: &Path, newest: &mut std::time::SystemTime) {
        let Ok(rd) = std::fs::read_dir(d) else { return };
        for e in rd.flatten() {
            let name = e.file_name();
            let name = name.to_string_lossy();
            if name.starts_with('.') || name == "node_modules" {
                continue;
            }
            let Ok(md) = e.metadata() else { continue };
            if md.is_dir() {
                walk(&e.path(), newest);
            } else if let Ok(m) = md.modified() {
                *newest = (*newest).max(m);
            }
        }
    }
    let mut newest = std::time::SystemTime::UNIX_EPOCH;
    walk(folder, &mut newest);
    newest
}

/// A space's layout, read from its space.tsx with bun.
pub fn load(slug: &str, folder: &Path) -> Result<Layout, String> {
    let out = std::process::Command::new("bun")
        .arg(here().join("kit/load.ts"))
        .arg(folder)
        .output()
        .map_err(|e| format!("bun: {e}"))?;
    let text = String::from_utf8_lossy(&out.stdout);
    let v: serde_json::Value = serde_json::from_str(text.trim()).map_err(|_| format!("reading {}: {}{}", folder.join("space.tsx").display(), text.trim(), String::from_utf8_lossy(&out.stderr).trim()))?;
    if let Some(e) = v.get("error").and_then(|e| e.as_str()) {
        return Err(e.to_string());
    }
    let mut l: Layout = serde_json::from_value(v).map_err(|e| format!("space.tsx's layout: {e}"))?;
    l.folder = folder.to_path_buf();
    l.slug = slug.to_string();
    Ok(l)
}

/// The texture panel `i` is painted into (the stage's are u64::MAX - 3 .. - 11).
pub fn texture(i: usize) -> u64 {
    u64::MAX - 200 - i as u64
}

/// What each panel is doing: its page's size in pixels (0 until its first paint) and its last error.
#[derive(Clone, Debug, Default)]
pub struct PanelStatus {
    pub page: (u32, u32),
    pub error: Option<String>,
    /// frames it was stepped, and painted (a settled panel is neither: apps/void/shell.feature, Only what's seen is drawn)
    pub steps: u64,
    pub paints: u64,
}

enum Cmd {
    /// the open space's panels (their component files and names), or none
    Set(Vec<(PathBuf, String, bool)>),
    /// panel i shown or not (a sheet, closed: not stepped)
    Shown(usize, bool),
    /// panel i's box in points, and the screen's scale
    Rect(usize, f64, f64, f64),
    Pointer(usize, f64, f64, bool),
    Key(usize, String, bool),
    /// an action handed to panel i: a key held for one frame (@raea/motion actionsIn reads it)
    Action(usize, String),
    /// what void tells the panels (context)
    Context(Vec<String>),
    /// what the object on the stage publishes
    Stage(Option<String>),
    /// panel i painted on its next frame, changed or not
    Repaint(usize),
}

struct Engine {
    tx: Sender<Cmd>,
    status: Arc<Mutex<Vec<PanelStatus>>>,
}

static ENGINE: Mutex<Option<Engine>> = Mutex::new(None);

fn send(c: Cmd) {
    let mut g = ENGINE.lock().unwrap();
    if g.is_none() {
        let (tx, rx) = channel();
        let status = Arc::new(Mutex::new(Vec::new()));
        let st = status.clone();
        if std::thread::Builder::new().name("void panels".into()).spawn(move || run(rx, st)).is_err() {
            return;
        }
        *g = Some(Engine { tx, status });
    }
    if let Some(e) = g.as_ref() {
        let _ = e.tx.send(c);
    }
}

/// Panel sets sent so far: a new set's panels open only when told their boxes, so each panel's box is told again
/// after every set (ui.rs PanelView), even when it's the same box as the last space's panel there (a space switched for
/// one laid out the same, as a fork of void is).
pub static SETS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// Open the layout's panels (closing the last space's), or close them all.
pub fn open(l: Option<&Layout>) {
    *AGENT.lock().unwrap() = l.map(|l| (l.agent_text(), l.actions()));
    // a sheet starts closed, and a shell panel hidden if it was (⌘B, ⌘J)
    let shown = *crate::shell_ui::SHOWN.peek();
    let tab = |name: &str| crate::shell_ui::tab_id(name).is_some_and(|id| !shown.dock || id != shown.dock_active);
    let hidden = |name: &str, side: &str| side == "sheet" || (name == "shell-files" && !shown.files) || (name == "shell-chat" && !shown.chat) || (name == "shell-dock" && !shown.dock) || tab(name);
    let files: Vec<(PathBuf, String, bool)> = l.map(|l| l.panels().into_iter().zip(l.actions()).map(|(p, a)| { let h = hidden(&a.0, &p.1); (p.0, a.0, h) }).collect()).unwrap_or_default();
    OPEN.store(!files.is_empty(), std::sync::atomic::Ordering::Relaxed);
    SHEETS.write().clear();
    if files.is_empty() && ENGINE.lock().unwrap().is_none() {
        return;
    }
    send(Cmd::Set(files));
    SETS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
}

pub fn rect(i: usize, w: f64, h: f64, scale: f64) {
    send(Cmd::Rect(i, w, h, scale));
}

/// The pointer over panel i, in points from its top-left (the panel's CSS pixels).
pub fn pointer(i: usize, x: f64, y: f64, down: bool) {
    send(Cmd::Pointer(i, x, y, down));
}

/// Text pasted into panel i (Cmd-V while it has the keys), handed in for one frame as `void:paste "TEXT"`.
pub fn paste(i: usize, text: &str) {
    crate::mcp::log(format!("panels: panel {i} pasted {} characters", text.chars().count()));
    if let Some(id) = cli_of(i) {
        return crate::pty::paste(id, text);
    }
    send(Cmd::Action(i, format!("void:paste {}", serde_json::Value::from(text))));
}

/// The wheel turned over panel i: points scrolled (down positive), handed in for one frame as `void:wheel DY`.
pub fn wheel(i: usize, dy: f64) {
    send(Cmd::Action(i, format!("void:wheel {dy}")));
}

/// The shell panel i draws, if it's a cli terminal in the dock (its keys and paste go to the shell: pty.rs).
pub fn cli_of(i: usize) -> Option<u64> {
    crate::shell_ui::pty_of(&CURRENT.peek().as_ref()?.actions().get(i)?.0)
}

pub fn key(i: usize, name: &str, down: bool) {
    send(Cmd::Key(i, name.to_string(), down));
}

pub fn status() -> Vec<PanelStatus> {
    ENGINE.lock().unwrap().as_ref().map(|e| e.status.lock().unwrap().clone()).unwrap_or_default()
}

/// Panel i's page read back to a PNG (the MCP's screenshot of a panel, tests).
pub fn snapshot(i: usize, path: &Path) -> Result<(u32, u32), String> {
    // a settled panel isn't painted, and the engine lets an unbound target go (parked, 240 frames): what the chrome
    // shows it keeps, but a read finds nothing, so the panel is painted again for the next try
    let (w, h, px) = raea::quads::read_texture(texture(i)).inspect_err(|_| send(Cmd::Repaint(i)))?;
    crate::stage_native::save_rgba(path, w, h, &px)?;
    Ok((w, h))
}

/// The composition a panel is played as: its component at its box in CSS pixels, scaled by the screen's factor, as an
/// endless game. Written at the space's root as a dotfile (the bundler copies the entry's folder, dotfiles aside, so
/// the panel's own imports resolve as they do in the space).
fn entry(src: &Path, scale: f64) -> Result<PathBuf, String> {
    // a space's folder; a shell panel's own (apps/void/shell/, a workspace's shell/)
    let dir = src.ancestors().skip(1).find(|d| d.join("space.tsx").is_file()).or(src.parent()).ok_or("a panel with no folder")?.to_path_buf();
    let stem = src.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_else(|| "panel".into());
    let out = dir.join(format!(".panel-{stem}.tsx"));
    let rel = pathdiff(src, &dir);
    let k = if scale > 0.0 { scale } else { 2.0 };
    let text = format!(
        r#"// void writes this (space_ui.rs): the panel {rel} played live at its box, in CSS pixels scaled by the screen's factor.
import {{ Composition, registerRoot, useVideoConfig }} from "@raea/motion";
import Panel from "{rel}";

const K = {k};
function Box() {{
  const {{ width, height }} = useVideoConfig();
  return (
    <div style={{{{ position: "absolute", left: 0, top: 0, width: width / K, height: height / K, transform: `scale(${{K}})`, transformOrigin: "0 0", overflow: "hidden" }}}}>
      <Panel width={{width / K}} height={{height / K}} />
    </div>
  );
}}

registerRoot(() => <Composition id="edit" component={{Box}} width={{480}} height={{960}} fps={{60}} durationInFrames={{1000000000}} />);
"#
    );
    if std::fs::read_to_string(&out).ok().as_deref() != Some(text.as_str()) {
        std::fs::write(&out, text).map_err(|e| e.to_string())?;
    }
    Ok(out)
}

/// `to` relative to the folder `from`, as an import path.
fn pathdiff(to: &Path, from: &Path) -> String {
    let a: Vec<_> = to.components().collect();
    let b: Vec<_> = from.components().collect();
    let n = a.iter().zip(&b).take_while(|(x, y)| x == y).count();
    let mut s = String::new();
    for _ in n..b.len() {
        s.push_str("../");
    }
    let rest: Vec<String> = a[n..].iter().map(|c| c.as_os_str().to_string_lossy().into_owned()).collect();
    if s.is_empty() {
        s.push_str("./");
    }
    s + rest.join("/").as_str()
}

struct Panel {
    src: PathBuf,
    name: String,
    /// the key held on the stage for what it publishes (`space:NAME JSON`), if it publishes
    held: Option<String>,
    live: Option<raea_react::live::Live>,
    /// its box in points and the screen's scale (the page is the box in pixels)
    rect: (f64, f64, f64),
    keys: Vec<String>,
    /// actions handed in, and keys pressed, each held for the next frame at least (a key let go within a frame still
    /// reaches the panel)
    once: Vec<String>,
    /// the keys handed in the frame before: a key pressed again (a second press, the key repeating) waits a frame
    /// without it, so the panel sees it go down again rather than held
    last: Vec<String>,
    /// the last message it sent (publishing { send: { n, text } }): a higher n is a new one
    sent: i64,
    /// the last control of the stage's player it published ({ control: { n, do, frame } }), and the player's state as
    /// last handed in (void:player)
    controlled: i64,
    player: String,
    /// the last { do } it published
    done: i64,
    /// void's context is due on its next frame (it changed, or the panel just opened)
    ctx_due: bool,
    pointer: Option<(f64, f64, bool)>,
    /// presses not yet stepped: each one's frame sees the button down even if it was let go before (a quick click)
    press: std::collections::VecDeque<(f64, f64)>,
    /// a sheet while it's closed: not stepped (what's handed in waits for it)
    hidden: bool,
    /// the last frame stepped was a queued press (the next one lets go before another press goes in)
    pressed: bool,
    frame: i64,
    failed: bool,
    /// frames in a row its page came out of a step unchanged, the pointer as of its last step, and when it was last
    /// stepped: a panel nothing reaches and whose page has settled isn't stepped or painted (apps/void/shell.feature: Only
    /// what's seen is drawn), save once a second for what reads the clock
    quiet: u32,
    seen: Option<(f64, f64, bool)>,
    stepped: Instant,
    /// painted on its next step whether or not its page changed (shown again: its texture may be gone)
    repaint: bool,
    /// a cli terminal's shell (pty.rs), the generation of its screen last handed in, and wheel turned short of a line
    pty: Option<u64>,
    pty_seen: u64,
    wheel: f64,
}

fn run(rx: Receiver<Cmd>, status: Arc<Mutex<Vec<PanelStatus>>>) {
    let mut panels: Vec<Panel> = Vec::new();
    let tick = Duration::from_secs_f64(1.0 / 60.0);
    let mut next = Instant::now();
    let mut last_press = Instant::now();
    let mut ctx: Vec<String> = CONTEXT.lock().unwrap().clone();
    let mut stage: Option<String> = None;
    loop {
        // commands until the next frame is due (none open: wait for one)
        let wait = if panels.iter().any(|p| p.live.is_some()) { next.saturating_duration_since(Instant::now()) } else { Duration::from_secs(3600) };
        let first = match rx.recv_timeout(wait) {
            Ok(c) => Some(c),
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => None,
            Err(_) => return,
        };
        for c in first.into_iter().chain(rx.try_iter()) {
            match c {
                // a panel of the last set with the same file and name is kept as it is (the shell's, the dock's terminals
                // still open), painted again where it now is; the rest are opened when told their boxes
                Cmd::Set(files) => {
                    let mut old: Vec<Option<Panel>> = std::mem::take(&mut panels).into_iter().map(Some).collect();
                    let was = std::mem::take(&mut *status.lock().unwrap());
                    let mut st = Vec::new();
                    for (src, name, hidden) in files {
                        let kept = old.iter().position(|p| p.as_ref().is_some_and(|p| p.src == src && p.name == name));
                        match kept {
                            Some(j) => {
                                let mut p = old[j].take().unwrap();
                                p.hidden = hidden;
                                p.repaint = true;
                                p.quiet = 0;
                                p.ctx_due = true;
                                panels.push(p);
                                st.push(was.get(j).cloned().unwrap_or_default());
                            }
                            None => {
                                let pty = crate::shell_ui::pty_of(&name);
                                panels.push(Panel { src, name, hidden, held: None, live: None, rect: (0.0, 0.0, 0.0), keys: Vec::new(), once: Vec::new(), last: Vec::new(), sent: 0, controlled: 0, player: String::new(), done: 0, ctx_due: true, pointer: None, press: Default::default(), pressed: false, frame: -1, failed: false, quiet: 0, seen: None, stepped: Instant::now(), repaint: false, pty, pty_seen: 0, wheel: 0.0 });
                                st.push(PanelStatus::default());
                            }
                        }
                    }
                    for p in old.into_iter().flatten() {
                        if let Some(k) = &p.held {
                            crate::stage_native::key(k, false);
                        }
                    }
                    DRAGS.lock().unwrap().clear();
                    *status.lock().unwrap() = st;
                }
                Cmd::Rect(i, w, h, k) => {
                    let Some(p) = panels.get_mut(i) else { continue };
                    if p.rect == (w, h, k) || w < 1.0 || h < 1.0 {
                        continue;
                    }
                    // a cli's shell is its box in cells: started the first time it's laid out
                    if let Some(id) = p.pty {
                        let (c, r) = crate::pty::cells(w, h);
                        if let Err(e) = crate::pty::size(id, c, r, crate::shell_ui::root_path().as_deref()) {
                            crate::mcp::log(format!("cli {id}: {e}"));
                        }
                    }
                    let px = ((w * k).round() as u32, (h * k).round() as u32);
                    let rescaled = p.rect.2 != k;
                    p.rect = (w, h, k);
                    match p.live.as_mut() {
                        Some(l) if !rescaled => {
                            l.resize(px.0, px.1);
                            p.quiet = 0;
                        }
                        _ => {
                            p.failed = false;
                            let t = Instant::now();
                            let opened = entry(&p.src, k).and_then(|e| raea_react::live::Live::open_with(&e, px, true));
                            match opened {
                                Ok(l) => {
                                    crate::mcp::log(format!("panels: {} open in {:.0} ms at {}x{}", p.src.display(), t.elapsed().as_secs_f64() * 1e3, l.w, l.h));
                                    p.live = Some(l);
                                    p.frame = -1;
                                    p.ctx_due = true;
                                    p.player.clear();
                                    status.lock().unwrap()[i].error = None;
                                }
                                Err(e) => {
                                    crate::mcp::log(format!("panels: {}: {e}", p.src.display()));
                                    p.failed = true;
                                    status.lock().unwrap()[i].error = Some(e);
                                }
                            }
                        }
                    }
                }
                Cmd::Pointer(i, x, y, down) => {
                    if let Some(p) = panels.get_mut(i) {
                        if down && !p.pointer.is_some_and(|q| q.2) {
                            p.press.push_back((x, y));
                        }
                        p.pointer = Some((x, y, down));
                    }
                }
                Cmd::Stage(v) => {
                    let key = v.map(|v| format!("void:stage {v}"));
                    if key != stage {
                        stage = key;
                        panels.iter_mut().for_each(|p| p.ctx_due = true);
                    }
                }
                Cmd::Repaint(i) => {
                    if let Some(p) = panels.get_mut(i) {
                        p.repaint = true;
                        p.quiet = 0;
                    }
                }
                Cmd::Shown(i, on) => {
                    if let Some(p) = panels.get_mut(i) {
                        p.hidden = !on;
                        p.quiet = 0;
                        p.repaint |= on;
                    }
                }
                // merged by name (void:transcript, void:chats, ...): each of void's effects tells its own part
                Cmd::Context(keys) => {
                    let name = |k: &str| k.split(' ').next().unwrap_or("").to_string();
                    let mut changed = false;
                    for k in keys {
                        match ctx.iter_mut().find(|c| name(c) == name(&k)) {
                            Some(c) if *c == k => {}
                            Some(c) => { *c = k; changed = true; }
                            None => { ctx.push(k); changed = true; }
                        }
                    }
                    if changed {
                        panels.iter_mut().for_each(|p| p.ctx_due = true);
                    }
                }
                Cmd::Action(i, k) => {
                    if let Some(p) = panels.get_mut(i) {
                        // the wheel over a cli scrolls its shell's screen back, a line a cell high
                        if let (Some(id), Some(dy)) = (p.pty, k.strip_prefix("void:wheel ").and_then(|d| d.parse::<f64>().ok())) {
                            p.wheel -= dy / crate::pty::CELL_H;
                            let lines = p.wheel.trunc();
                            if lines != 0.0 {
                                p.wheel -= lines;
                                crate::pty::scroll(id, lines as i32);
                            }
                            continue;
                        }
                        p.once.push(k);
                    }
                }
                Cmd::Key(i, k, down) => {
                    if let Some(p) = panels.get_mut(i) {
                        p.keys.retain(|x| *x != k);
                        if down {
                            // pressed, or repeating while held: a press each time
                            p.once.push(k.clone());
                            p.keys.push(k);
                        }
                    }
                }
            }
        }
        if Instant::now() < next {
            continue;
        }
        next = (next + tick).max(Instant::now());
        let mut painted = false;
        // the held keys pressed again once a second: an object opened since started with no keys
        let again = last_press.elapsed() > Duration::from_secs(1);
        if again {
            last_press = Instant::now();
        }
        // the stage's player (a timeline's playhead, its length and rate, playing or not), handed in when it changes
        let st = crate::stage_native::status();
        let player = format!("void:player {}", serde_json::json!({"frame": st.frame, "frames": st.frames, "rate": st.rate, "playing": st.playing}));
        for (i, p) in panels.iter_mut().enumerate() {
            let Some(l) = p.live.as_mut() else { continue };
            if p.failed {
                continue;
            }
            if p.hidden && p.once.is_empty() {
                continue;
            }
            // a cli's screen, handed in when its shell drew (the panel keeps it in its state)
            if let Some((g, screen)) = p.pty.and_then(|id| crate::pty::screen_since(id, p.pty_seen)) {
                p.pty_seen = g;
                p.once.push(format!("void:pty {screen}"));
            }
            // settled and nothing reaching it (no press, key, action, context or pointer move): not stepped. The
            // stage's player moves every frame while something plays: a panel it changes (a playhead) isn't settled and
            // takes every frame; one it doesn't is woken by it four times a second
            let player_wakes = p.player != player && (p.quiet < SETTLED || p.stepped.elapsed() >= Duration::from_millis(250));
            let input = p.frame < 1 || !p.once.is_empty() || p.ctx_due || !p.press.is_empty() || p.pressed || !p.keys.is_empty() || p.pointer != p.seen;
            if !input && !player_wakes && p.quiet >= SETTLED && p.stepped.elapsed() < Duration::from_secs(1) && !every_frame() {
                continue;
            }
            // what reached it may change its page a frame or two later (React commits after the step that took it): it
            // settles again only after SETTLED steps that changed nothing
            if input {
                p.quiet = 0;
            }
            p.stepped = Instant::now();
            p.seen = p.pointer;
            p.frame += 1;
            // presses quicker than a frame each get one, with a frame let go between them: a second press before
            // the next frame took the first one's place (two clicks on a header, one heard)
            let pointer = match (p.pressed, p.press.front().copied()) {
                (true, Some(_)) => {
                    p.pressed = false;
                    p.pointer.map(|(x, y, _)| (x, y, false))
                }
                (_, Some((x, y))) => {
                    p.press.pop_front();
                    p.pressed = true;
                    Some((x, y, true))
                }
                (_, None) => {
                    p.pressed = false;
                    p.pointer
                }
            };
            let due = std::mem::take(&mut p.ctx_due) && (!ctx.is_empty() || stage.is_some());
            let moved = p.player != player;
            if moved {
                p.player.clone_from(&player);
            }
            let r = if p.once.is_empty() && !due && !moved {
                p.last.clone_from(&p.keys);
                l.step(p.frame, &p.keys, pointer)
            } else {
                // a press of a key handed in last frame waits: this frame goes without it, so it's seen going down again
                // (and a second press queued behind it waits for the frame after)
                let mut now: Vec<String> = Vec::new();
                let mut again: Vec<String> = Vec::new();
                for k in p.once.drain(..) {
                    if p.last.contains(&k) || now.contains(&k) || again.contains(&k) { again.push(k) } else { now.push(k) }
                }
                let mut keys: Vec<String> = p.keys.iter().filter(|k| !again.contains(k) || now.contains(k)).cloned().collect();
                for k in now {
                    if !keys.contains(&k) {
                        keys.push(k);
                    }
                }
                p.once = again;
                if due {
                    keys.extend(ctx.iter().cloned());
                    keys.extend(stage.iter().cloned());
                }
                if moved {
                    keys.push(player.clone());
                }
                let r = l.step(p.frame, &keys, pointer);
                p.last = keys;
                r
            };
            // painted only when its page changed (the first frame always)
            let changed = l.changed() || p.frame == 0 || std::mem::take(&mut p.repaint) || every_frame();
            p.quiet = if changed { 0 } else { p.quiet + 1 };
            let r = match r {
                Ok(()) if changed => l.paint_texture_fit(texture(i), (l.w, l.h)),
                Ok(()) => Ok(status.lock().unwrap()[i].page),
                Err(e) => Err(e),
            };
            // what it publishes, held on the stage as a key while it stands (@raea/motion spaceIn reads it)
            if r.is_ok() {
                let published = l.published();
                // a message to send (a terminal of the space's own)
                if let Some(v) = published.as_deref().and_then(|v| serde_json::from_str::<serde_json::Value>(v).ok()) {
                    // where a press moves the window
                    let areas: Vec<[f64; 4]> = v["drag"].as_array().map(|a| a.iter().filter_map(|r| {
                        let r = r.as_array()?;
                        Some([r.first()?.as_f64()?, r.get(1)?.as_f64()?, r.get(2)?.as_f64()?, r.get(3)?.as_f64()?])
                    }).collect()).unwrap_or_default();
                    {
                        let mut d = DRAGS.lock().unwrap();
                        d.retain(|(j, _)| *j != i);
                        if !areas.is_empty() {
                            d.push((i, areas));
                        }
                    }
                    // void's chrome asked to do something (a header or terminal of the space's own: stop, listen, ...)
                    // one ask, or a list of them (the last few: a sheet opens an object and closes in one frame)
                    let asks: Vec<&serde_json::Value> = match &v["do"] {
                        serde_json::Value::Array(a) => a.iter().collect(),
                        d => vec![d],
                    };
                    let seen = p.done;
                    for d in asks {
                        let dn = d["n"].as_i64().unwrap_or(0);
                        if dn > seen {
                            p.done = p.done.max(dn);
                            if let Some(what) = d["what"].as_str() {
                                DOES.lock().unwrap().push((i, what.to_string(), d["x"].as_f64().unwrap_or(0.0), d["y"].as_f64().unwrap_or(0.0), d["arg"].as_str().unwrap_or("").to_string()));
                                crate::win::redraw();
                            }
                        }
                    }
                    // the stage's player driven ({ control: { n, do: "play" | "pause" | "seek" | "playFrom", frame } }), each n
                    // once; playFrom seeks and plays in one (two asks in one step publish only the last)
                    let c = &v["control"];
                    let cn = c["n"].as_i64().unwrap_or(0);
                    if cn > p.controlled {
                        p.controlled = cn;
                        use crate::stage_native::{control, Control};
                        match (c["do"].as_str(), c["frame"].as_i64()) {
                            (Some("play"), _) => control(Control::Play),
                            (Some("pause"), _) => control(Control::Pause),
                            (Some("seek"), Some(f)) => control(Control::Seek(f.max(0))),
                            (Some("playFrom"), Some(f)) => {
                                control(Control::Seek(f.max(0)));
                                control(Control::Play);
                            }
                            _ => {}
                        }
                    }
                    let n = v["send"]["n"].as_i64().unwrap_or(0);
                    if n > p.sent {
                        p.sent = n;
                        if let Some(text) = v["send"]["text"].as_str().filter(|t| !t.trim().is_empty()) {
                            SENDS.lock().unwrap().push(text.to_string());
                            crate::win::redraw();
                        }
                    }
                }
                let key = published.map(|v| format!("space:{} {v}", p.name));
                if key != p.held {
                    if let Some(k) = &p.held {
                        crate::stage_native::key(k, false);
                    }
                    if let Some(k) = &key {
                        crate::stage_native::key(k, true);
                    }
                    p.held = key;
                } else if again {
                    if let Some(k) = &p.held {
                        crate::stage_native::key(k, true);
                    }
                }
            }
            let mut st = status.lock().unwrap();
            match r {
                Ok(size) => {
                    st[i].page = size;
                    st[i].steps += 1;
                    st[i].paints += changed as u64;
                    painted |= changed;
                }
                Err(e) => {
                    crate::mcp::log(format!("panels: {}: frame {}: {e}", p.src.display(), p.frame));
                    st[i].error = Some(e);
                    p.failed = true;
                }
            }
        }
        if painted {
            crate::win::redraw();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fork_copies_into_the_workspace() {
        let root = std::env::temp_dir().join(format!("void-fork-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let (slug, dest) = fork(&root, "Build 3D", Some("my 3d")).unwrap();
        assert_eq!(slug, "my-3d");
        assert!(dest.join("space.tsx").is_file() && dest.join("panels/outliner.tsx").is_file());
        assert!(std::fs::read_dir(&dest).unwrap().flatten().all(|e| !e.file_name().to_string_lossy().starts_with(".panel-")));
        assert_eq!(find(Some(&root), "my-3d").map(|x| x.1), Some(dest.clone()));
        // a folder passed on, by path, and a second fork onto the same name refused
        let (s2, _) = fork(&root, dest.to_str().unwrap(), Some("again")).unwrap();
        assert_eq!(s2, "again");
        assert!(fork(&root, "build-3d", Some("my-3d")).is_err());
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn build_3d_loads() {
        let (slug, folder) = find(None, "Build 3D").expect("void ships build-3d");
        let l = load(&slug, &folder).expect("its space.tsx reads");
        assert_eq!(l.title(), "Build 3D");
        // its two panels, then the shell's three (apps/void/kit/load.ts adds them to every space)
        assert_eq!(l.panels().len(), 5);
        assert_eq!(l.terminal().as_deref(), Some("right"));
        assert_eq!(l.agent.skills, vec!["raea-bevy".to_string()]);
        let sh = crate::shell_ui::Shown::default();
        // the header's 40 and the materials' 52 + 8 over it, the files' 260 and the outliner's 240 + 8 left, the chat's 380 right
        assert_eq!(l.insets(sh), (100.0, 380.0, 0.0, 508.0));
        assert_eq!(l.insets(crate::shell_ui::Shown { files: false, chat: false, ..sh }), (100.0, 0.0, 0.0, 248.0));
        let acts = l.actions();
        assert_eq!(acts.iter().map(|(p, a)| (p.as_str(), a.len())).collect::<Vec<_>>()[..2], vec![("materials", 1), ("outliner", 1)]);
        assert_eq!(acts.iter().map(|(p, _)| p.as_str()).collect::<Vec<_>>()[2..], ["shell-header", "shell-files", "shell-chat"]);
        assert!(l.agent_text().contains("outliner.pick: Pick one of the scene's parts"));
    }

    #[test]
    fn import_paths() {
        assert_eq!(pathdiff(Path::new("/a/s/panels/x.tsx"), Path::new("/a/s")), "./panels/x.tsx");
    }
}
