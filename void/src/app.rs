//! The window: a black void, the stage in the middle, the box (spaces) in the top-right corner, and one floating
//! composer at the bottom. Everything else is a glass sheet that appears when asked for.

use std::path::PathBuf;
use std::rc::Rc;
use std::sync::{Arc, Mutex};

use dioxus::prelude::*;
use futures_util::StreamExt;

use crate::brain::{Brain, BrainEvent, Provider};
use crate::intent::{self, Intent};
use crate::spaces::{self, Space};
use crate::stage::{self, Shape};
use crate::stage_ts;
use crate::{frames, mem};
use raea_void::voice::runtime::{Voice, VoiceEvent};
use raea_void::voice::Pocket;

#[derive(Clone, Copy, PartialEq, Debug, serde::Serialize, serde::Deserialize)]
pub enum Who {
    You,
    Void,
    Tool,
    Note,
}

#[derive(Clone, PartialEq, Debug, serde::Serialize, serde::Deserialize)]
pub struct Line {
    pub id: u64,
    pub who: Who,
    pub text: String,
    /// Still streaming (a reply) or still being heard (your words while you talk).
    pub live: bool,
    pub ok: bool,
    /// A step's mark in the terminal (narrate.rs): ✓ ✗ ⚠ ◉ ·
    pub mark: char,
    /// A process's verb (write, listen, look …) and the lines kept under it when done.
    pub verb: String,
    pub detail: Vec<String>,
    /// Voice said between steps (a murmur), not the reply.
    pub murmur: bool,
}

#[derive(Clone, PartialEq, Debug)]
pub enum Action {
    Submit(String, bool),
    OpenSpace(String),
    NewSpace(String),
    DeleteSpace(String),
    Listen(bool),
    SetVoice(String),
    Scrub(i64),
    Play(bool),
    Clear,
    /// Stop the agent's turn and the voice.
    Stop,
    /// ↑ / ↓ in the terminal: what was typed before.
    History(bool),
    /// Dock the terminal: bottom, top, left or right.
    Layout(String),
    /// Bring files into the open space (the Files tab's + or a drop): assets.rs places them, video and audio are
    /// transcribed.
    Import(Vec<PathBuf>),
    /// Remove one of the open space's assets, by its path in the space.
    RemoveAsset(String),
    /// Work in a folder (workspace.rs): open it as it is (false), or make it a new workspace (true).
    OpenWorkspace(PathBuf, bool),
    /// Take a folder off the recent workspaces (the folder stays).
    ForgetWorkspace(PathBuf),
    /// A right-click: the menu for what was clicked, at that point (the founder, 2026-10-17: "everything needs its own
    /// context menu").
    Menu(f64, f64, MenuFor),
    /// The person's answer to the agent's question (`raea ask`): its id and the answer (a label, labels joined by
    /// ", ", or "other: their words"). The turn goes on with it.
    Answer(String, String),
    /// A chat of this project, by id; or a new one.
    OpenSession(String),
    NewSession,
    /// An earlier version of an object on screen (its files and render checked out beside, the object untouched); back
    /// to the object as it is; the object put back as that version (through the agent's shell, so undo takes it back).
    OpenVersion(String, u64),
    BackToObject,
    Restore(String, u64),
}

/// What a right-click was on: each has its own menu (ui.rs ContextMenu).
#[derive(Clone, PartialEq, Debug, Default)]
pub enum MenuFor {
    /// The window itself: the app's settings.
    #[default]
    Window,
    /// The terminal: the conversation and the prompt.
    Terminal,
    /// The Files tab's empty space: the object's files as a whole.
    Files,
    /// One file in the Files tab: its path in the object and on disk.
    File(String, PathBuf),
    /// The stage: the object playing on it.
    Stage,
}

/// The window's state, all signals (Copy), handed to each part.
#[derive(Clone, Copy, PartialEq)]
pub struct S {
    pub lines: Signal<Vec<Line>>,
    pub status: Signal<String>,
    pub space: Signal<Option<Space>>,
    pub spaces: Signal<Vec<Space>>,
    pub shape: Signal<Option<Shape>>,
    pub frame: Signal<i64>,
    pub frame_url: Signal<Option<String>>,
    pub playing: Signal<bool>,
    /// Playing, but the frame due isn't rendered yet.
    pub buffering: Signal<bool>,
    /// Frames of this version in the cache, as ranges (the scrubber shows them).
    pub buffered: Signal<Vec<(i64, i64)>>,
    pub version: Signal<u64>,
    /// The last frame's render time.
    pub render_ms: Signal<f64>,
    /// The app's physical footprint (mem.rs).
    pub mem: Signal<u64>,
    /// The version whose preview video is ready (stage.rs preview); playback plays it.
    pub preview: Signal<Option<u64>>,
    /// A preview is rendering.
    pub previewing: Signal<bool>,
    pub listening: Signal<bool>,
    pub speaking: Signal<bool>,
    pub level: Signal<f32>,
    pub voice_ready: Signal<bool>,
    pub voice: Signal<String>,
    pub thinking: Signal<bool>,
    pub brain_label: Signal<String>,
    // chrome
    pub box_open: Signal<bool>,
    pub voice_open: Signal<bool>,
    pub autohide: Signal<bool>,
    pub reveal: Signal<bool>,
    pub menu: Signal<Option<(f64, f64)>>,
    /// What the open menu is for.
    pub menu_for: Signal<MenuFor>,
    pub draft: Signal<String>,
    /// The terminal: open, its height, being resized.
    pub term_open: Signal<bool>,
    pub term_h: Signal<f64>,
    pub drag: Signal<bool>,
    /// The emulated screen (term.rs), its box in px, how far it's scrolled back, the spinner's clock.
    pub screen: Signal<Vec<Vec<crate::term::Run>>>,
    pub tsize: Signal<(f64, f64)>,
    pub scroll: Signal<i32>,
    pub tick: Signal<u64>,
    /// The mic's last levels (0-1), oldest first: the voice button's waveform.
    pub wave: Signal<Vec<f32>>,
    /// Where the terminal docks: "bottom", "top", "left" or "right"; the stage takes the rest.
    pub layout: Signal<String>,
    /// The terminal's width when docked left or right.
    pub term_w: Signal<f64>,
    /// What the mic is hearing in the turn so far (cleaned), shown dim on the prompt line.
    pub hearing: Signal<String>,
    /// Text is being selected in the terminal: its screen holds still until the selection goes.
    pub selecting: Signal<bool>,
    /// The space's pane shows its Files tab (ui.rs FilesPane) rather than the terminal; the space's files, their
    /// lengths (by absolute path, probed in the background), files being brought in (name, what's happening), and
    /// files dragged over the window.
    pub files_tab: Signal<bool>,
    pub assets: Signal<Vec<crate::assets::Asset>>,
    pub asset_secs: Signal<std::collections::HashMap<String, f64>>,
    pub importing: Signal<Vec<(String, String)>>,
    pub dropping: Signal<bool>,
    /// The folder void works in (workspace.rs). Its root and its spaces' folder are here for any part that reads
    /// files (`ws.root`, `ws.spaces_dir()`); the open space's folder is `space.folder()` (the root for All spaces).
    pub workspace: Signal<Option<crate::workspace::Workspace>>,
    /// The workspace chooser is showing (at launch with nothing to reopen, or Switch workspace…).
    pub chooser: Signal<bool>,
    /// The open space is a TypeScript project (stage_ts.rs): the stage shows its render. `version` is then its
    /// sources' hash, `preview` the version of the render on screen (stale while it differs), `previewing` a preview
    /// being rendered here, and `frame_url` a still of a stale frame.
    pub ts: Signal<bool>,
    /// A stale frame's still is being rendered.
    pub still: Signal<bool>,
    /// The project's objects as its record has them (@raea/data/project.ts objects): slug, kind, state, last version.
    pub objects: Signal<Vec<serde_json::Value>>,
    /// The question the agent asked, waiting for the person's answer (ui.rs QuestionPanel).
    pub question: Signal<Option<serde_json::Value>>,
    /// The chat on screen, and the sessions sheet open.
    pub session: Signal<Option<crate::sessions::Session>>,
    pub sessions_open: Signal<bool>,
    /// An earlier version on the stage instead of the object as it is: (slug, version).
    pub viewing: Signal<Option<(String, u64)>>,
    /// The native stage (stage_native.rs: the RAEA engine presenting to a Metal layer over the stage's box) opened, and
    /// whether it's showing the object on screen now; its last status for the MCP state.
    pub native: Signal<bool>,
    pub native_on: Signal<bool>,
    /// shown and not yet playing: a timeline is baked on Show (an agent session, 2026-10-17: React steps through every frame once,
    /// ~0.6 s for a 20 s edit), so the stage says "Preparing" until its frame moves
    pub native_wait: Signal<bool>,
    /// a timeline on the stage, for the player bar: (frame, length in frames, rate, playing); length 0 for a game
    pub native_play: Signal<(i64, i64, f64, bool)>,
    /// the person paused it (the bar or the menu): a sheet closing doesn't start it again
    pub user_paused: Signal<bool>,
    pub native_status: Signal<String>,
    /// The stage's page in pixels, for the native chrome's composited stage (the image that carries its texture).
    pub native_page: Signal<(u32, u32)>,
    /// The stage holds the keyboard (a click on it), until the terminal or a sheet takes it back: keys then go to the
    /// object on the native stage, in either chrome.
    pub stage_keys: Signal<bool>,
}

/// What an object is, from its space.json (the agent says it: `new_object`, `raea object kind`).
pub fn kind_of(folder: &std::path::Path) -> Option<String> {
    std::fs::read_to_string(folder.join("space.json")).ok().and_then(|t| serde_json::from_str::<serde_json::Value>(&t).ok())
        .and_then(|v| v["kind"].as_str().map(str::to_string))
}

/// Objects played live in the stage rather than shown as their rendered video: games, decks, documents and music (the
/// user, 2026-10-17: "not everything is a video and in a 9x16"; "games and videos and images and documents and music
/// ... all within the same engine": the native stage takes every kind when it lands).
pub fn plays_live(kind: Option<&str>) -> bool {
    matches!(kind, Some("game" | "deck" | "document" | "music"))
}

/// Kinds the native stage plays: every kind (the founder, 2026-10-17: "games and videos and images and documents and music
/// ... all within the same engine"). Video and motion joined once the engine had video textures, its mixer and its
/// loop (an agent session, 2026-10-17); an object with no kind said is a video.
pub fn plays_native(kind: Option<&str>) -> bool {
    matches!(kind, None | Some("video" | "motion" | "game" | "deck" | "document" | "still" | "music" | "scene3d"))
}

/// The native stage kept in step with the window: the object on screen shown in it (or nothing), its box under the
/// cube's row (#native-box) measured and reported, its status kept. Every 250 ms; cheap when nothing changed.
async fn native_watch(mut s: S) {
    let mut shown: Option<(PathBuf, Option<String>)> = None;
    // the shown object's sources as they were when shown: the agent's edits play once they settle (as the webview's
    // stage follows its version), checked every 4th tick
    let mut shown_version = 0u64;
    let mut tick = 0u64;
    let mut boxed = String::new();
    let mut hidden = false;
    // the frame the stage was at when told to show: the wait ends when it moves
    let mut waiting: Option<i64> = None;
    loop {
        tokio::time::sleep(std::time::Duration::from_millis(250)).await;
        if !*s.native.peek() {
            continue;
        }
        let target = s.space.peek().clone().and_then(|sp| {
            let kind = sp.kind.clone().or_else(|| kind_of(&sp.folder()));
            (plays_native(kind.as_deref()) && sp.folder().join("edit.tsx").exists()).then(|| (sp.folder(), kind))
        });
        tick += 1;
        let mut changed = false;
        if target.is_some() && target == shown && tick % 4 == 0 {
            let f = target.as_ref().map(|t| t.0.clone()).unwrap_or_default();
            if let Ok(src) = tokio::task::spawn_blocking(move || crate::stage_ts::sources(&f)).await {
                let settled = src.newest.and_then(|t| t.elapsed().ok()).is_none_or(|e| e.as_secs_f64() > 2.0);
                if src.version != shown_version && settled {
                    changed = shown_version != 0;
                    shown_version = src.version;
                }
            }
        }
        if target != shown || changed {
            if target != shown { shown_version = 0; }
            match &target {
                Some((f, k)) => {
                    waiting = Some(crate::stage_native::status().frame);
                    crate::stage_native::show(Some(f), k.as_deref());
                    crate::mcp::log(format!("native stage: {}", f.display()));
                }
                None => { waiting = None; crate::stage_native::show(None, None) }
            }
            s.native_wait.set(waiting.is_some());
            s.native_on.set(target.is_some());
            shown = target;
            boxed.clear();
            hidden = false;
        }
        // a sheet over the stage (the cube's list, the chats, a question, the menu, the voice panel) draws over it; the
        // object holds still until it closes
        let covered = *s.box_open.peek() || *s.sessions_open.peek() || s.menu.peek().is_some() || *s.voice_open.peek() || s.question.peek().is_some();
        // (not before its first frame: a question waiting as the app opens would leave the stage black)
        if shown.is_some() && waiting.is_none() && covered != hidden {
            // (a closing sheet plays it again only if the person hadn't paused it)
            if covered {
                crate::stage_native::control(crate::stage_native::Control::Pause);
            } else if !*s.user_paused.peek() {
                crate::stage_native::control(crate::stage_native::Control::Play);
            }
            hidden = covered;
        }
        if shown.is_some() {
            if let Some((x, y, w, h)) = crate::focus::rect("native-box").await {
                {
                    let n = [x, y, w, h, crate::win::scale()];
                    let key = format!("{:?}", n);
                    if key != boxed && n[2] > 1.0 && n[3] > 1.0 {
                        crate::stage_native::rect(n[0], n[1], n[2], n[3], n[4]);
                        crate::mcp::log(format!("native stage: rect {:.0},{:.0} {:.0}x{:.0} @{}", n[0], n[1], n[2], n[3], n[4]));
                        boxed = key;
                    }
                }
            }
            let st = crate::stage_native::status();
            if let Some(f0) = waiting {
                let on = st.showing.as_ref() == shown.as_ref().map(|t| &t.0);
                if st.error.is_some() || (on && st.frame != f0) {
                    waiting = None;
                    s.native_wait.set(false);
                }
            }
            if st.page != *s.native_page.peek() && st.page.0 > 0 {
                crate::mcp::log(format!("native stage: page {}x{}", st.page.0, st.page.1));
                s.native_page.set(st.page);
            }
            let line = format!("{} fps, {} missed{}", st.fps.round(), st.missed, st.error.as_ref().map(|e| format!(", error: {e}")).unwrap_or_default());
            if *s.native_status.peek() != line {
                s.native_status.set(line);
            }
        }
    }
}

/// The project's folder: the workspace's root.
pub fn project_root(s: S) -> Option<PathBuf> {
    s.workspace.peek().as_ref().map(|w| w.root.clone())
}

/// The objects' record read again (kinds, draft or ready, last versions), off the UI thread.
pub fn refresh_objects(mut s: S) {
    let folder = s.space.peek().as_ref().filter(|sp| !sp.is_all() && *s.viewing.peek() == None).map(|sp| sp.folder()).or_else(|| project_root(s));
    let Some(folder) = folder else { return };
    spawn(async move {
        let (j, code) = crate::host::project(&folder, &["objects".into()], true).await;
        if code == 0 {
            if let Ok(rows) = serde_json::from_str::<Vec<serde_json::Value>>(&j) {
                s.objects.set(rows);
            }
        }
    });
}

/// The open space's assets read again, and the lengths of video and audio not yet known probed in the background.
pub fn refresh_assets(mut s: S) {
    let Some(folder) = s.space.peek().as_ref().map(|sp| sp.folder()) else {
        s.assets.set(Vec::new());
        return;
    };
    let list = crate::assets::list(&folder);
    let need: Vec<PathBuf> = {
        let known = s.asset_secs.peek();
        list.iter()
            .filter(|a| a.kind.heard() && !a.broken)
            .map(|a| folder.join(&a.rel))
            .filter(|p| !known.contains_key(&p.to_string_lossy().into_owned()))
            .collect()
    };
    // the native chrome reads thumbnails from disk: the missing ones drawn in the background, then the list shown again
    {
        let paths: Vec<PathBuf> = list.iter().map(|a| folder.join(&a.rel)).filter(|p| crate::assets::cached_thumb(p).is_none()).collect();
        if !paths.is_empty() {
            let again = list.clone();
            spawn(async move {
                let mut s = s;
                if tokio::task::spawn_blocking(move || crate::assets::make_thumbs(&paths)).await.unwrap_or(false) {
                    s.assets.set(again);
                }
            });
        }
    }
    s.assets.set(list);
    // the search index caught up in the background (new or changed files only; assets.rs reindex)
    spawn(async move {
        let f = folder.clone();
        let _ = tokio::task::spawn_blocking(move || crate::assets::reindex(&f)).await;
    });
    if !need.is_empty() {
        spawn(async move {
            for p in need {
                let key = p.to_string_lossy().into_owned();
                if let Ok(Some(t)) = tokio::task::spawn_blocking(move || crate::assets::seconds(&p)).await {
                    s.asset_secs.write().insert(key, t);
                }
            }
        });
    }
}

/// One transcription at a time, however many imports are running: Parakeet is loaded per file.
static LISTENING: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

/// Files brought into the space at `folder`: each placed (copied or linked, assets.rs), shown at once, then video and
/// audio transcribed as `import_media` does (brain.rs `listen`). Each lands as a terminal note and a notice the agent
/// reads at the start of its next turn.
async fn import(mut s: S, folder: PathBuf, paths: Vec<PathBuf>) {
    let files = crate::assets::expand(&paths);
    if files.is_empty() {
        note(s, "Nothing to bring in: drop files, or a folder of files");
        return;
    }
    let name_of = |p: &PathBuf| p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    s.importing.write().extend(files.iter().map(|f| (name_of(f), "copying".to_string())));
    let mut heard = Vec::new();
    for f in files {
        let name = name_of(&f);
        let placed = tokio::task::spawn_blocking({
            let (folder, f) = (folder.clone(), f.clone());
            move || crate::assets::place(&folder, &f)
        }).await.map_err(anyhow::Error::from).and_then(|r| r);
        match placed {
            Ok(p) if p.kind.heard() && !p.already => {
                if let Some(e) = s.importing.write().iter_mut().find(|e| e.0 == name) { e.1 = "listening".into(); }
                heard.push((f, p));
            }
            Ok(p) => {
                s.importing.write().retain(|e| e.0 != name);
                if p.already {
                    note(s, format!("{name} is already in the space ({})", p.rel));
                } else {
                    note(s, format!("Added {}{}", p.rel, if p.linked { " (linked)" } else { "" }));
                    crate::brain::notice(&folder, format!("added {} ({}{})", p.rel, p.kind.label().to_lowercase(), if p.linked { ", a link to the original" } else { "" }));
                }
            }
            Err(e) => {
                s.importing.write().retain(|e| e.0 != name);
                note(s, format!("Couldn't bring in {name}: {e}"));
            }
        }
        if s.space.peek().as_ref().is_some_and(|sp| sp.folder() == folder) { refresh_assets(s); }
    }
    for (src, p) in heard {
        let name = name_of(&src);
        let said = {
            let _one = LISTENING.lock().await;
            crate::brain::listen(&src, &p.dest).await
        };
        let link = if p.linked { ", a link to the original" } else { "" };
        // the file's own length (words made earlier only reach the last word)
        let probed = tokio::task::spawn_blocking({ let d = p.dest.clone(); move || crate::assets::seconds(&d) }).await.ok().flatten();
        match said {
            Ok((words, secs, wp)) => {
                let secs = probed.unwrap_or(secs);
                let words_rel = wp.strip_prefix(&folder).unwrap_or(&wp).display().to_string();
                note(s, if words.is_empty() { format!("Added {} · {secs:.1} s, no speech heard", p.rel) } else { format!("Added {} · {secs:.1} s, {} words", p.rel, words.len()) });
                crate::brain::notice(&folder, format!("added {} ({}, {secs:.1} s{link}); its speech, {} words: {words_rel}", p.rel, p.kind.label().to_lowercase(), words.len()));
            }
            Err(e) => {
                note(s, format!("Added {} · no speech heard", p.rel));
                crate::brain::notice(&folder, format!("added {} ({}{link}); not transcribed: {}", p.rel, p.kind.label().to_lowercase(), e.to_string().lines().next().unwrap_or("")));
            }
        }
        s.importing.write().retain(|e| e.0 != name);
        if s.space.peek().as_ref().is_some_and(|sp| sp.folder() == folder) { refresh_assets(s); }
    }
}

fn settings_path() -> PathBuf {
    spaces::home().join("settings.json")
}

fn load_settings() -> serde_json::Value {
    std::fs::read_to_string(settings_path()).ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default()
}

pub fn save_setting(key: &str, value: serde_json::Value) {
    let mut s = load_settings();
    if !s.is_object() {
        s = serde_json::json!({});
    }
    s[key] = value;
    let _ = std::fs::create_dir_all(spaces::home());
    let _ = std::fs::write(settings_path(), serde_json::to_string_pretty(&s).unwrap_or_default());
}

/// A cached frame as the void:// URL the webview loads it by (main.rs serves it from frames.rs's memory).
fn frame_url(version: u64, f: i64) -> String {
    format!("void://localhost/frame?k={version:x}-{f}")
}

/// The still the TypeScript stage wants (an edit.tsx, its version, a frame), one rendered at a time (ts_stills).
type Stills = Rc<(tokio::sync::watch::Sender<Option<(PathBuf, u64, i64)>>, tokio::sync::watch::Receiver<Option<(PathBuf, u64, i64)>>)>;

/// The TypeScript stage's picture at the playhead: when the render on screen is the project as it is, the video seeks
/// there (an effect in App does it); when it's stale, a still of this version's frame, from the frame cache or rendered.
fn ts_request(mut s: S, stills: &Stills) {
    let v = *s.version.peek();
    let Some(sp) = s.space.peek().clone().filter(|sp| sp.is_ts()) else { return };
    if v == 0 || *s.preview.peek() == Some(v) {
        return;
    }
    let f = *s.frame.peek();
    if frames::has((v, f)) {
        s.frame_url.set(Some(frame_url(v, f)));
        return;
    }
    if stage_ts::blank(&sp.folder()) {
        return;
    }
    stills.0.send_replace(Some((sp.folder(), v, f)));
}

/// Stills of a stale project, one at a time: the newest frame asked for, skipped once the render caught up or the
/// project moved on. A version whose still fails (the composition throws) isn't tried again.
async fn ts_stills(mut s: S, stills: Stills) {
    let mut rx = stills.0.subscribe();
    let mut bad = 0u64;
    loop {
        if rx.changed().await.is_err() {
            return;
        }
        let Some((folder, v, f)) = rx.borrow_and_update().clone() else { continue };
        if *s.version.peek() != v || *s.preview.peek() == Some(v) || bad == v {
            continue;
        }
        if !frames::has((v, f)) {
            s.still.set(true);
            let t = std::time::Instant::now();
            let r = stage_ts::still(&folder, v, f).await;
            s.still.set(false);
            match r {
                Ok(png) => {
                    frames::put((v, f), png);
                    s.render_ms.set(t.elapsed().as_secs_f64() * 1e3);
                }
                Err(e) => {
                    bad = v;
                    crate::mcp::log(format!("stage: {e}"));
                    s.status.set(format!("stage: {e}"));
                    continue;
                }
            }
        }
        // still wanted: the same version and frame, and no render of it landed meanwhile
        if *s.version.peek() == v && *s.frame.peek() == f && *s.preview.peek() != Some(v) {
            s.frame_url.set(Some(frame_url(v, f)));
        }
    }
}

/// A render on the TypeScript stage: preview `key` (stage.rs preview_path), of this shape.
fn ts_show(mut s: S, key: u64, sh: Shape) {
    let first = s.shape.peek().is_none();
    s.shape.set(Some(sh));
    s.preview.set(Some(key));
    let f = *s.frame.peek();
    if first {
        s.frame.set(sh.frames / 3);
    } else if f >= sh.frames {
        s.frame.set(sh.frames - 1);
    }
    if key == *s.version.peek() {
        // the whole thing is there to play
        s.buffered.set(vec![(0, sh.frames - 1)]);
        s.frame_url.set(None);
    } else {
        s.buffered.set(Vec::new());
    }
}

/// Keeps the TypeScript stage current. Every 0.7 s (or when nudged: a space opened, the agent changed something) it
/// hashes the project's sources. A new version goes stale on screen at once (stills for scrubbing), then the newest
/// render that matches goes up: one kept from before, else the agent's final.mp4 if it's newer than every source,
/// else a preview rendered here, once the project has held still a second and the agent isn't mid-turn (a turn
/// usually ends with its own `raea render`, taken as it lands). With nothing on screen yet, an older final.mp4 shows
/// meanwhile. Nothing here blocks the window: hashing runs on a blocking thread, renders are child processes.
async fn ts_watch(mut s: S, stills: Stills, kick: Rc<tokio::sync::Notify>) {
    let failed = Rc::new(std::cell::Cell::new(0u64));
    let mut seen = (0u64, std::time::Instant::now());
    loop {
        let _ = tokio::time::timeout(std::time::Duration::from_millis(700), kick.notified()).await;
        let Some(sp) = s.space.peek().clone().filter(|sp| sp.is_ts()) else { continue };
        // the native stage plays it, or a game or a deck (they play live): no preview video or stills rendered beside it
        if *s.native_on.peek() || plays_live(sp.kind.clone().or_else(|| kind_of(&sp.folder())).as_deref()) {
            continue;
        }
        if !*s.ts.peek() {
            // a space with no project just got its edit.tsx (`raea init` on a turn): the stage switches to its render
            crate::mcp::log(format!("stage: {} is a TypeScript project now; showing its render", sp.name));
            s.ts.set(true);
            s.playing.set(false);
            s.version.set(0);
            s.preview.set(None);
            s.frame_url.set(None);
            s.shape.set(None);
            s.buffered.set(Vec::new());
        }
        let folder = sp.folder();
        let Ok(src) = tokio::task::spawn_blocking({
            let f = folder.clone();
            move || stage_ts::sources(&f)
        })
        .await
        else {
            continue;
        };
        // another space opened while hashing
        if s.space.peek().as_ref().map(|x| x.folder()) != Some(folder.clone()) {
            continue;
        }
        let v = src.version;
        if v != *s.version.peek() {
            frames::retain_version(Some(v));
            s.version.set(v);
            s.frame_url.set(None);
            seen = (v, std::time::Instant::now());
            if *s.preview.peek() != Some(v) {
                s.buffered.set(Vec::new());
                crate::mcp::log(format!("stage: {} is at {v:x}", sp.name));
            }
            // a still meanwhile, unless this version's render is already kept (it goes up just below)
            if !stage::preview_path(v).exists() {
                ts_request(s, &stills);
            }
        }
        let pv = *s.preview.peek();
        if pv == Some(v) {
            continue;
        }
        // a render of this version already kept (an earlier preview, or the project back to an earlier state)
        let kept = stage::preview_path(v);
        if kept.exists() {
            if let Some(sh) = stage_ts::probe(&kept).await {
                ts_show(s, v, sh);
                continue;
            }
        }
        // the agent's render (raea render or raea check), when it's from these sources
        if stage_ts::final_newer(&folder, &src) {
            let fin = stage_ts::final_mp4(&folder);
            if let Some(sh) = stage_ts::probe(&fin).await {
                match tokio::task::spawn_blocking(move || stage_ts::keep(&fin, v)).await {
                    Ok(Ok(_)) => {
                        crate::mcp::log(format!("preview: final.mp4 for {v:x}, {} frames", sh.frames));
                        ts_show(s, v, sh);
                        continue;
                    }
                    Ok(Err(e)) => crate::mcp::log(format!("preview: keeping final.mp4: {e}")),
                    Err(_) => {}
                }
            }
        }
        // nothing on screen yet: the last final.mp4, stale, while this version renders
        if pv.is_none() && stage_ts::final_settled(&folder) {
            let fin = stage_ts::final_mp4(&folder);
            let k = frames::version(&fin) | 1;
            if let Some(sh) = stage_ts::probe(&fin).await {
                if let Ok(Ok(_)) = tokio::task::spawn_blocking(move || stage_ts::keep(&fin, k)).await {
                    ts_show(s, k, sh);
                }
            }
        }
        if stage_ts::blank(&folder) {
            continue;
        }
        let settled = seen.0 == v && seen.1.elapsed().as_secs_f64() >= 1.0;
        if !settled || *s.thinking.peek() || *s.previewing.peek() || failed.get() == v {
            continue;
        }
        s.previewing.set(true);
        let (failed, kick) = (failed.clone(), kick.clone());
        spawn(async move {
            let mut s = s;
            let t = std::time::Instant::now();
            let r = stage_ts::render(&folder, v).await;
            s.previewing.set(false);
            match r {
                Ok(p) => match stage_ts::probe(&p).await {
                    Some(sh) => {
                        crate::mcp::log(format!("preview: {v:x} in {:.1} s", t.elapsed().as_secs_f64()));
                        if s.space.peek().as_ref().map(|x| x.folder()) == Some(folder) {
                            ts_show(s, v, sh);
                        }
                    }
                    None => crate::mcp::log(format!("preview: {} unreadable", p.display())),
                },
                Err(e) => {
                    failed.set(v);
                    crate::mcp::log(format!("preview: {e}"));
                    s.status.set(format!("preview: {e}"));
                }
            }
            kick.notify_one();
        });
    }
}

static NEXT_ID: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
fn next_id() -> u64 {
    NEXT_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
}

fn push(mut lines: Signal<Vec<Line>>, who: Who, text: impl Into<String>, live: bool) -> u64 {
    let id = next_id();
    lines.write().push(Line { id, who, text: text.into(), live, ok: true, mark: ' ', verb: String::new(), detail: Vec::new(), murmur: false });
    let n = lines.read().len();
    if n > 200 {
        lines.write().drain(..n - 200);
    }
    id
}

fn note(s: S, text: impl Into<String>) {
    let text = text.into();
    crate::mcp::log(format!("note: {text}"));
    push(s.lines, Who::Note, text, false);
}

/// The window's state as JSON (the MCP `state` tool).
fn state(s: S) -> serde_json::Value {
    let lines: Vec<_> = s.lines.peek().iter().rev().take(30).rev().map(|l| serde_json::json!({
        "who": format!("{:?}", l.who).to_lowercase(), "text": l.text, "live": l.live, "ok": l.ok,
    })).collect();
    let shape = *s.shape.peek();
    let (n, bytes, budget, hits, misses) = frames::stats();
    let cache = serde_json::json!({"frames": n, "mb": bytes >> 20, "budget_mb": budget >> 20, "hits": hits, "misses": misses});
    // the terminal as drawn (term.rs's rows, as text): what the person sees in it, spinners and bars at this frame
    let terminal: Vec<String> = s.screen.peek().iter().map(|r| r.iter().map(|x| x.text.as_str()).collect::<String>()).collect();
    serde_json::json!({
        "workspace": s.workspace.peek().as_ref().map(|w| serde_json::json!({"name": w.name(), "root": w.root, "spaces": w.spaces_dir()})),
        "chooser": *s.chooser.peek(),
        "space": s.space.peek().as_ref().map(|sp| serde_json::json!({"name": sp.name, "slug": sp.slug, "program": sp.has_program(),
            "edit": sp.tsx(), "kind": if sp.is_ts() { "ts" } else { "none" }})),
        "spaces": s.spaces.peek().iter().map(|sp| sp.name.clone()).collect::<Vec<_>>(),
        "stage": shape.map(|sh| {
            let mut st = serde_json::json!({"fps": sh.fps, "width": sh.width, "height": sh.height, "frames": sh.frames,
                "frame": *s.frame.peek(), "playing": *s.playing.peek(), "buffering": *s.buffering.peek(), "drawn": s.frame_url.peek().is_some(),
                "render_ms": (*s.render_ms.peek() * 10.0).round() / 10.0, "buffered": *s.buffered.peek(),
                "preview": *s.preview.peek() == Some(*s.version.peek()), "previewing": *s.previewing.peek()});
            if *s.ts.peek() {
                // the TypeScript stage: which render is on screen, and whether it's the project as it is now
                let (v, pv) = (*s.version.peek(), *s.preview.peek());
                st["kind"] = "ts".into();
                st["version"] = format!("{v:x}").into();
                st["showing"] = pv.map(|p| format!("{p:x}")).into();
                st["fresh"] = (pv == Some(v) && v != 0).into();
                st["still"] = s.frame_url.peek().clone().into();
                st["rendering_still"] = (*s.still.peek()).into();
            }
            st
        }),
        "ts": *s.ts.peek(),
        "cache": cache,
        "memory_mb": *s.mem.peek() >> 20,
        "voice": {"listening": *s.listening.peek(), "speaking": *s.speaking.peek(), "voice": *s.voice.peek(), "ready": *s.voice_ready.peek(), "level": *s.level.peek()},
        "brain": {"thinking": *s.thinking.peek(), "model": *s.brain_label.peek()},
        "status": *s.status.peek(),
        "chrome": {"box_open": *s.box_open.peek(), "voice_open": *s.voice_open.peek(), "autohide": *s.autohide.peek(),
            "composer_shown": !*s.autohide.peek() || *s.reveal.peek(), "menu": s.menu.peek().is_some(), "draft": *s.draft.peek(),
            "files_tab": *s.files_tab.peek(), "importing": *s.importing.peek()},
        "assets": s.assets.peek().iter().map(|a| serde_json::json!({"rel": a.rel, "kind": a.kind.label(), "bytes": a.bytes, "linked": a.linked, "words": a.words})).collect::<Vec<_>>(),
        "transcript": lines,
        "terminal": terminal,
        "objects": *s.objects.peek(),
        "question": *s.question.peek(),
        "session": s.session.peek().as_ref().map(|x| serde_json::json!({"id": x.id, "name": x.name})),
        "sessions": project_root(s).map(|r| crate::sessions::list(&r).iter().map(|x| serde_json::json!({"id": x.id, "name": x.name})).collect::<Vec<_>>()),
        "viewing": s.viewing.peek().as_ref().map(|(o, n)| serde_json::json!({"object": o, "version": n})),
        "native": {"open": *s.native.peek(), "showing": *s.native_on.peek(), "status": *s.native_status.peek()},
        "window_visible": crate::win::visible(),
    })
}

/// One MCP call that needs the window.
async fn answer(s: S, actions: Coroutine<Action>, call: crate::mcp::Call) -> Result<serde_json::Value, String> {
    use crate::mcp::Call;
    let mut s = s;
    match call {
        Call::State => Ok(state(s)),
        Call::Send { text, spoken } => {
            actions.send(Action::Submit(text, spoken));
            Ok(serde_json::json!("sent"))
        }
        Call::Act { action, value } => {
            let v = value.as_str().map(str::to_string).unwrap_or_else(|| value.to_string());
            match action.as_str() {
                "open_space" => {
                    let slug = spaces::find(&v).map(|sp| sp.slug).ok_or(format!("no space {v}"))?;
                    actions.send(Action::OpenSpace(slug));
                }
                "new_space" | "new_object" => actions.send(Action::NewSpace(v)),
                "open_object" => {
                    let slug = spaces::find(&v).map(|sp| sp.slug).ok_or(format!("no object {v}"))?;
                    actions.send(Action::OpenSpace(slug));
                }
                "objects" => { s.spaces.set(spaces::list()); refresh_objects(s); s.box_open.set(true); }
                "answer" => {
                    let q = s.question.peek().clone().ok_or("no question is waiting")?;
                    actions.send(Action::Answer(q["id"].as_str().unwrap_or("").to_string(), v));
                }
                "new_session" => actions.send(Action::NewSession),
                "open_session" => actions.send(Action::OpenSession(v)),
                "sessions" => s.sessions_open.set(true),
                "open_version" | "restore" => {
                    let (o, n) = v.rsplit_once(['@', ' ']).ok_or("value: OBJECT@N")?;
                    let n: u64 = n.trim_start_matches('v').parse().map_err(|_| "value: OBJECT@N")?;
                    actions.send(if action == "restore" { Action::Restore(o.into(), n) } else { Action::OpenVersion(o.into(), n) });
                }
                "back" => actions.send(Action::BackToObject),
                "delete_space" => actions.send(Action::DeleteSpace(v)),
                "scrub" => actions.send(Action::Scrub(value.as_i64().or_else(|| v.parse().ok()).ok_or("scrub needs a frame")?)),
                "play" => actions.send(Action::Play(true)),
                "pause" => actions.send(Action::Play(false)),
                "listen" => actions.send(Action::Listen(true)),
                "mute" => actions.send(Action::Listen(false)),
                "voice" => actions.send(Action::SetVoice(v)),
                "clear" => actions.send(Action::Clear),
                "spaces" => { s.spaces.set(spaces::list()); s.box_open.set(true); }
                "open_workspace" => actions.send(Action::OpenWorkspace(PathBuf::from(&v), false)),
                "new_workspace" => actions.send(Action::OpenWorkspace(PathBuf::from(&v), true)),
                "chooser" => s.chooser.set(value.as_bool().unwrap_or(true)),
                "voice_sheet" => s.voice_open.set(true),
                "terminal" => { let now = *s.term_open.peek(); s.term_open.set(value.as_bool().unwrap_or(!now)); }
                "close" => { s.box_open.set(false); s.voice_open.set(false); s.menu.set(None); }
                "assets" | "files" => { refresh_assets(s); s.files_tab.set(true); s.term_open.set(true); }
                "tab" => {
                    let files = v == "files" || v == "assets";
                    if files { refresh_assets(s); }
                    s.files_tab.set(files);
                    s.term_open.set(true);
                }
                "import" => {
                    let paths: Vec<PathBuf> = match &value {
                        serde_json::Value::Array(a) => a.iter().filter_map(|p| p.as_str().map(PathBuf::from)).collect(),
                        _ => vec![PathBuf::from(&v)],
                    };
                    actions.send(Action::Import(paths));
                }
                "remove_asset" => actions.send(Action::RemoveAsset(v)),
                "dock" => {
                    let on = value.as_bool().unwrap_or(!*s.autohide.peek());
                    s.autohide.set(on);
                }
                // a click as a person makes it, at window points {"x", "y"} (the native chrome: posted to void's own event
                // queue, so the pointer and the active app stay the person's)
                // files dropped as Finder drops them, through the window's drag handlers ({"paths": [...], "x", "y"})
                            "drop" => {
                    let paths: Vec<String> = value["paths"].as_array().ok_or("value: {paths, x, y}")?.iter().filter_map(|p| p.as_str().map(str::to_string)).collect();
                    let (x, y) = (value["x"].as_f64().unwrap_or(400.0), value["y"].as_f64().unwrap_or(300.0));
                    crate::win::drop_step(&paths, x, y, 0)?;
                    tokio::time::sleep(std::time::Duration::from_millis(400)).await;
                    let op = crate::win::drop_step(&paths, x, y, 1)?;
                    crate::mcp::log(format!("drop: the window takes it as operation {op}"));
                    if op == 0 {
                        return Err("the window refused the drop (no valid drag actions)".into());
                    }
                    crate::win::drop_step(&paths, x, y, 2)?;
                    tokio::time::sleep(std::time::Duration::from_millis(1500)).await;
                    crate::win::drop_step(&paths, x, y, 3)?;
                }
                            "click" | "right_click" => {
                    let (x, y) = (value["x"].as_f64().ok_or("value: {x, y}")?, value["y"].as_f64().ok_or("value: {x, y}")?);
                    crate::win::post_click(x, y, action == "right_click")?;
                }
                other => return Err(format!("no action {other}")),
            }
            Ok(serde_json::json!("ok"))
        }
        Call::Wait(timeout) => {
            let t0 = std::time::Instant::now();
            // let a just-sent turn start before waiting for it to end
            tokio::time::sleep(std::time::Duration::from_millis(300)).await;
            while (*s.thinking.peek() || *s.speaking.peek()) && t0.elapsed().as_secs_f64() < timeout {
                tokio::time::sleep(std::time::Duration::from_millis(100)).await;
            }
            let mut st = state(s);
            st["waited"] = serde_json::json!((t0.elapsed().as_secs_f64() * 10.0).round() / 10.0);
            st["timed_out"] = serde_json::json!(*s.thinking.peek() || *s.speaking.peek());
            Ok(st)
        }
    }
}

/// When the person last sent something in this run: their line ends the transcript between sending and the model's
/// first word, and the live row shows under it then. A line of theirs left last by an earlier run (a turn cut off when
/// void closed) shows none.
static SENT: std::sync::Mutex<Option<std::time::Instant>> = std::sync::Mutex::new(None);
fn just_sent(lines: &[Line]) -> bool {
    lines.last().is_some_and(|l| matches!(l.who, Who::You))
        && SENT.lock().unwrap().is_some_and(|t| t.elapsed().as_secs() < 30)
}

#[component]
pub fn App() -> Element {
    // the native chrome's window, for win.rs (size, scale, drag, the NSWindow for the native stage)
    crate::win::keep(dioxus_native::use_window());
    let settings = load_settings();
    let s = S {
        lines: use_signal(Vec::new),
        status: use_signal(String::new),
        space: use_signal(|| None),
        spaces: use_signal(spaces::list),
        shape: use_signal(|| None),
        frame: use_signal(|| 0),
        frame_url: use_signal(|| None),
        playing: use_signal(|| false),
        buffering: use_signal(|| false),
        buffered: use_signal(Vec::new),
        version: use_signal(|| 0),
        render_ms: use_signal(|| 0.0),
        mem: use_signal(|| 0),
        preview: use_signal(|| None),
        previewing: use_signal(|| false),
        listening: use_signal(|| false),
        speaking: use_signal(|| false),
        level: use_signal(|| 0.0),
        voice_ready: use_signal(|| false),
        // the voice the user picked for void (~/.raea-void/voices/watts.wav, a cloning prompt, 2026-10-15), else Pocket's alba
        voice: use_signal(|| {
            settings["voice"].as_str().map(str::to_string).unwrap_or_else(Pocket::default_voice)
        }),
        thinking: use_signal(|| false),
        brain_label: use_signal(String::new),
        box_open: use_signal(|| false),
        voice_open: use_signal(|| false),
        autohide: use_signal(|| settings["autohide"].as_bool().unwrap_or(false)),
        reveal: use_signal(|| false),
        menu: use_signal(|| None),
        menu_for: use_signal(MenuFor::default),
        draft: use_signal(String::new),
        term_open: use_signal(|| settings["terminal"].as_bool().unwrap_or(true)),
        term_h: use_signal(|| settings["terminal_h"].as_f64().unwrap_or(240.0)),
        drag: use_signal(|| false),
        screen: use_signal(Vec::new),
        tsize: use_signal(|| (900.0, 220.0)),
        scroll: use_signal(|| 0),
        tick: use_signal(|| 0),
        wave: use_signal(|| vec![0.0; 64]),
        layout: use_signal(|| settings["layout"].as_str().unwrap_or("bottom").to_string()),
        term_w: use_signal(|| settings["terminal_w"].as_f64().unwrap_or(440.0)),
        hearing: use_signal(String::new),
        selecting: use_signal(|| false),
        files_tab: use_signal(|| settings["tab"].as_str() == Some("files")),
        assets: use_signal(Vec::new),
        asset_secs: use_signal(std::collections::HashMap::new),
        importing: use_signal(Vec::new),
        dropping: use_signal(|| false),
        workspace: use_signal(|| None),
        chooser: use_signal(|| false),
        ts: use_signal(|| false),
        still: use_signal(|| false),
        objects: use_signal(Vec::new),
        question: use_signal(|| None),
        session: use_signal(|| None),
        sessions_open: use_signal(|| false),
        viewing: use_signal(|| None),
        native: use_signal(|| false),
        native_on: use_signal(|| false),
        native_status: use_signal(String::new),
        native_wait: use_signal(|| false),
        native_play: use_signal(|| (0, 0, 30.0, false)),
        user_paused: use_signal(|| false),
        native_page: use_signal(|| (0, 0)),
        stage_keys: use_signal(|| false),
    };
    use_future(move || async move { native_watch(s).await });
    // the player bar's playhead, ten times a second while something is on the stage (only the bar reads it)
    use_future(move || async move {
        let mut s = s;
        loop {
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
            if !*s.native_on.peek() {
                continue;
            }
            let st = crate::stage_native::status();
            let now = (st.frame, st.frames, st.rate, st.playing);
            if *s.native_play.peek() != now {
                s.native_play.set(now);
            }
        }
    });
    // the native chrome sends no onresize: the terminal's box is measured, as the stage's is
    use_future(move || async move {
        let mut s = s;
        loop {
            tokio::time::sleep(std::time::Duration::from_millis(300)).await;
            crate::focus::measure_cell().await;
            // the content box, as the webview's onresize gave it: the body's box less its 12 px padding each side (and 6 below)
            if let Some((_, _, w, h)) = crate::focus::rect("tbody").await {
                let (w, h) = (w - 24.0, h - 6.0);
                if (w, h) != *s.tsize.peek() && w > 1.0 { s.tsize.set((w, h)); }
            }
        }
    });

    // the terminal (term.rs): the conversation projected through Alacritty's emulator, the prompt on its last line
    let shell = use_hook(|| Rc::new(std::cell::RefCell::new(crate::term::Shell::new(100, 12))));
    // what changed is only noted here; the screen is rebuilt at most ~30 times a second below, so a streaming reply
    // (dozens of deltas a second) or fast typing can't queue a rebuild per keystroke and stall the window
    let dirty = use_hook(|| Rc::new(std::cell::Cell::new(true)));
    use_effect({
        let dirty = dirty.clone();
        move || {
            let _ = (s.lines.read().len(), s.lines.read().last().map(|l| l.text.len()), (s.draft)(), (s.tick)(), (s.tsize)(),
                (s.scroll)(), (s.hearing)(), (s.listening)(), (s.selecting)(), (s.thinking)());
            dirty.set(true);
        }
    });
    use_future({
        let (shell, dirty) = (shell.clone(), dirty.clone());
        move || {
            let (shell, dirty) = (shell.clone(), dirty.clone());
            async move {
                let mut s = s;
                loop {
                    tokio::time::sleep(std::time::Duration::from_millis(33)).await;
                    // holding still while text is selected (a redraw would drop the selection)
                    if !dirty.get() || *s.selecting.peek() {
                        continue;
                    }
                    dirty.set(false);
                    let draft = s.draft.peek().clone();
                    let hearing = s.hearing.peek().clone();
                    let (w, h) = *s.tsize.peek();
                    let scroll = *s.scroll.peek();
                    let prompt = if *s.listening.peek() { "◉" } else { "❯" };
                    let (rows, offset) = {
                        let lines = s.lines.peek();
                        let mut sh = shell.borrow_mut();
                        // the terminal font's cell, as measured (SF Mono at 12.5 px in the webview: 7.53 a cell, 18 a line)
                        let (cw, ch) = crate::focus::cell();
                        sh.resize(((w - 4.0) / cw).floor().max(20.0) as usize, ((h - 10.0) / ch.max(18.0)).floor().max(3.0) as usize);
                        sh.dim_input = draft.is_empty() && !hearing.is_empty();
                        sh.input = if sh.dim_input { hearing } else { draft };
                        sh.set_frame(*s.tick.peek() as usize);
                        // the agent at work: from the moment the person's message lands (before the model is even
                        // reached) until the turn ends, a live row under the last line
                        sh.set_thinking(*s.thinking.peek() || just_sent(&lines));
                        sh.set_scroll(scroll);
                        sh.load(lines.iter().map(|l| match l.who {
                            Who::You => crate::term::Entry::You(&l.text),
                            Who::Note => crate::term::Entry::Note(&l.text),
                            Who::Tool => crate::term::Entry::Proc { verb: &l.verb, text: &l.text, live: l.live, mark: l.mark, detail: &l.detail },
                            Who::Void => crate::term::Entry::Voice { id: l.id, text: &l.text, live: l.live, murmur: l.murmur },
                        }));
                        let rows = sh.render(prompt);
                        (rows, sh.offset())
                    };
                    // scrolled past the top of the history: hold at the top, so scrolling back down answers at once
                    if offset != scroll {
                        s.scroll.set(offset);
                    }
                    // only a changed screen reaches the page
                    if *s.screen.peek() != rows {
                        s.screen.set(rows);
                    }
                }
            }
        }
    });
    // the session's transcript, saved when a turn settles (not on every streamed word), so a crash or a relaunch
    // loses nothing (sessions.rs; the model's history is saved with it at each turn's end)
    use_effect(move || {
        let lines = s.lines.read();
        if lines.iter().any(|l| l.live) {
            return;
        }
        let (Some(sess), Some(root)) = (s.session.peek().clone(), project_root(s)) else { return };
        let keep: Vec<Line> = lines.iter().rev().take(400).rev().cloned().collect();
        drop(lines);
        std::thread::spawn(move || {
            let _ = crate::sessions::save_lines(&root, &sess, &keep);
        });
    });

    // the spinner's clock, while anything runs
    use_future(move || async move {
        let mut s = s;
        loop {
            tokio::time::sleep(std::time::Duration::from_millis(90)).await;
            // only while something on screen animates: a running step, a reply being typed out, the agent thinking (from
            // the person's message on)
            let running = *s.thinking.peek()
                || just_sent(&s.lines.peek())
                || s.lines.peek().iter().any(|l| l.live && matches!(l.who, Who::Tool | Who::Void));
            if running {
                let n = *s.tick.peek();
                s.tick.set(n + 1);
            }
        }
    });

    let open_space = use_hook(|| Arc::new(Mutex::new(None::<Space>)));
    let brain = use_hook(|| Rc::new(tokio::sync::Mutex::new(None::<Brain>)));
    // a session's model history, handed to the brain made for its next turn
    let resume = use_hook(|| Rc::new(std::cell::RefCell::new(None::<Vec<serde_json::Value>>)));
    let turn_task = use_hook(|| Rc::new(std::cell::RefCell::new(None::<dioxus::core::Task>)));
    let (vtx, vrx) = use_hook(|| {
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel::<VoiceEvent>();
        (tx, Rc::new(std::cell::RefCell::new(Some(rx))))
    });
    let voice = use_hook({
        let vtx = vtx.clone();
        let v = s.voice.peek().clone();
        move || Voice::spawn(vtx, &v)
    });

    // the TypeScript stage (stage_ts.rs): the still wanted (a stale frame, scrubbed to), one rendered at a time, the
    // newest asked for first; and a nudge for the watcher below when something may have changed
    let stills = use_hook(|| Rc::new(tokio::sync::watch::channel(None::<(PathBuf, u64, i64)>)));
    let kick = use_hook(|| Rc::new(tokio::sync::Notify::new()));
    use_future({
        let stills = stills.clone();
        move || {
            let stills = stills.clone();
            async move { ts_stills(s, stills).await }
        }
    });
    use_future({
        let (stills, kick) = (stills.clone(), kick.clone());
        move || {
            let (stills, kick) = (stills.clone(), kick.clone());
            async move { ts_watch(s, stills, kick).await }
        }
    });

    // what to draw now: the open space's TypeScript render at the current frame (stage_ts.rs); a space with no
    // edit.tsx yet (an old rrl space before its first turn's `raea init`) shows an empty stage
    let request = {
        let stills = stills.clone();
        move |mut s: S| {
            if s.space.peek().as_ref().is_some_and(|sp| sp.is_ts()) {
                return ts_request(s, &stills);
            }
            s.frame_url.set(None);
        }
    };

    // an object onto the stage, the agent's too (its shell works there); `view` puts one on the stage alone (an earlier
    // version checked out beside it). The chat stays: it's the session's, not the object's.
    let view = {
        let request = request.clone();
        let kick = kick.clone();
        move |mut s: S, space: Space| {
            s.space.set(Some(space.clone()));
            s.spaces.set(spaces::list());
            if *s.files_tab.peek() { refresh_assets(s); }
            s.playing.set(false);
            s.shape.set(None);
            if space.is_ts() || *s.ts.peek() {
                // the TypeScript stage starts over: the watcher reads the project and puts its render up
                s.ts.set(space.is_ts());
                s.version.set(0);
                s.preview.set(None);
                s.frame_url.set(None);
                s.buffered.set(Vec::new());
                s.frame.set(0);
            }
            request(s);
            if space.is_ts() {
                kick.notify_one();
            }
        }
    };
    let show = {
        let open_space = open_space.clone();
        let view = view.clone();
        move |mut s: S, space: Space| {
            *open_space.lock().unwrap() = Some(space.clone());
            s.viewing.set(None);
            view(s, space.clone());
            if let Some(root) = project_root(s) {
                let slug = space.slug.clone();
                spawn(async move { let _ = crate::host::project(&space.folder(), &["object".into(), "open".into(), slug], false).await; refresh_objects(s); });
                let _ = root;
            }
        }
    };
    // a chat onto the terminal: its lines, and its history for the agent's next turn (the agent starts over in it)
    let open_session = {
        let (brain, turn_task, resume) = (brain.clone(), turn_task.clone(), resume.clone());
        move |mut s: S, root: PathBuf, id: Option<String>| {
            if let Some(t) = turn_task.borrow_mut().take() {
                t.cancel();
            }
            s.thinking.set(false);
            let found = id.as_deref().and_then(|i| crate::sessions::load(&root, i));
            let (sess, lines, history) = match found {
                Some(x) => x,
                None => match crate::sessions::create(&root, "New chat") {
                    Ok(n) => (n, Vec::new(), Vec::new()),
                    Err(e) => return note(s, format!("Couldn't start a chat: {e}")),
                },
            };
            if let Some(max) = lines.iter().map(|l| l.id).max() {
                NEXT_ID.fetch_max(max + 1, std::sync::atomic::Ordering::Relaxed);
            }
            s.lines.set(lines);
            s.session.set(Some(sess));
            s.sessions_open.set(false);
            *resume.borrow_mut() = Some(history);
            let brain = brain.clone();
            spawn(async move { *brain.lock().await = None; });
            // a question asked in this project and not answered yet
            spawn(async move {
                let (j, code) = crate::host::project(&root, &["question".into()], true).await;
                s.question.set(serde_json::from_str::<serde_json::Value>(&j).ok().filter(|v| code == 0 && v.is_object()));
            });
        }
    };

    // into a workspace (workspace.rs): it becomes the folder every space call works in, goes first in the recent list,
    // and its most recent space opens (a fresh workspace gets an Untitled one to name from the first request). The
    // agent starts over in it: its spaces folder and shell are the workspace's.
    let enter = {
        let (show, brain, turn_task, open_session) = (show.clone(), brain.clone(), turn_task.clone(), open_session.clone());
        move |mut s: S, ws: crate::workspace::Workspace| {
            crate::workspace::set_current(Some(ws.clone()));
            if let Err(e) = crate::workspace::remember_in(&crate::workspace::recent_file(), &ws) {
                crate::mcp::log(format!("recent.json: {e}"));
            }
            crate::mcp::log(format!("workspace: {} ({})", ws.name(), ws.root.display()));
            if let Some(t) = turn_task.borrow_mut().take() {
                t.cancel();
            }
            s.thinking.set(false);
            let brain = brain.clone();
            spawn(async move { *brain.lock().await = None; });
            // the most recent object on the stage (a new project has none: the first request makes one), and the most
            // recent chat
            let first = ws.spaces().into_iter().next();
            s.workspace.set(Some(ws.clone()));
            s.chooser.set(false);
            s.box_open.set(false);
            match first {
                Some(mut sp) => { sp.touch(); show(s, sp); }
                None => { s.space.set(None); s.frame_url.set(None); s.ts.set(false); refresh_objects(s); }
            }
            open_session(s, ws.root.clone(), crate::sessions::list(&ws.root).first().map(|x| x.id.clone()));
        }
    };

    // at launch: the folder given (`void PATH`), else the last workspace if it's recent and still there, else the
    // chooser (the legacy spaces are on its list)
    use_hook({
        let enter = enter.clone();
        move || {
            spawn(async move {
                let mut s = s;
                match crate::workspace::reopen(&crate::workspace::recent_file()).map(|p| crate::workspace::Workspace::open(&p)) {
                    Some(Ok(ws)) => enter(s, ws),
                    Some(Err(e)) => { note(s, format!("{e}")); s.chooser.set(true); }
                    None => s.chooser.set(true),
                }
                match Provider::resolve().await {
                    Ok(p) => s.brain_label.set(p.label.clone()),
                    Err(e) => note(s, format!("{e}")),
                }
            });
        }
    });

    // playback: a clock at the program's frame rate. It starts once half a second is buffered, shows each frame when
    // its time comes, and when the frame due isn't rendered yet it holds (buffering) rather than skipping, then
    // restarts the clock from there
    use_effect({
        let request = request.clone();
        move || {
            // the frame clock plays only while there's no preview video for this version (and never for a TypeScript
            // project: it plays its render)
            if !(s.playing)() || (s.ts)() || (s.preview)() == Some(*s.version.peek()) {
                return;
            }
            let request = request.clone();
            spawn(async move {
                let mut s = s;
                let Some(shape) = *s.shape.peek() else { return s.playing.set(false) };
                let fps = shape.fps.max(1.0);
                let tick = std::time::Duration::from_secs_f64(0.5 / fps);
                let preroll = (fps * 0.5) as i64;
                request(s);
                let mut anchor: Option<(std::time::Instant, i64)> = None;
                while *s.playing.peek() && *s.preview.peek() != Some(*s.version.peek()) {
                    let v = *s.version.peek();
                    let f = *s.frame.peek();
                    let Some((t0, f0)) = anchor else {
                        // preroll: wait for the next half second (or the end of the program) to be rendered
                        let ready = (f..(f + preroll).min(shape.frames)).all(|g| frames::has((v, g)));
                        s.buffering.set(!ready);
                        if ready {
                            anchor = Some((std::time::Instant::now(), f));
                        }
                        tokio::time::sleep(tick).await;
                        continue;
                    };
                    let due = f0 + (t0.elapsed().as_secs_f64() * fps) as i64;
                    let due = if due >= shape.frames { 0 } else { due };
                    if due < f0 && due == 0 {
                        // looped: restart the clock at the top
                        anchor = Some((std::time::Instant::now(), 0));
                    }
                    if due != f {
                        if frames::has((v, due)) {
                            s.frame.set(due);
                            s.frame_url.set(Some(frame_url(v, due)));
                            s.buffering.set(false);
                            request(s);
                        } else {
                            // the frame due isn't rendered: hold here and restart the clock when it is
                            s.frame.set(due);
                            s.buffering.set(true);
                            request(s);
                            anchor = None;
                        }
                    }
                    tokio::time::sleep(tick).await;
                }
                s.buffering.set(false);
                request(s);
            });
        }
    });

    // memory (mem.rs): every 2 s, the footprint against the budget; shed the frame cache, then listening
    use_future({
        let voice = voice.clone();
        move || {
            let voice = voice.clone();
            let mut s = s;
            async move {
                let mut was = mem::Level::Ok;
                loop {
                    tokio::time::sleep(std::time::Duration::from_secs(2)).await;
                    // the window's number for screenshots, read again: at launch it can still name a placeholder window
                    {
                        let ns = crate::win::ns_window();
                        if !ns.is_null() {
                            let n: isize = unsafe { objc2::msg_send![&*(ns as *const objc2::runtime::AnyObject), windowNumber] };
                            crate::mcp::WINDOW.store(n, std::sync::atomic::Ordering::Relaxed);
                        }
                    }
                    let bytes = mem::footprint();
                    let level = mem::level(bytes);
                    s.mem.set(bytes);
                    if level > mem::Level::Ok {
                        frames::retain_version(Some(*s.version.peek()));
                    }
                    if level != was {
                        crate::mcp::log(format!("memory: {} MB, {level:?}", bytes >> 20));
                        if level == mem::Level::Hard {
                            frames::retain_version(None);
                            if voice.listening() {
                                voice.mute();
                                note(s, format!("Memory is high ({} MB): stopped listening to free its models", bytes >> 20));
                            }
                        }
                        was = level;
                    }
                }
            }
        }
    });

    let actions = use_coroutine({
        let (voice, brain, turn_task, open_space, show, request, vtx) =
            (voice.clone(), brain.clone(), turn_task.clone(), open_space.clone(), show.clone(), request.clone(), vtx.clone());
        let vrx = vrx.clone();
        let enter = enter.clone();
        let shell = shell.clone();
        let kick = kick.clone();
        let (view, open_session, resume) = (view.clone(), open_session.clone(), resume.clone());
        move |mut rx: UnboundedReceiver<Action>| {
            let (view, open_session, resume) = (view.clone(), open_session.clone(), resume.clone());
            let (voice, brain, turn_task, open_space, show, request, vtx) =
                (voice.clone(), brain.clone(), turn_task.clone(), open_space.clone(), show.clone(), request.clone(), vtx.clone());
            let shell = shell.clone();
            let enter = enter.clone();
            let kick = kick.clone();
            let mut vrx = vrx.borrow_mut().take();
            let mut s = s;
            async move {
                let Some(mut vrx) = vrx.take() else { return };
                // one brain turn, cancellable (barge-in, or a new message)
                let ask = {
                    let (voice, brain, turn_task, show, request) = (voice.clone(), brain.clone(), turn_task.clone(), show.clone(), request.clone());
                    let open_space = open_space.clone();
                    let kick = kick.clone();
                    let resume = resume.clone();
                    move |s: S, text: String, spoken: bool| {
                        if let Some(t) = turn_task.borrow_mut().take() {
                            t.cancel();
                        }
                        // the router: how this line is taken, shown as the terminal's route line and told to the brain
                        let r = crate::route::route(&text);
                        crate::mcp::log(format!("route: {} <- {}", r.label(), text.chars().take(80).collect::<String>()));
                        let text = format!("{}{text}", r.hint());
                        voice.stop();
                        let (voice, brain, show, request, open_space) = (voice.clone(), brain.clone(), show.clone(), request.clone(), open_space.clone());
                        let kick = kick.clone();
                        let resume = resume.clone();
                        let task = spawn(async move {
                            let mut s = s;
                            let mut guard = brain.lock().await;
                            if guard.is_none() {
                                match Provider::resolve().await {
                                    Ok(p) => {
                                        s.brain_label.set(p.label.clone());
                                        let mut b = Brain::new(p, open_space.clone());
                                        if let Some(h) = resume.borrow_mut().take() { b.set_history(h); }
                                        *guard = Some(b);
                                    }
                                    Err(e) => return note(s, format!("No model to answer with: {e}")),
                                }
                            }
                            let b = guard.as_mut().unwrap();
                            b.repair();
                            s.thinking.set(true);
                            let speak = spoken || *s.listening.peek();
                            let mut reply: Option<u64> = None;
                            let mut what = String::new();
                            let mut clause = String::new();
                            let mut ev = |e: BrainEvent| {
                                let mut s = s;
                                match e {
                                    BrainEvent::Text(t) => {
                                        let id = *reply.get_or_insert_with(|| push(s.lines, Who::Void, "", true));
                                        if let Some(l) = s.lines.write().iter_mut().find(|l| l.id == id) {
                                            l.text.push_str(&t);
                                        }
                                        // held until the round ends: a round that calls tools was thinking out loud
                                        if speak {
                                            clause.push_str(&t);
                                        }
                                    }
                                    BrainEvent::Doing(name, w) => {
                                        crate::mcp::log(format!("brain: {name} {w}"));
                                        // voice just before a step was a murmur, not the reply
                                        if let Some(id) = reply.take() {
                                            if let Some(l) = s.lines.write().iter_mut().find(|l| l.id == id) {
                                                l.murmur = true;
                                                l.live = false;
                                            }
                                        }
                                        let id = push(s.lines, Who::Tool, crate::narrate::doing(&name, &w), true);
                                        if let Some(l) = s.lines.write().iter_mut().find(|l| l.id == id) {
                                            l.verb = crate::narrate::verb(&name, &w).to_string();
                                        }
                                        what = w;
                                    }
                                    BrainEvent::Done(name, ok, out) => {
                                        crate::mcp::log(format!("brain: {name} {} {}", if ok { "ok" } else { "refused" }, out.lines().next().unwrap_or("")));
                                        let (m, t) = crate::narrate::step(&name, &what, ok, &out);
                                        let detail = if ok { crate::narrate::detail(&name, &what, &out) } else { Vec::new() };
                                        if let Some(l) = s.lines.write().iter_mut().rev().find(|l| l.who == Who::Tool && l.live) {
                                            l.live = false;
                                            l.ok = ok && m != '✗';
                                            l.mark = m;
                                            l.text = t;
                                            l.detail = detail;
                                        }
                                    }
                                    BrainEvent::Open(space) => show(s, space),
                                    BrainEvent::Ask(q) => s.question.set(Some(q)),
                                    // a TypeScript project: the stage's watcher looks now (ts_watch)
                                    BrainEvent::Changed if s.space.peek().as_ref().is_some_and(|sp| sp.is_ts()) => kick.notify_one(),
                                    BrainEvent::Changed => request(s),
                                    BrainEvent::RoundEnd(tools) => {
                                        if tools {
                                            clause.clear();
                                        } else if speak {
                                            clause.push(' ');
                                            for c in intent::clauses(&mut clause) {
                                                voice.say(&c);
                                            }
                                        }
                                    }
                                    BrainEvent::Error(e) => note(s, e),
                                    BrainEvent::End => {
                                        if speak && !clause.trim().is_empty() {
                                            voice.say(&clause);
                                            clause.clear();
                                        }
                                    }
                                }
                            };
                            b.turn(&text, &mut ev).await;
                            for l in s.lines.write().iter_mut().filter(|l| l.live && l.who != Who::You) {
                                l.live = false;
                            }
                            s.thinking.set(false);
                            // the session kept whole: its lines and the model's history
                            if let (Some(sess), Some(root)) = (s.session.peek().clone(), project_root(s)) {
                                let lines: Vec<Line> = s.lines.peek().iter().rev().take(400).rev().cloned().collect();
                                let _ = crate::sessions::save(&root, &sess, &lines, b.history());
                            }
                            refresh_objects(s);
                            // the turn's over: a TypeScript stage takes its render, or renders the project now
                            kick.notify_one();
                        });
                        *turn_task.borrow_mut() = Some(task);
                    }
                };

                loop {
                    tokio::select! {
                        a = rx.next() => {
                            let Some(a) = a else { break };
                            match a {
                                Action::Submit(text, spoken) => match { *SENT.lock().unwrap() = Some(std::time::Instant::now()); shell.borrow_mut().set_input(&text); shell.borrow_mut().take_input(); s.scroll.set(0); intent::parse(&natural(&text)) } {
                                    None => {}
                                    // `/name args`: one of the person's commands, or a skill, sent to the agent expanded (skills.rs)
                                    Some(Intent::Ask(q)) if q.starts_with('/') && crate::skills::expand(project_root(s).as_deref(), &q).is_some() => {
                                        let expanded = crate::skills::expand(project_root(s).as_deref(), &q).unwrap_or_default();
                                        push(s.lines, Who::You, q.clone(), false);
                                        ask(s, expanded, spoken);
                                    }
                                    Some(Intent::Skills) => {
                                        let root = project_root(s);
                                        let sk = crate::skills::skills(root.as_deref());
                                        let cm = crate::skills::commands(root.as_deref());
                                        let mut lines = Vec::new();
                                        if !cm.is_empty() { lines.push(format!("Your commands: {}", cm.iter().map(|c| format!("/{}", c.name)).collect::<Vec<_>>().join("  "))); }
                                        lines.push(if sk.is_empty() { "No skills on this computer (~/.claude/skills, ~/.raea-void/skills, the project's .void/skills)".into() } else { format!("Skills: {}", sk.iter().map(|x| format!("/{}", x.name)).collect::<Vec<_>>().join("  ")) });
                                        lines.push("Type /name and what you want; add commands as markdown files in ~/.raea-void/commands or the project's .void/commands".into());
                                        for l in lines { note(s, l); }
                                    }
                                    Some(Intent::Ask(q)) => {
                                        // a new chat takes its name from the first thing asked in it
                                        let fresh = s.session.peek().clone().filter(|x| x.name == "New chat");
                                        if let (Some(mut sess), Some(root)) = (fresh, project_root(s)) {
                                            if crate::sessions::rename(&root, &mut sess, &spaces::name_from(&q)).is_ok() {
                                                s.session.set(Some(sess));
                                            }
                                        }
                                        push(s.lines, Who::You, q.clone(), false);
                                        ask(s, q, spoken);
                                    }
                                    Some(Intent::Play(on)) => s.playing.set(on),
                                    Some(Intent::Layout(l)) => {
                                        let l = l.filter(|l| l != "top").unwrap_or_else(|| match s.layout.peek().as_str() { "bottom" => "right", "right" => "left", _ => "bottom" }.to_string());
                                        s.layout.set(l.clone());
                                        save_setting("layout", l.into());
                                    }
                                    Some(Intent::Spaces) => { s.spaces.set(spaces::list()); s.box_open.set(true); }
                                    Some(Intent::Open(name)) => match spaces::find(&name) {
                                        Some(mut sp) => { sp.touch(); note(s, format!("Opened {}", sp.name)); show(s, sp); }
                                        None => note(s, format!("No space called “{name}”")),
                                    },
                                    Some(Intent::New(name)) => match spaces::create(&name).and_then(|sp| spaces::scaffold(&sp).map(|_| sp)) {
                                        Ok(sp) => { note(s, format!("New space · {}", sp.name)); show(s, sp); }
                                        Err(e) => note(s, format!("{e}")),
                                    },
                                    Some(Intent::Voice(None)) => s.voice_open.set(true),
                                    Some(Intent::Voice(Some(v))) => {
                                        let v = v.to_lowercase();
                                        if Pocket::voices().contains(&v) {
                                            voice.set_voice(&v); s.voice.set(v.clone()); save_setting("voice", v.clone().into());
                                            note(s, format!("Voice · {v}"));
                                        } else {
                                            note(s, format!("Voices: {}", Pocket::voices().join(", ")));
                                        }
                                    }
                                    Some(Intent::Listen) => { voice.listen(vtx.clone()); }
                                    Some(Intent::Mute) => { voice.mute(); s.listening.set(false); }
                                    // a project's changes are the shell's commands, each taken back by the agent's
                                    // shell (undo: true); there are no rrl moves to undo any more
                                    Some(Intent::Undo(_)) => note(s, "Ask for it: the agent's shell takes back its last commands"),
                                    Some(Intent::Clear) => {
                                        s.lines.write().clear();
                                        if let Some(b) = brain.lock().await.as_mut() { b.forget(); }
                                    }
                                    Some(Intent::Dock(v)) => {
                                        let on = v.unwrap_or(!*s.autohide.peek());
                                        s.autohide.set(on); save_setting("autohide", on.into());
                                    }
                                    Some(Intent::Cache(None)) => note(s, frames::report()),
                                    Some(Intent::Cache(Some(what))) => match frames::clear(&what) {
                                        Ok(m) => {
                                            s.buffered.set(Vec::new());
                                            // previews gone: render this version's again (the version effect reruns)
                                            if what == "previews" || what == "all" {
                                                s.preview.set(None);
                                                let v = *s.version.peek();
                                                s.version.set(0);
                                                s.version.set(v);
                                            }
                                            note(s, m);
                                            request(s);
                                        }
                                        Err(e) => note(s, format!("{e}")),
                                    },
                                    Some(Intent::Help) => note(s, "Say or type what you want made or changed. The box (top right) holds your spaces; right-click anywhere for settings. Commands: /spaces /new /open (open all: the workspace's root) /voice /listen /mute /undo /clear /dock"),
                                },
                                Action::OpenSpace(slug) => {
                                    if let Some(mut sp) = spaces::list().into_iter().find(|sp| sp.slug == slug) {
                                        sp.touch(); show(s, sp);
                                    }
                                    s.box_open.set(false);
                                }
                                Action::NewSpace(name) => {
                                    if let Ok(sp) = spaces::create(&name).and_then(|sp| spaces::scaffold(&sp).map(|_| sp)) { show(s, sp); }
                                    s.box_open.set(false);
                                }
                                Action::DeleteSpace(slug) => {
                                    if let Some(sp) = spaces::list().into_iter().find(|sp| sp.slug == slug) {
                                        let _ = spaces::delete(&sp);
                                        if s.space.peek().as_ref().is_some_and(|o| o.slug == slug) {
                                            *open_space.lock().unwrap() = None;
                                            s.space.set(None);
                                            s.frame_url.set(None);
                                        }
                                    }
                                    s.spaces.set(spaces::list());
                                }
                                Action::OpenWorkspace(path, new) => {
                                    let ws = if new { crate::workspace::Workspace::create(&path) } else { crate::workspace::Workspace::open(&path) };
                                    match ws {
                                        Ok(ws) => { note(s, format!("Workspace · {} ({})", ws.name(), crate::workspace::tilde(&ws.root))); enter(s, ws); }
                                        Err(e) => note(s, format!("Couldn't open {}: {e}", path.display())),
                                    }
                                }
                                Action::Answer(qid, answer) => {
                                    let folder = s.space.peek().as_ref().filter(|_| s.viewing.peek().is_none()).map(|sp| sp.folder()).or_else(|| project_root(s));
                                    let Some(folder) = folder else { continue };
                                    let (said, code) = crate::host::project(&folder, &["answer".into(), qid.clone(), answer.clone()], true).await;
                                    let v: serde_json::Value = serde_json::from_str(&said).unwrap_or_default();
                                    if code != 0 {
                                        note(s, v["error"].as_str().unwrap_or(&said).to_string());
                                        continue;
                                    }
                                    let q = s.question.peek().clone().unwrap_or_default();
                                    s.question.set(None);
                                    let chosen = v["other"].as_str().map(str::to_string).unwrap_or_else(|| v["answer"].as_array().into_iter().flatten().filter_map(|a| a.as_str()).collect::<Vec<_>>().join(", "));
                                    // what each chosen option means goes with it, as the agent wrote it
                                    let meant: Vec<String> = q["options"].as_array().into_iter().flatten()
                                        .filter(|o| v["answer"].as_array().is_some_and(|a| a.iter().any(|x| x == &o["label"])))
                                        .map(|o| format!("{}: {}", o["label"].as_str().unwrap_or(""), o["meaning"].as_str().unwrap_or(""))).collect();
                                    push(s.lines, Who::You, chosen.clone(), false);
                                    let text = format!("(answering {qid}, \"{}\") {}", v["text"].as_str().unwrap_or(""), if meant.is_empty() { chosen } else { meant.join("; ") });
                                    ask(s, text, false);
                                }
                                Action::OpenSession(id) => {
                                    if let Some(root) = project_root(s) { open_session(s, root, Some(id)); }
                                }
                                Action::NewSession => {
                                    if let Some(root) = project_root(s) { open_session(s, root, None); }
                                }
                                Action::OpenVersion(slug, n) => {
                                    let Some(sp) = spaces::list().into_iter().find(|sp| sp.slug == slug) else { continue };
                                    let dir = spaces::home().join("checkouts").join(format!("{slug}-v{n}"));
                                    let _ = std::fs::remove_dir_all(&dir);
                                    let (said, code) = crate::host::project(&sp.folder(), &["checkout".into(), n.to_string(), dir.to_string_lossy().into_owned(), "--object".into(), slug.clone()], false).await;
                                    if code != 0 { note(s, said); continue; }
                                    // its footage is the object's (a version keeps code, not media)
                                    let _ = std::os::unix::fs::symlink(sp.folder().join("media"), dir.join("media"));
                                    let mut old = Space::at(&dir, &format!("{} · v{n}", sp.name));
                                    old.kind = sp.kind.clone().or_else(|| kind_of(&sp.folder()));
                                    s.box_open.set(false);
                                    s.viewing.set(Some((slug, n)));
                                    view(s, old);
                                }
                                Action::BackToObject => {
                                    s.viewing.set(None);
                                    if let Some(sp) = open_space.lock().unwrap().clone() { view(s, sp); }
                                }
                                Action::Restore(slug, n) => {
                                    let Some(sp) = spaces::list().into_iter().find(|sp| sp.slug == slug) else { continue };
                                    if *s.thinking.peek() { note(s, "void is working: restore when it's done"); continue; }
                                    // through the agent's own shell, so the restore is one of its commands and undo takes it back
                                    let said = {
                                        let mut guard = brain.lock().await;
                                        match guard.as_mut() {
                                            Some(b) => b.exec_in(&sp.folder(), &format!("raea restore {n}")).await,
                                            None => match crate::shell::Terminal::open(&sp.folder()).await {
                                                Ok(mut t) => t.exec(&format!("raea restore {n}"), 120_000).await,
                                                Err(e) => Err(e),
                                            },
                                        }
                                    };
                                    note(s, match said { Ok(t) => t.trim().to_string(), Err(e) => format!("Couldn't restore: {e}") });
                                    s.viewing.set(None);
                                    show(s, sp);
                                    kick.notify_one();
                                }
                                Action::Menu(x, y, what) => {
                                    // kept on the window: a menu near an edge opens inward
                                    let (w, h) = (crate::ui::window_width(), crate::ui::window_height());
                                    s.menu_for.set(what);
                                    s.menu.set(Some(((x).clamp(8.0, (w - 250.0).max(8.0)), if y > h - 320.0 { (y - 300.0).max(8.0) } else { y })));
                                }
                                Action::ForgetWorkspace(path) => {
                                    let _ = crate::workspace::forget_in(&crate::workspace::recent_file(), &path);
                                    // the chooser reads the list as it draws: nudge it
                                    s.chooser.set(false);
                                    s.chooser.set(true);
                                }
                                Action::Listen(on) => {
                                    voice.chime(on);
                                    if on { voice.listen(vtx.clone()); } else { voice.mute(); s.listening.set(false); }
                                }
                                Action::Layout(l) => {
                                    let l = if l == "top" { "bottom".to_string() } else { l };
                                    s.layout.set(l.clone());
                                    save_setting("layout", l.into());
                                    // the stage's box changed: redraw at the new size
                                    request(s);
                                }
                                Action::SetVoice(v) => {
                                    voice.set_voice(&v); s.voice.set(v.clone()); save_setting("voice", v.into());
                                }
                                Action::Scrub(f) => {
                                    s.playing.set(false);
                                    s.frame.set(f);
                                    request(s);
                                }
                                Action::Play(on) => s.playing.set(on),
                                Action::Import(paths) => match s.space.peek().clone() {
                                    Some(sp) => {
                                        s.files_tab.set(true);
                                        s.term_open.set(true);
                                        refresh_assets(s);
                                        spawn(import(s, sp.folder(), paths));
                                    }
                                    None => note(s, "Open a space first, then bring files into it"),
                                },
                                Action::RemoveAsset(rel) => {
                                    if let Some(sp) = s.space.peek().clone() {
                                        match crate::assets::remove(&sp.folder(), &rel) {
                                            Ok(()) => {
                                                note(s, format!("Removed {rel}"));
                                                crate::brain::notice(&sp.folder(), format!("removed {rel}"));
                                            }
                                            Err(e) => note(s, format!("Couldn't remove {rel}: {e}")),
                                        }
                                        refresh_assets(s);
                                    }
                                }
                                Action::Clear => s.lines.write().clear(),
                                Action::History(up) => {
                                    let line = shell.borrow_mut().history(up).to_string();
                                    s.draft.set(line);
                                }
                                Action::Stop => {
                                    voice.stop();
                                    if let Some(t) = turn_task.borrow_mut().take() { t.cancel(); }
                                    for l in s.lines.write().iter_mut().filter(|l| l.live && l.who != Who::You) { l.live = false; }
                                    s.thinking.set(false);
                                    s.speaking.set(false);
                                    note(s, "stopped");
                                }
                            }
                        }
                        e = vrx.recv() => {
                            let Some(e) = e else { continue };
                            match e {
                                VoiceEvent::Status(t) => { crate::mcp::log(format!("voice: {t}")); s.status.set(t) }
                                VoiceEvent::Listening(on) => { s.listening.set(on); if !on { s.level.set(0.0); } }
                                VoiceEvent::VoiceReady => s.voice_ready.set(true),
                                VoiceEvent::Level(l) => {
                                    s.level.set(l);
                                    let mut w = s.wave.write();
                                    w.remove(0);
                                    w.push(l);
                                }
                                VoiceEvent::Hearing(words) => {
                                    // what you're saying, dim on the prompt line until the turn ends
                                    s.hearing.set(crate::intent::clean_speech(&words));
                                }
                                VoiceEvent::Heard(words) => {
                                    let words = crate::intent::clean_speech(&words);
                                    s.hearing.set(String::new());
                                    if words.is_empty() { continue }
                                    push(s.lines, Who::You, words.clone(), false);
                                    match intent::parse(&words) {
                                        Some(Intent::Ask(q)) => ask(s, q, true),
                                        _ => {}
                                    }
                                }
                                VoiceEvent::Speaking(on) => s.speaking.set(on),
                                VoiceEvent::Measured(what, took, text) => {
                                    // voice.say's time includes waiting for the speaker to drain: it's the clause as heard
                                    let mut span = crate::telemetry::tracer().root(what);
                                    span.backdate(took).kind("CHAIN").set("raea.voice.text", text.as_str()).set("raea.voice.ms", took.as_millis() as u64);
                                    span.end();
                                }
                                VoiceEvent::BargeIn => {
                                    voice.stop();
                                    if let Some(t) = turn_task.borrow_mut().take() { t.cancel(); }
                                    for l in s.lines.write().iter_mut().filter(|l| l.live && l.who != Who::You) { l.live = false; }
                                    s.thinking.set(false);
                                    s.speaking.set(false);
                                }
                            }
                        }
                    }
                }
            }
        }
    });

    // the MCP server (mcp.rs): an agent's calls answered from the window's own state
    let calls = use_hook({
        let (voice, vtx) = (voice.clone(), vtx.clone());
        move || {
            let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
            crate::mcp::serve(tx, voice, vtx);
            Rc::new(std::cell::RefCell::new(Some(rx)))
        }
    });
    use_hook(|| {
        let ns = crate::win::ns_window();
        if !ns.is_null() {
            let n: isize = unsafe { objc2::msg_send![&*(ns as *const objc2::runtime::AnyObject), windowNumber] };
            crate::mcp::WINDOW.store(n, std::sync::atomic::Ordering::Relaxed);
            // a test instance (RAEA_VOID_BACKGROUND=1) in front, so macOS keeps drawing it, but never the key window: the
            // person's typing stays in their app
                    if std::env::var("RAEA_VOID_BACKGROUND").is_ok() {
                let w = unsafe { &*(ns as *const objc2::runtime::AnyObject) };
                // on every Space and over other windows (a covered window, or one on another Space, isn't drawn)
                let _: () = unsafe { objc2::msg_send![w, setCollectionBehavior: (1usize << 0) | (1usize << 8)] };
                let _: () = unsafe { objc2::msg_send![w, orderFrontRegardless] };
                let _: () = unsafe { objc2::msg_send![w, setLevel: 3isize] };
            }
        }
        // the native stage under the stage's box (stage_native: a view under the webview, which is transparent there), the
        // RAEA engine drawing live objects; RAEA_VOID_STAGE=web keeps the webview player
        if !ns.is_null() && std::env::var("RAEA_VOID_STAGE").as_deref() != Ok("web") {
            let mut n = s.native;
            match crate::stage_native::open(ns as *mut std::ffi::c_void) {
                Ok(()) => {
                    n.set(true);
                    crate::mcp::log("native stage: open");
                    // the native chrome draws the stage itself, as a texture in its own frame: the glass over it blurs the
                    // game, and one present shows both (an agent session's composited stage)
                    // (RAEA_VOID_STAGE_LAYER=1 keeps the stage in its own layer under the chrome instead)
                                    if std::env::var("RAEA_VOID_STAGE_LAYER").is_err() {
                        crate::stage_native::composited(true);
                        crate::stage_native::on_frame(Box::new(crate::win::redraw));
                    }
                }
                Err(e) => crate::mcp::log(format!("native stage: couldn't open ({e}); the webview player plays live objects")),
            }
        }
    });
    use_future(move || {
        let rx = calls.borrow_mut().take();
        async move {
            let Some(mut rx) = rx else { return };
            while let Some((call, reply)) = rx.recv().await {
                spawn(async move {
                    let _ = reply.send(answer(s, actions, call).await);
                });
            }
        }
    });

    // files dragged in from Finder, on the native chrome: Blitz drops winit's drag events, so the window's own events show
    // the drop zone and bring the files in (the webview's are the page's dragover and drop, ui.rs DropZone)
    dioxus_native::use_window_event(move |ev, el| {
        use dioxus_native::winit::{data_transfer::TypeHint, event::WindowEvent, event_loop::DndAction};
        let mut d = s.dropping;
        match ev {
            // a drag is accepted only for the actions said valid (none, and macOS refuses the drop)
            WindowEvent::DragEntered { id, .. } | WindowEvent::DragPosition { id, .. } => {
                let _ = el.set_valid_dnd_actions(*id, &[DndAction::Copy]);
                if !*d.peek() { d.set(true); }
            }
            WindowEvent::DragLeft { .. } => d.set(false),
            WindowEvent::DragDropped { id, .. } => {
                d.set(false);
                if let Err(e) = el.fetch_data_transfer(*id, &TypeHint::UriList) {
                    crate::mcp::log(format!("drop: no files ({e})"));
                }
            }
            WindowEvent::DataTransferReceived { value, .. } => match value.try_as_file_paths() {
                Ok(paths) if !paths.is_empty() => actions.send(Action::Import(paths)),
                Ok(_) => {}
                Err(e) => crate::mcp::log(format!("drop: {e}")),
            },
            _ => {}
        }
    });

    let send = move |a: Action| actions.send(a);

    rsx! {
        div {
            class: "void",
            class: if (s.drag)() { "resizing" },
            class: "lay-{s.layout}",
            // the native stage shows through the page where it's transparent (stage_native: its view sits under the webview)
            class: if (s.native_on)() { "native" },
            oncontextmenu: move |e| {
                e.prevent_default();
                let p = e.client_coordinates();
                let (mut m, mut f) = (s.menu, s.menu_for);
                f.set(MenuFor::Window);
                m.set(Some((p.x, p.y)));
            },
            onclick: move |_| {
                let (mut m, mut b) = (s.menu, s.box_open);
                if m.peek().is_some() { m.set(None); }
                if *b.peek() { b.set(false); }
            },
            // files dragged in from Finder: the drop zone takes them (ui.rs DropZone)
            ondragover: move |e| {
                e.prevent_default();
                let mut d = s.dropping;
                if !*d.peek() { d.set(true); }
            },
            onmousemove: move |e| {
                // resizing the terminal by its inner edge
                if !*s.drag.peek() { return; }
                let p = e.client_coordinates();
                let (w, h) = (crate::ui::window_width(), crate::ui::window_height());
                let (mut th, mut tw) = (s.term_h, s.term_w);
                match s.layout.peek().as_str() {
                    "left" => tw.set(p.x.clamp(260.0, w * 0.7)),
                    "right" => tw.set((w - p.x).clamp(260.0, w * 0.7)),
                    _ => th.set((h - p.y - crate::ui::HEAD_H).clamp(90.0, h * 0.8)),
                }
            },
            onmouseup: move |_| {
                let mut d = s.drag;
                if *d.peek() {
                    d.set(false);
                    save_setting("terminal_h", (*s.term_h.peek()).into());
                    save_setting("terminal_w", (*s.term_w.peek()).into());
                }
            },
            onkeydown: move |e: KeyboardEvent| {
                let (mut b, mut v, mut m, mut t) = (s.box_open, s.voice_open, s.menu, s.term_open);
                // ctrl+` shows or hides the terminal, as in an IDE
                if e.modifiers().ctrl() && e.key() == Key::Character("`".into()) {
                    e.prevent_default();
                    let on = !*t.peek();
                    t.set(on);
                    save_setting("terminal", on.into());
                    if on { crate::focus::focus("term-input"); }
                    return;
                }
                if e.key() == Key::Escape {
                    b.set(false); v.set(false); m.set(None);
                    crate::focus::focus("term-input");
                }
            },
            // the native chrome has no page head: its stylesheet is an element (the webview's is in main.rs's custom head)
            {native_style()}
            div { class: "prism" }
            div { class: "drag", onmousedown: move |_| crate::win::drag() }
            crate::ui::StageView { s, send }
            crate::ui::Terminal { s, send }
            crate::ui::BoxMenu { s, send }
            crate::ui::SessionsMenu { s, send }
            crate::ui::QuestionPanel { s, send }
            crate::ui::VoicePanel { s, send }
            crate::ui::ContextMenu { s, send }
            crate::ui::DropZone { s, send }
            crate::chooser::Chooser { s, send }
        }
    }
}

fn native_style() -> Element {
    // Blitz runs no CSS animations yet, and a sheet's entrance (`pop`, from opacity 0) would hold it invisible
    // and no :has(): the page is see-through at its root, the black on the app's own element (or, with the native
    // stage showing, as the stage box's shadow, a hole where the stage is)
    const NATIVE: &str = "*, *::before, *::after { animation: none !important; } html, body, #main { background: transparent !important; } .void { background: #000; } .void.native { background: transparent; }";
    rsx! { style { {crate::CSS} } style { {NATIVE} } }
}

/// Plain commands without the slash ("open hook", "play", "undo"), when the router calls them commands.
fn natural(text: &str) -> String {
    let t = text.trim();
    if t.starts_with('/') || crate::route::route(t) != crate::route::Route::Command {
        return t.to_string();
    }
    format!("/{t}")
}
