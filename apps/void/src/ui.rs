//! The window's parts. Styles are in assets/void.css.

use dioxus::prelude::*;
use dioxus::html::HasFileData;

use crate::app::{Action, S};
use crate::intent;
use raea_void::voice::Pocket;

/// The window's width in CSS pixels.
pub fn window_width() -> f64 {
    crate::win::size().0
}

/// The window's height in CSS pixels (the dock's reveal edge).
pub fn window_height() -> f64 {
    crate::win::size().1
}

fn ago(t: u64) -> String {
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    let d = now.saturating_sub(t);
    match d {
        0..60 => "now".into(),
        60..3600 => format!("{}m", d / 60),
        3600..86400 => format!("{}h", d / 3600),
        _ => format!("{}d", d / 86400),
    }
}

#[component]
pub fn StageView(s: S, send: EventHandler<Action>) -> Element {
    let space = (s.space)();
    let viewing = (s.viewing)();
    let kind = space.as_ref().and_then(|sp| sp.kind.clone().or_else(|| crate::app::kind_of(&sp.folder())));
    let native = (s.native_on)();
    let key_name = |k: Key| match k { Key::Character(c) => c, other => other.to_string() };
    // the open space's panels around the stage (space_ui.rs); none: the stage fills the box
    let layout = crate::space_ui::CURRENT();
    // a space's own header (a panel named header) stands in for void's row over the stage, and takes its place; neither
    // void's row nor its player bar shows while the launch's space is on its way (space_ui::waiting)
    let waiting = crate::space_ui::waiting();
    let own_header = waiting || layout.as_ref().is_some_and(|l| l.own_header());
    let top = if own_header { 0.0 } else { 40.0 };
    // the shell's side panels shown or not, and their widths (shell_ui.rs; ⌘B, ⌘J, the cube, the cat, their edges)
    let sh = crate::shell_ui::SHOWN();
    let boxes = layout.as_ref().map(|l| l.boxes(top, sh)).unwrap_or_default();
    // a space's own player (a panel named player) stands in for void's player bar
    let own_player = waiting || layout.as_ref().is_some_and(|l| l.own_player());
    let with_bar = !own_player && (s.native_play)().1 > 0;
    // a shell edge being dragged: which, and where the pointer was against the panel's width
    // (which, the pointer's coordinate less or plus the size as it was, across or not, which way it grows)
    let resizing = use_signal(|| None::<(&'static str, f64, bool, f64)>);
    // what the panels are told (space_ui::context): the conversation's last lines and the object on the stage, again
    // whenever either changes; and what a terminal of the space's own sends, submitted as if typed
    use_effect(move || {
        if crate::space_ui::CURRENT.read().is_none() {
            return;
        }
        let lines = s.lines.read();
        let from = lines.len().saturating_sub(40);
        let transcript: Vec<serde_json::Value> = lines[from..]
            .iter()
            .map(|l| serde_json::json!({"who": format!("{:?}", l.who).to_lowercase(), "text": l.text, "live": l.live}))
            .collect();
        let object = s.space.read().as_ref().map(|sp| serde_json::json!({"name": sp.name, "kind": sp.kind})).unwrap_or(serde_json::Value::Null);
        let state = serde_json::json!({"busy": (s.thinking)() || (s.speaking)(), "speaking": (s.speaking)(), "listening": (s.listening)(), "chat": (s.session)().map(|x| x.name).unwrap_or("void".into())});
        crate::space_ui::context(vec![format!("void:transcript {}", serde_json::Value::from(transcript)), format!("void:object {object}"), format!("void:state {state}")]);
    });
    // what the space's sheets list, each told when it changes (merged by name: space_ui::context)
    use_effect(move || {
        if crate::space_ui::CURRENT.read().is_none() {
            return;
        }
        let current = s.space.read().as_ref().map(|sp| sp.slug.clone());
        let record = s.objects.read().clone();
        let items: Vec<serde_json::Value> = s.spaces.read().iter().map(|sp| {
            let r = record.iter().find(|o| o["name"].as_str() == Some(sp.slug.as_str())).cloned().unwrap_or_default();
            serde_json::json!({"slug": sp.slug, "name": sp.name, "kind": r["kind"].as_str().map(str::to_string).or(sp.kind.clone()).unwrap_or("video".into()), "state": r["state"], "current": current.as_deref() == Some(sp.slug.as_str())})
        }).collect();
        let workspace = s.workspace.read().as_ref().map(|w| w.meta.name.clone()).unwrap_or("Objects".into());
        let chats: Vec<serde_json::Value> = {
            let now = s.session.read().as_ref().map(|x| x.id.clone());
            s.workspace.read().as_ref().map(|w| crate::sessions::list(&w.root)).unwrap_or_default().into_iter()
                .map(|x| serde_json::json!({"id": x.id, "name": x.name, "ago": ago(x.updated.max(x.created)), "current": now.as_deref() == Some(x.id.as_str())})).collect()
        };
        let files: Vec<serde_json::Value> = s.assets.read().iter().map(|a| serde_json::json!({"rel": a.rel, "name": a.name, "kind": format!("{:?}", a.kind).to_lowercase(), "bytes": a.bytes})).collect();
        let voice = serde_json::json!({"voices": Pocket::voices(), "voice": (s.voice)(), "listening": (s.listening)(), "ready": (s.voice_ready)(), "brain": (s.brain_label)()});
        let question = s.question.read().clone().unwrap_or(serde_json::Value::Null);
        let here = s.workspace.read().as_ref().map(|w| w.root.clone());
        let workspaces: Vec<serde_json::Value> = crate::workspace::choices(&crate::workspace::recent_file(), &crate::workspace::legacy_root()).into_iter()
            .map(|r| serde_json::json!({"path": r.path, "name": r.name, "ago": if r.opened > 0 { ago(r.opened) } else { String::new() }, "current": here.as_deref() == Some(r.path.as_path())})).collect();
        crate::space_ui::context(vec![
            format!("void:objects {}", serde_json::json!({"workspace": workspace, "items": items})),
            format!("void:chats {}", serde_json::Value::from(chats)),
            format!("void:files {}", serde_json::Value::from(files)),
            format!("void:voice {voice}"),
            format!("void:question {question}"),
            format!("void:workspaces {}", serde_json::json!({"open": here.is_some(), "items": workspaces})),
        ]);
    });
    // what the shell's panels are told (shell_ui.rs): its side panels shown, the open space, the spaces there are
    use_effect(move || {
        let Some(l) = crate::space_ui::CURRENT() else { return };
        let sh = crate::shell_ui::SHOWN();
        let root = s.workspace.read().as_ref().map(|w| w.root.clone());
        crate::shell_ui::root(root.as_deref());
        let spaces: Vec<serde_json::Value> = crate::space_ui::list(root.as_deref()).into_iter().map(|(slug, _)| serde_json::json!({"slug": slug})).collect();
        crate::space_ui::context(vec![
            format!("void:shell {}", serde_json::json!({"files": sh.files, "chat": sh.chat, "dock": sh.dock, "space": l.slug, "title": l.title(), "spaces": spaces})),
            format!("void:dock {}", crate::shell_ui::dock_json()),
        ]);
        crate::shell_ui::tell_shown();
    });
    // void's own sheets give way to the space's of the same name: what opened one opens the space's instead (a sheet
    // called objects, chats, voice, settings or files; question while the agent asks one)
    use_effect(move || {
        let Some(l) = crate::space_ui::CURRENT.read().clone() else { return };
        let mut redirect = |open: bool, name: &str, mut off: Box<dyn FnMut()>| {
            if let (true, Some(i)) = (open, l.sheet(name)) {
                off();
                if !crate::space_ui::SHEETS.peek().contains(&i) {
                    crate::space_ui::show_sheet(i, true);
                }
            }
        };
        redirect((s.box_open)(), "objects", Box::new(move || { let mut b = s.box_open; b.set(false); }));
        redirect((s.sessions_open)(), "chats", Box::new(move || { let mut b = s.sessions_open; b.set(false); }));
        redirect((s.voice_open)(), "voice", Box::new(move || { let mut b = s.voice_open; b.set(false); }));
        let files_menu = matches!((s.menu_for)(), crate::app::MenuFor::Files | crate::app::MenuFor::File(..));
        redirect((s.menu)().is_some(), if files_menu { "files" } else { "settings" }, Box::new(move || { let mut m = s.menu; m.set(None); }));
        redirect((s.files_tab)(), "files", Box::new(move || { let mut f = s.files_tab; f.set(false); }));
        redirect((s.chooser)(), "workspaces", Box::new(move || { let mut c = s.chooser; c.set(false); }));
        if let Some(i) = l.sheet("question") {
            let asking = s.question.read().is_some();
            if asking != crate::space_ui::SHEETS.peek().contains(&i) {
                crate::space_ui::show_sheet(i, asking);
            }
        }
    });
    use_future(move || async move {
        let mut tree = 0;
        loop {
            // the workspace's tree, again when the folder changed or a folder in it opened (shell_ui.rs)
            if let Some((g, t)) = crate::shell_ui::tree_since(tree) {
                tree = g;
                crate::space_ui::context(vec![format!("void:tree {t}")]);
            }
            for text in crate::space_ui::take_sends() {
                send.call(Action::Submit(text, false));
            }
            // what a header or terminal of the space's own asked of void's chrome; a menu opens where the panel was pressed
            for (i, what, x, y, arg) in crate::space_ui::take_does() {
                crate::mcp::log(format!("panels: panel {i} does {what}"));
                let sheet = |name: &str| crate::space_ui::CURRENT.peek().as_ref().and_then(|l| l.sheet(name));
                match what.as_str() {
                    "stop" => send.call(Action::Stop),
                    "listen" => send.call(Action::Listen(!*s.listening.peek())),
                    "close" => crate::space_ui::show_sheet(i, false),
                    w if w.starts_with("open:") || w.starts_with("close:") => {
                        let (on, name) = w.split_once(':').map(|(a, b)| (a == "open", b)).unwrap_or((false, ""));
                        match sheet(name) {
                            Some(k) => crate::space_ui::show_sheet(k, on),
                            None => crate::mcp::log(format!("panels: no sheet called {name}")),
                        }
                    }
                    // void's own sheets, or the space's of the same name (the redirect below opens it)
                    "chats" => { let mut o = s.sessions_open; let on = !*o.peek(); o.set(on); }
                    "objects" => { let mut sp = s.spaces; sp.set(crate::spaces::list()); crate::app::refresh_objects(s); let mut b = s.box_open; b.set(true); }
                    "voice" => { let mut v = s.voice_open; v.set(true); }
                    "files" => { crate::app::refresh_assets(s); let (mut f, mut o) = (s.files_tab, s.term_open); f.set(true); o.set(true); }
                    "settings" => {
                        let at = match PANEL_NAMES.get(i) { Some(n) => crate::focus::rect(n).await, None => None };
                        let (ox, oy) = at.map(|r| (r.0, r.1)).unwrap_or((0.0, 0.0));
                        let mut f = s.menu_for;
                        f.set(crate::app::MenuFor::Terminal);
                        let mut m = s.menu;
                        m.set(Some((ox + x, oy + y)));
                    }
                    // the shell's: the cube and the cat show and hide the files and the chat; the tree's folders open
                    // and close, a composition in it opens, a space in its second section opens
                    "toggle" => crate::shell_ui::toggle(&arg),
                    "tree-open" | "tree-close" => crate::shell_ui::open(&arg, what == "tree-open"),
                    "tree-file" => match arg.strip_prefix("objects/").and_then(|r| r.split('/').next()).filter(|o| !o.is_empty()) {
                        Some(slug) => send.call(Action::OpenSpace(slug.to_string())),
                        None => crate::mcp::log(format!("panels: {arg} isn't a composition (file actions come with P4)")),
                    },
                    "space" => send.call(Action::Submit(format!("/space {arg}"), false)),
                    // the dock's: a terminal shown, closed or opened (+ and its picker), the dock moved round (⇅)
                    "dock-tab" => crate::shell_ui::show_tab(arg.parse().unwrap_or(0)),
                    "dock-close" => crate::shell_ui::close_tab(arg.parse().unwrap_or(0)),
                    "dock-new" => crate::shell_ui::open_tab(&arg),
                    "dock-side" => crate::shell_ui::dock_side(crate::shell_ui::Side::parse(&arg)),
                    // what the sheets act on
                    "open-object" => send.call(Action::OpenSpace(arg)),
                    "new-object" => send.call(Action::NewSpace(if arg.trim().is_empty() { "Untitled".into() } else { arg })),
                    "open-chat" => send.call(Action::OpenSession(arg)),
                    "new-chat" => send.call(Action::NewSession),
                    "clear" => send.call(Action::Clear),
                    "set-voice" => send.call(Action::SetVoice(arg)),
                    "answer" => if let Some(q) = s.question.peek().clone() { send.call(Action::Answer(q["id"].as_str().unwrap_or("").to_string(), arg)) },
                    "import" => { spawn(async move { if let Some(p) = pick_files().await { send.call(Action::Import(p)); } }); }
                    "file-open" | "file-reveal" => {
                        if let Some(f) = s.space.peek().as_ref().map(|sp| sp.folder().join(&arg)) {
                            if what == "file-open" { let _ = std::process::Command::new("open").arg(&f).spawn(); } else { crate::assets::reveal(&f); }
                        }
                    }
                    "file-ask" => send.call(Action::Submit(format!("look at {arg} and tell me what's in it"), false)),
                    "open-workspace" => send.call(Action::OpenWorkspace(std::path::PathBuf::from(arg), false)),
                    "forget-workspace" => send.call(Action::ForgetWorkspace(std::path::PathBuf::from(arg))),
                    "open-folder" => crate::chooser::pick(send, false),
                    "new-workspace" => crate::chooser::pick(send, true),
                    other => crate::mcp::log(format!("panels: no such thing to do: {other}")),
                }
            }
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        }
    });
    let box_style = match layout.as_ref() {
        Some(l) => {
            let (t, r, b, le) = l.insets(sh);
            format!("top: {}px; right: {r}px; bottom: {}px; left: {le}px", top + t, b + if with_bar { 46.0 } else { 0.0 })
        }
        None => String::new(),
    };
    rsx! {
        div { class: "stage", class: if native { "live" }, style: "{stage_inset(s)}",
            id: "stage",
            oncontextmenu: move |e| { e.prevent_default(); e.stop_propagation(); let p = e.client_coordinates(); send.call(Action::Menu(p.x, p.y, crate::app::MenuFor::Stage)); },
            // the native stage takes the keys while it has focus (a click on it gives it focus)
            tabindex: "0",
            onkeydown: move |e: KeyboardEvent| if dock_key(&e) { e.prevent_default(); } else if native && (s.stage_keys)() && !cmd(e.modifiers()) { e.prevent_default(); crate::stage_native::key(&key_name(e.key()), true); }
                else if let Some(i) = *crate::space_ui::FOCUS.peek() { if dock_key(&e) || cli_key(i, &e) { e.prevent_default(); } else if !cmd(e.modifiers()) { e.prevent_default(); crate::space_ui::key(i, &key_name(e.key()), true); } else if key_name(e.key()) == "v" { e.prevent_default(); crate::space_ui::paste(i, &paste()); } else if shell_key(&key_name(e.key())) { e.prevent_default(); } },
            onkeyup: move |e: KeyboardEvent| if native && (s.stage_keys)() { crate::stage_native::key(&key_name(e.key()), false); }
                else if let Some(i) = *crate::space_ui::FOCUS.peek() { crate::space_ui::key(i, &key_name(e.key()), false); },
            // the cube: the project's objects (BoxMenu), top-left of the stage; a space's own header stands in for it
            // (an earlier version open still shows here, with its way back)
            div { class: "stage-tl", style: if own_header && viewing.is_none() { "display: none" } else { "" }, onclick: move |e| e.stop_propagation(),
                Cube { s }
                span { class: "obj-name", {space.as_ref().map(|sp| sp.name.clone()).unwrap_or_default()} }
                if let Some(k) = kind.clone() { span { class: "obj-kind", "{k}" } }
                if let Some((slug, n)) = viewing.clone() {
                    span { class: "viewing",
                        "an earlier version, v{n}"
                        button { onclick: move |_| send.call(Action::BackToObject), "Back to now" }
                        button { class: "warn", onclick: { let slug = slug.clone(); move |_| send.call(Action::Restore(slug.clone(), n)) }, "Restore v{n}" }
                    }
                }
            }
            // the native stage's box: the RAEA engine's layer sits over it; the pointer over it goes to the engine
            if native {
                div { id: "native-box", class: if with_bar { "native-box with-bar" } else { "native-box" }, style: "{box_style}",
                    onmounted: move |e| crate::focus::keep("native-box", e),
                    onpointermove: move |e| { let p = e.element_coordinates(); crate::stage_native::pointer(p.x, p.y, !e.held_buttons().is_empty()); },
                    onpointerdown: move |e| { let mut k = s.stage_keys; k.set(true); *crate::space_ui::FOCUS.write() = None; let p = e.element_coordinates(); crate::stage_native::pointer(p.x, p.y, true); },
                    onpointerup: move |e| { let p = e.element_coordinates(); crate::stage_native::pointer(p.x, p.y, false); },
                    {stage_texture(s)}
                    if *s.native_wait.read() {
                        div { class: "stage-wait", "Preparing…" }
                    }
                }
                if !own_player {
                    PlayerBar { s }
                }
            }
            // the open space's panels, whether or not an object is on the stage
            for (i, b) in boxes.into_iter().enumerate() {
                PanelView { key: "{i}", s, i, place: b }
            }
            // the shell's side panels' inner edges, dragged to widen them (kept: shell_ui.rs)
            if layout.as_ref().is_some_and(|l| l.own_header()) {
                if sh.files {
                    div { class: "shell-edge", style: "left: {sh.files_w - 3.0}px",
                        onpointerdown: move |e| { e.stop_propagation(); let mut r = resizing; r.set(Some(("files", e.client_coordinates().x - sh.files_w, false, 1.0))); } }
                }
                if sh.chat {
                    div { class: "shell-edge", style: "right: {sh.chat_w - 3.0}px",
                        onpointerdown: move |e| { e.stop_propagation(); let mut r = resizing; r.set(Some(("chat", e.client_coordinates().x + sh.chat_w, false, -1.0))); } }
                }
                // the dock's, across it: grows away from its edge
                if let Some((place, across)) = layout.as_ref().and_then(|l| l.dock_edge(top, sh)) {
                    div { class: if across { "shell-edge across" } else { "shell-edge" }, style: "{place}",
                        onpointerdown: move |e| {
                            e.stop_propagation();
                            let p = e.client_coordinates();
                            let sign = match sh.dock_side { crate::shell_ui::Side::Bottom | crate::shell_ui::Side::Right => -1.0, _ => 1.0 };
                            let at = if across { p.y } else { p.x };
                            let mut r = resizing;
                            r.set(Some(("dock", at - sign * sh.dock_size, across, sign)));
                        } }
                }
            }
            if resizing().is_some() {
                // over everything while an edge is dragged, so the panels under the pointer don't take it
                div { class: "shell-drag",
                    onpointermove: move |e| if let Some((which, from, across, sign)) = resizing() {
                        let p = e.client_coordinates();
                        crate::shell_ui::width(which, sign * ((if across { p.y } else { p.x }) - from));
                    },
                    onpointerup: move |_| { let mut r = resizing; r.set(None); },
                }
            }
            // the keys of the panel that last took a click: a press on a panel focuses this (Blitz focuses only what's
            // focusable, and a click elsewhere leaves nothing focused, so the window's keys reached no handler), and it
            // hands them on, Command-V as the clipboard
            if let Some(i) = *crate::space_ui::FOCUS.read() {
                input {
                    id: "panel-keys",
                    key: "{crate::focus::epoch(\"panel-keys\")}",
                    autofocus: true,
                    onmounted: move |e| crate::focus::keep("panel-keys", e),
                    class: "ghost",
                    autocomplete: "off",
                    spellcheck: false,
                    value: "",
                    onkeydown: move |e: KeyboardEvent| {
                        e.stop_propagation();
                        e.prevent_default();
                        let k = key_name(e.key());
                        if dock_key(&e) || cli_key(i, &e) {} else if !cmd(e.modifiers()) { crate::space_ui::key(i, &k, true); } else if k == "v" { crate::space_ui::paste(i, &paste()); } else { shell_key(&k); }
                    },
                    onkeyup: move |e: KeyboardEvent| { e.stop_propagation(); crate::space_ui::key(i, &key_name(e.key()), false); },
                }
            }
            // nothing on the stage yet: the object's name and the hint
            if !native {
                div { class: "empty",
                    div { class: "glyph" }
                    p { class: "title",
                        {space.as_ref().map(|sp| sp.name.clone()).unwrap_or("void".into())}
                    }
                    p { class: "hint", "Say or type anything" }
                }
            }
        }
    }
}

/// A shortcut of the shell's, with Command held: ⌘B shows or hides the files, ⌘J the chat. Whether it was one.
pub fn shell_key(k: &str) -> bool {
    match k.to_lowercase().as_str() {
        "b" => crate::shell_ui::toggle("files"),
        "j" => crate::shell_ui::toggle("chat"),
        _ => return false,
    }
    true
}

/// Control-` shows or hides the dock, as in VS Code. Whether it was that.
fn dock_key(e: &KeyboardEvent) -> bool {
    let m = e.modifiers();
    if m.ctrl() && !cmd(m) && format!("{:?}", e.code()) == "Backquote" {
        crate::shell_ui::toggle("dock");
        return true;
    }
    false
}

/// A key on a cli terminal (pty.rs) goes to its shell as the bytes a terminal sends, Control and Option with it,
/// Command aside (⌘V pastes, ⌘B, ⌘J). Whether panel i is one.
fn cli_key(i: usize, e: &KeyboardEvent) -> bool {
    let Some(id) = crate::space_ui::cli_of(i) else { return false };
    let m = e.modifiers();
    if cmd(m) {
        return false;
    }
    // Option makes a letter something else on a Mac keyboard: the key under it is what a terminal sends after Escape
    let code = format!("{:?}", e.code());
    let k = match (m.alt(), code.strip_prefix("Key").or_else(|| code.strip_prefix("Digit"))) {
        (true, Some(c)) if c.len() == 1 => c.to_lowercase(),
        _ => match e.key() { Key::Character(c) => c, other => other.to_string() },
    };
    if let Some(b) = crate::pty::key_bytes(id, &k, m.ctrl(), m.alt()) {
        crate::pty::input(id, b);
    }
    true
}

/// Command held: the native chrome's keys say so as SUPER (winit's name for it), a webview's as META.
pub fn cmd(m: Modifiers) -> bool {
    m.meta() || m.contains(Modifiers::SUPER)
}

/// The names panels' boxes are kept under (focus::keep takes a static name).
const PANEL_NAMES: [&str; 32] = [
    "panel-0", "panel-1", "panel-2", "panel-3", "panel-4", "panel-5", "panel-6", "panel-7", "panel-8", "panel-9", "panel-10", "panel-11", "panel-12", "panel-13", "panel-14", "panel-15",
    "panel-16", "panel-17", "panel-18", "panel-19", "panel-20", "panel-21", "panel-22", "panel-23", "panel-24", "panel-25", "panel-26", "panel-27", "panel-28", "panel-29", "panel-30", "panel-31",
];

/// Panel i's box in the window, in points (x, y, w, h): where a click on it lands (act panel_rect).
pub async fn panel_rect(i: usize) -> Option<(f64, f64, f64, f64)> {
    crate::focus::rect(PANEL_NAMES.get(i).copied()?).await
}

/// One of the open space's panels: its texture (space_ui paints it), the pointer over it and, after a click, the keys.
/// Its box is measured as it lays out and the panel's page laid out again at it.
#[component]
pub fn PanelView(s: S, i: usize, place: String) -> Element {
    let name = PANEL_NAMES.get(i).copied();
    use_future(move || async move {
        let Some(name) = name else { return };
        let mut last = (0.0, 0.0, 0.0, u64::MAX);
        loop {
            if let Some((_, _, w, h)) = crate::focus::rect(name).await {
                // told again after a new set of panels (space_ui::SETS), whose panel here opens only once told
                let now = (w.round(), h.round(), crate::win::scale(), crate::space_ui::SETS.load(std::sync::atomic::Ordering::Relaxed));
                if now != last && w > 1.0 && h > 1.0 {
                    crate::space_ui::rect(i, now.0, now.1, now.2);
                    last = now;
                }
            }
            tokio::time::sleep(std::time::Duration::from_millis(200)).await;
        }
    });
    // a sheet shows only while it's open
    let hidden = crate::space_ui::CURRENT().as_ref().is_some_and(|l| l.panels().get(i).is_some_and(|p| p.1 == "sheet")) && !crate::space_ui::SHEETS().contains(&i);
    let st = crate::space_ui::status();
    let (w, h) = st.get(i).map(|p| p.page).unwrap_or_default();
    let error = st.get(i).and_then(|p| p.error.clone());
    rsx! {
        div { class: "space-panel", style: if hidden { "{place}; display: none" } else { "{place}" },
            onmounted: move |e| if let Some(n) = name { crate::focus::keep(n, e) },
            onpointermove: move |e| { let p = e.element_coordinates(); crate::space_ui::pointer(i, p.x, p.y, !e.held_buttons().is_empty()); },
            // a press where the panel said the window moves (a header's empty stretch) drags the window, as a title bar
            onmousedown: move |e| { let p = e.element_coordinates(); if crate::space_ui::drags_at(i, p.x, p.y) { crate::mcp::log(format!("panels: panel {i} drags the window")); crate::win::drag(); } },
            onpointerdown: move |e| { let mut k = s.stage_keys; k.set(false); *crate::space_ui::FOCUS.write() = Some(i); crate::focus::focus("panel-keys"); let p = e.element_coordinates(); crate::space_ui::pointer(i, p.x, p.y, true); },
            onpointerup: move |e| { let p = e.element_coordinates(); crate::space_ui::pointer(i, p.x, p.y, false); },
            onwheel: move |e| crate::space_ui::wheel(i, e.delta().strip_units().y),
            if w > 0 {
                img { "data-raea-texture": "{crate::space_ui::texture(i)}", "data-raea-w": "{w}", "data-raea-h": "{h}",
                    // at its page's own size, from the top-left: a box resized shows the last frame as it was until the
                    // panel paints at the new size (stretched to the box, a resize squashed it)
                    style: "position: absolute; left: 0; top: 0; width: {w}px; height: {h}px; pointer-events: none" }
            }
            if let Some(e) = error {
                div { class: "panel-error", "{e}" }
            }
        }
    }
}

/// A timeline's player under the stage (a video, motion, music, a 3D scene): play or pause, the playhead to click or
/// drag, and the time. Hidden (a placeholder, never nothing: see below) for a game, which plays on.
#[component]
pub fn PlayerBar(s: S) -> Element {
    let (frame, frames, rate, playing) = (s.native_play)();
    let rate = if rate > 0.0 { rate } else { 30.0 };
    let clock = |f: i64| { let t = (f.max(0) as f64 / rate) as i64; format!("{}:{:02}", t / 60, t % 60) };
    let fill = if frames > 0 { (frame as f64 / (frames - 1).max(1) as f64 * 100.0).clamp(0.0, 100.0) } else { 0.0 };
    // a point on the track to a frame: its width read from the track as laid out
    let seek = move |x: f64| {
        spawn(async move {
            if let Some((_, _, w, _)) = crate::focus::rect("scrub").await {
                if w > 1.0 && frames > 0 {
                    let f = ((x / w).clamp(0.0, 1.0) * (frames - 1) as f64).round() as i64;
                    crate::stage_native::control(crate::stage_native::Control::Seek(f));
                    let mut p = s.native_play;
                    let now = *p.peek();
                    p.set((f, now.1, now.2, now.3));
                }
            }
        });
    };
    let toggle = move |_| {
        let mut u = s.user_paused;
        if playing {
            u.set(true);
            crate::stage_native::control(crate::stage_native::Control::Pause);
        } else {
            u.set(false);
            crate::stage_native::control(crate::stage_native::Control::Play);
        }
    };
    rsx! {
        div { class: if frames > 0 { "player-bar" } else { "player-bar off" }, onclick: move |e| e.stop_propagation(),
            button { class: "pp", title: if playing { "Pause" } else { "Play" }, onclick: toggle,
                if playing {
                    svg { width: "14", height: "14", view_box: "0 0 24 24", fill: "currentColor", rect { x: "6", y: "5", width: "4", height: "14", rx: "1" } rect { x: "14", y: "5", width: "4", height: "14", rx: "1" } }
                } else {
                    svg { width: "14", height: "14", view_box: "0 0 24 24", fill: "currentColor", path { d: "M8 5.5v13a1 1 0 0 0 1.5.86l10.5-6.5a1 1 0 0 0 0-1.72L9.5 4.64A1 1 0 0 0 8 5.5z" } }
                }
            }
            span { class: "t", "{clock(frame)}" }
            div { class: "pb-scrub", id: "scrub",
                onmounted: move |e| crate::focus::keep("scrub", e),
                onpointerdown: move |e| seek(e.element_coordinates().x),
                onpointermove: move |e| if !e.held_buttons().is_empty() { seek(e.element_coordinates().x) },
                div { class: "pb-track",
                    div { class: "pb-fill", style: "width: {fill}%" }
                    div { class: "pb-head", style: "left: {fill}%" }
                }
            }
            span { class: "t end", "{clock(frames)}" }
        }
    }
}

// A closed sheet, menu or panel renders a hidden placeholder, never nothing: dioxus-core 0.8 alpha panics (mount.rs,
// anchor_slot) looking for an anchor through a sibling component that rendered no nodes, as switching objects after a
// drop did.

/// The cat: void's mark (brand/logo/cat-outline-simple.svg, traced from the founder's pixel cat), left of the cube; it
/// opens the app's settings (the menu a right-click opens), anchored under it.
#[component]
pub fn CatButton(s: S) -> Element {
    rsx! {
        button {
            class: "box cat",
            title: "Settings",
            onclick: move |e| {
                e.stop_propagation();
                let p = e.client_coordinates();
                let mut m = s.menu;
                let w = window_width();
                let h = window_height();
                m.set(Some(((p.x - 10.0).clamp(8.0, w - 230.0), if p.y > h - 280.0 { p.y - 270.0 } else { p.y + 14.0 })));
            },
            svg { width: "18", height: "18", view_box: "0 0 24 24", fill: "none", stroke: "currentColor", stroke_width: "1.5", stroke_linejoin: "round", stroke_linecap: "round",
                polygon { points: "6.156,1 8.219,1 9.594,7.875 11.656,3.062 13.719,3.062 14.406,5.125 19.906,3.75 20.594,13.375 19.219,14.75 21.969,18.875 21.281,20.938 15.094,18.188 13.031,18.875 11.656,23 9.594,23 8.906,18.188 6.844,16.812 4.094,20.938 2.031,20.25 3.406,12 6.156,8.562" }
                path { d: "M14.75 8.863v1.462" }
                path { d: "M18.188 8.863v1.462" }
            }
        }
    }
}

/// The cube: the project's objects, at the stage's top-left.
#[component]
pub fn Cube(s: S) -> Element {
    let open = (s.box_open)();
    rsx! {
        button {
            class: "box",
            class: if open { "open" },
            title: "Objects",
            onclick: move |e| {
                e.stop_propagation();
                let (mut b, mut l) = (s.box_open, s.spaces);
                l.set(crate::spaces::list());
                crate::app::refresh_objects(s);
                b.set(!open);
            },
            {cube_mark()}
        }
    }
}

/// The stage drawn as a texture in the chrome's own frame (the default; RAEA_VOID_STAGE_LAYER=1 keeps its own layer
/// under the chrome)
fn composited_stage() -> bool {
    std::env::var("RAEA_VOID_STAGE_LAYER").is_err()
}

/// The composited stage: an image carrying the stage's texture (stage_native::TEXTURE), drawn by the chrome in its own
/// frame.
fn stage_texture(s: S) -> Element {
    let (w, h) = (s.native_page)();
    // the last object's frame gone while the next one bakes (the surface's black shows, under "Preparing"). Always the
    // one wrapper, see-through (a background behind the texture would cover it): a component switching between nothing
    // and nodes in place panicked dioxus-core's mount (anchor_slot) when an object was switched
    let show = w > 0 && composited_stage() && !*s.native_wait.read();
    rsx! {
        div { style: "position: absolute; top: 0; right: 0; bottom: 0; left: 0; pointer-events: none",
            if show {
                img { "data-raea-texture": "{crate::stage_native::TEXTURE}", "data-raea-w": "{w}", "data-raea-h": "{h}",
                    style: "width: 100%; height: 100%; object-fit: contain; border-radius: 14px; pointer-events: none" }
            }
        }
    }
}

/// The cube's mark: a drawn cube (Blitz has no 3D transforms)
fn cube_mark() -> Element {
    rsx! {
        svg { width: "16", height: "16", view_box: "0 0 24 24", fill: "none", stroke: "rgba(255,255,255,0.8)", stroke_width: "1.4", stroke_linejoin: "round",
            path { d: "M12 2.5 20.5 7v10L12 21.5 3.5 17V7Z" }
            path { d: "M3.5 7 12 11.5 20.5 7M12 11.5v10" }
        }
    }
}

/// The project's objects, cascading out of the cube: newest first, each with what it is, draft or ready and its last
/// version; `+` for a new one, and typing filters them (↑ ↓ to move, return to open; with nothing matching, return makes
/// one of that name). A row's version opens its versions: any one can be looked at (checked out beside the object) or
/// restored. No text field: a hidden input takes the keys and the filter shows as a line only while you type.
#[component]
pub fn BoxMenu(s: S, send: EventHandler<Action>) -> Element {
    let mut filter = use_signal(String::new);
    let mut pick = use_signal(|| 0usize);
    let mut confirm = use_signal(|| None::<String>);
    // the object whose versions are open, and them
    let mut versions_of = use_signal(|| None::<String>);
    let mut versions = use_signal(Vec::<serde_json::Value>::new);
    if !(s.box_open)() {
        return rsx! { div { class: "slot", style: "display: none" } };
    }
    let current = (s.space)().map(|sp| sp.slug);
    let q = filter().trim().to_lowercase();
    let ws = (s.workspace)();
    let shown: Vec<crate::spaces::Space> = s.spaces.read().iter().filter(|sp| q.is_empty() || sp.name.to_lowercase().contains(&q)).cloned().collect();
    let record = (s.objects)();
    let rec = move |slug: &str| record.iter().find(|o| o["name"].as_str() == Some(slug)).cloned().unwrap_or_default();
    let head = ws.map(|w| w.meta.name).unwrap_or_else(|| "Objects".into());
    let n = shown.len();
    let at = pick().min(n.saturating_sub(1));
    let slugs: Vec<String> = shown.iter().map(|sp| sp.slug.clone()).collect();
    rsx! {
        div {
            class: "sheet spaces",
            style: "{spaces_box(s)}",
            onclick: move |e| e.stop_propagation(),

            div { class: "sheet-head",
                span { "{head}" }
                span { class: "count", "{s.spaces.read().len()}" }
                button {
                    class: "plus",
                    title: "New object",
                    onclick: move |_| { send.call(Action::NewSpace("Untitled".into())); filter.set(String::new()); },
                    "+"
                }
            }
            // the keys: typing filters, arrows move, return opens or makes
            input {
                id: "spaces-keys",
                key: "{crate::focus::epoch(\"spaces-keys\")}",
                autofocus: true,
                onmounted: move |e| { crate::focus::keep("spaces-keys", e); crate::focus::focus("spaces-keys"); },
                class: "ghost",
                value: "{filter}",
                autocomplete: "off",
                spellcheck: false,
                oninput: move |e| { filter.set(e.value()); pick.set(0); },
                onkeydown: {
                    let slugs = slugs.clone();
                    move |e: KeyboardEvent| match e.key() {
                        Key::ArrowDown => { e.prevent_default(); if n > 0 { pick.set((at + 1) % n); } }
                        Key::ArrowUp => { e.prevent_default(); if n > 0 { pick.set((at + n - 1) % n); } }
                        Key::Enter => {
                            e.prevent_default();
                            match slugs.get(at) {
                                Some(slug) => send.call(Action::OpenSpace(slug.clone())),
                                None if !filter.peek().trim().is_empty() => send.call(Action::NewSpace(filter.peek().trim().to_string())),
                                None => {}
                            }
                            filter.set(String::new());
                        }
                        Key::Escape => { let mut b = s.box_open; b.set(false); }
                        _ => {}
                    }
                },
            }
            if !q.is_empty() {
                div { class: "filter", span { class: "f", "{filter}" } span { class: "caret" } }
            }
            div { class: "list",
                for (i, sp) in shown.into_iter().enumerate() {
                    {
                    let r = rec(&sp.slug);
                    let kind = r["kind"].as_str().map(str::to_string).or(sp.kind.clone()).unwrap_or("video".into());
                    let st = r["state"].as_str().unwrap_or(if sp.has_program() { "no render" } else { "empty" }).to_string();
                    let last = r["last"].as_u64();
                    let st_class = format!("state st-{}", st.replace(' ', "-"));
                    let open_v = versions_of().as_deref() == Some(sp.slug.as_str());
                    rsx! {
                    div {
                        key: "{sp.slug}",
                        class: if i == at { "row picked" } else { "row" },
                        class: if current.as_deref() == Some(sp.slug.as_str()) { "current" },
                        style: "--i: {i}",
                        onmouseenter: move |_| pick.set(i),
                        onclick: {
                            let slug = sp.slug.clone();
                            move |_| send.call(Action::OpenSpace(slug.clone()))
                        },
                        span { class: "dot", style: "--h: {sp.hue}" }
                        span { class: "name", "{sp.name}" }
                        span { class: "kind", "{kind}" }
                        span { class: "{st_class}", "{st}" }
                        if let Some(n) = last {
                            button { class: if open_v { "vers on" } else { "vers" }, title: "Versions",
                                onclick: {
                                    let (slug, folder) = (sp.slug.clone(), sp.folder());
                                    move |e: MouseEvent| {
                                        e.stop_propagation();
                                        if versions_of.peek().as_deref() == Some(slug.as_str()) { versions_of.set(None); return; }
                                        versions_of.set(Some(slug.clone()));
                                        let folder = folder.clone();
                                        spawn(async move {
                                            let (j, _) = crate::host::project(&folder, &["versions".into()], true).await;
                                            versions.set(serde_json::from_str::<Vec<serde_json::Value>>(&j).unwrap_or_default().into_iter().rev().collect());
                                        });
                                    }
                                },
                                "v{n} ▾"
                            }
                        }
                        span { class: "meta", {if sp.has_program() { ago(sp.opened.max(sp.created)) } else { "empty".into() }} }
                        button {
                            class: "del",
                            class: if confirm().as_deref() == Some(sp.slug.as_str()) { "armed" },
                            title: "Delete",
                            onclick: {
                                let slug = sp.slug.clone();
                                move |e: MouseEvent| {
                                    e.stop_propagation();
                                    if confirm.peek().as_deref() == Some(slug.as_str()) {
                                        send.call(Action::DeleteSpace(slug.clone()));
                                        confirm.set(None);
                                    } else {
                                        confirm.set(Some(slug.clone()));
                                    }
                                }
                            },
                            if confirm().as_deref() == Some(sp.slug.as_str()) { "Delete" } else { "×" }
                        }
                    }
                    if open_v {
                        div { class: "versions",
                            for v in versions() {
                                {
                                let n = v["n"].as_u64().unwrap_or(0);
                                let msg = v["message"].as_str().unwrap_or("").to_string();
                                let check = v["check"].as_object().map(|c| {
                                    let (t, f) = (c.get("total").and_then(|x| x.as_u64()).unwrap_or(0), c.get("fails").and_then(|x| x.as_array()).map(|a| a.len() as u64).unwrap_or(0));
                                    format!("{} {}/{t}", if c.get("full").and_then(|x| x.as_bool()) == Some(true) { "full" } else { "fast" }, t - f.min(t))
                                }).unwrap_or("not checked".into());
                                let at = v["at"].as_u64().unwrap_or(0) / 1000;
                                let slug = sp.slug.clone();
                                rsx! {
                                    div { key: "{n}", class: "vrow",
                                        span { class: "vn", "v{n}" }
                                        span { class: "vmsg", "{msg}" }
                                        span { class: "meta", "{check} · {ago(at)}" }
                                        button { title: "Look at this version (the object stays as it is)", onclick: { let slug = slug.clone(); move |e: MouseEvent| { e.stop_propagation(); send.call(Action::OpenVersion(slug.clone(), n)) } }, "Open" }
                                        button { class: "warn", title: "Put the object back as this version (undo takes it back)", onclick: { let slug = slug.clone(); move |e: MouseEvent| { e.stop_propagation(); send.call(Action::Restore(slug.clone(), n)) } }, "Restore" }
                                    }
                                }
                                }
                            }
                        }
                    }
                    }
                    }
                }
                if n == 0 && !q.is_empty() {
                    div { class: "row make", onclick: move |_| { send.call(Action::NewSpace(filter.peek().trim().to_string())); filter.set(String::new()); },
                        span { class: "plusmark", "+" }
                        span { class: "name", "New object “{filter}”" }
                        span { class: "meta", "return" }
                    }
                }
            }
        }
    }
}

/// The window's empty top strip: the traffic lights, and a place to drag the window by.
pub const TITLE_H: f64 = 38.0;
/// The terminal's header: the cube, where you are, the voice, the docks. It moves with the terminal.
pub const HEAD_H: f64 = 38.0;

/// How tall the terminal is when docked at the bottom: its header, and its screen when open.
pub fn footer_and_terminal(s: S) -> f64 {
    HEAD_H + if (s.term_open)() { (s.term_h)() } else { 0.0 }
}

/// The dock: "bottom" (the default), "left" or "right". The terminal is never on top.
pub fn dock(s: S) -> String {
    match s.layout.read().as_str() {
        "left" => "left".into(),
        "right" => "right".into(),
        _ => "bottom".into(),
    }
}

/// The stage's box: below the top strip, less the terminal at its dock.
pub fn stage_inset(s: S) -> String {
    // the shell lays out the whole window itself (its header is the title bar)
    if crate::space_ui::CURRENT().as_ref().is_some_and(|l| l.own_header()) {
        return "top: 0; right: 0; bottom: 0; left: 0".into();
    }
    let w = (s.term_w)();
    let (t, r, b, l) = match dock(s).as_str() {
        // beside a side dock the stage is full bleed, up to the window's top edge
        "left" => (16.0, 16.0, 16.0, w + 16.0),
        "right" => (16.0, w + 16.0, 16.0, 16.0),
        _ => (TITLE_H + 6.0, 24.0, footer_and_terminal(s) + 16.0, 24.0),
    };
    format!("top: {t}px; right: {r}px; bottom: {b}px; left: {l}px")
}

/// The terminal's box at its dock. Every property each time: Dioxus merges a style's properties rather than replacing
/// the string, so a dock that left one out would keep the last dock's.
fn term_box(s: S) -> String {
    let w = (s.term_w)();
    match dock(s).as_str() {
        // a side dock runs the window's full height, its header at the very top
        "left" => format!("left: 0; right: auto; top: 0; bottom: 0; width: {w}px; height: auto"),
        "right" => format!("left: auto; right: 0; top: 0; bottom: 0; width: {w}px; height: auto"),
        _ => format!("left: 0; right: 0; top: auto; bottom: 0; width: auto; height: {}px", footer_and_terminal(s)),
    }
}

/// The objects sheet, under the cube at the stage's top-left.
fn spaces_box(s: S) -> String {
    let w = (s.term_w)();
    let (t, l) = match dock(s).as_str() {
        "left" => (16.0, w + 16.0),
        "right" => (16.0, 16.0),
        _ => (TITLE_H + 6.0, 24.0),
    };
    format!("left: {}px; right: auto; top: {}px; bottom: auto", l + 6.0, t + 44.0)
}

/// The chats sheet, beside the chat's name in the terminal's header.
fn sessions_box(s: S) -> String {
    let below = HEAD_H + 6.0;
    match dock(s).as_str() {
        "left" => format!("left: 60px; right: auto; top: {below}px; bottom: auto"),
        "right" => format!("left: auto; right: {}px; top: {below}px; bottom: auto", ((s.term_w)() - 360.0).max(10.0)),
        _ => format!("left: 90px; right: auto; top: auto; bottom: {}px", HEAD_H + 8.0 + if (s.term_open)() { (s.term_h)() } else { 0.0 }),
    }
}

/// The project's chats (sessions.rs): newest first, `+` for a new one. Objects aren't a chat's: any chat works on any.
#[component]
pub fn SessionsMenu(s: S, send: EventHandler<Action>) -> Element {
    if !(s.sessions_open)() {
        return rsx! { div { class: "slot", style: "display: none" } };
    }
    let list = s.workspace.read().as_ref().map(|w| crate::sessions::list(&w.root)).unwrap_or_default();
    let current = (s.session)().map(|x| x.id);
    rsx! {
        div { class: "sheet spaces sessions", style: "{sessions_box(s)}", onclick: move |e| e.stop_propagation(),
            div { class: "sheet-head",
                span { "Chats" }
                span { class: "count", "{list.len()}" }
                button { class: "plus", title: "New chat", onclick: move |_| send.call(Action::NewSession), "+" }
            }
            div { class: "list",
                for (i, x) in list.into_iter().enumerate() {
                    div { key: "{x.id}", class: "row", class: if current.as_deref() == Some(x.id.as_str()) { "current" }, style: "--i: {i}",
                        onclick: { let id = x.id.clone(); move |_| send.call(Action::OpenSession(id.clone())) },
                        span { class: "name", "{x.name}" }
                        span { class: "meta", {ago(x.updated.max(x.created))} }
                    }
                }
            }
        }
    }
}

/// The agent's question (`raea ask`), answered by choosing, as in an agent chat: each option with what it means,
/// several when it asks for several, and always "Other" for the person's own words. The answer goes to the record and
/// the agent's turn goes on with it.
#[component]
pub fn QuestionPanel(s: S, send: EventHandler<Action>) -> Element {
    let mut chosen = use_signal(Vec::<String>::new);
    let mut other = use_signal(String::new);
    let theirs = crate::space_ui::CURRENT().as_ref().is_some_and(|l| l.sheet("question").is_some());
    let Some(q) = (s.question)().filter(|_| !theirs) else { return rsx! { div { class: "slot", style: "display: none" } } };
    let id = q["id"].as_str().unwrap_or("").to_string();
    let multi = q["multi"].as_bool() == Some(true);
    let options: Vec<(String, String)> = q["options"].as_array().into_iter().flatten()
        .map(|o| (o["label"].as_str().unwrap_or("").to_string(), o["meaning"].as_str().unwrap_or("").to_string())).collect();
    let bottom = if dock(s) == "bottom" { footer_and_terminal(s) + 14.0 } else { 24.0 };
    // over the stage, not the terminal: centred in what a side dock leaves
    let w = (s.term_w)();
    let left = match dock(s).as_str() {
        "left" => format!("calc({w}px + (100% - {w}px) / 2)"),
        "right" => format!("calc((100% - {w}px) / 2)"),
        _ => "50%".into(),
    };
    let side = if dock(s) == "bottom" { 0.0 } else { w };
    rsx! {
        div { class: "sheet question", style: "bottom: {bottom}px; left: {left}; width: min(560px, calc(100% - {side}px - 32px))", onclick: move |e| e.stop_propagation(),
            div { class: "qhead", span { class: "qid", "{id}" } span { class: "qtext", {q["text"].as_str().unwrap_or("").to_string()} } }
            div { class: "qopts",
                for (i, (label, meaning)) in options.into_iter().enumerate() {
                    button {
                        key: "{i}",
                        class: if chosen().contains(&label) { "qopt on" } else { "qopt" },
                        onclick: {
                            let (id, label) = (id.clone(), label.clone());
                            move |_| {
                                if multi {
                                    let mut c = chosen.write();
                                    if let Some(k) = c.iter().position(|x| x == &label) { c.remove(k); } else { c.push(label.clone()); }
                                } else {
                                    send.call(Action::Answer(id.clone(), label.clone()));
                                }
                            }
                        },
                        span { class: "qn", "{i + 1}" }
                        span { class: "ql", "{label}" }
                        if !meaning.is_empty() { span { class: "qm", "{meaning}" } }
                    }
                }
            }
            div { class: "qother",
                input {
                    id: "q-other",
                    placeholder: "Other: say it in your own words",
                    value: "{other}",
                    spellcheck: false,
                    oninput: move |e| other.set(e.value()),
                    onkeydown: {
                        let id = id.clone();
                        move |e: KeyboardEvent| if e.key() == Key::Enter && !other.peek().trim().is_empty() {
                            send.call(Action::Answer(id.clone(), format!("other: {}", other.peek().trim())));
                            other.set(String::new());
                        }
                    },
                }
                if multi {
                    button { class: "qsend", disabled: chosen().is_empty(),
                        onclick: { let id = id.clone(); move |_| { send.call(Action::Answer(id.clone(), chosen.peek().join(", "))); chosen.set(Vec::new()); } },
                        "Answer"
                    }
                }
            }
        }
    }
}

/// A run's inline style, every property every time: Dioxus merges a style's properties rather than replacing the
/// string, so a span reused for another run kept the last run's background (the white blocks after marks).
fn run_style(r: &crate::term::Run) -> String {
    let bg = r.bg.map(|b| format!("rgb({},{},{})", b.0, b.1, b.2)).unwrap_or("transparent".into());
    format!(
        "color: rgb({},{},{}); background: {bg}; font-weight: {}; opacity: {}; font-style: {}; text-decoration: {};",
        r.fg.0, r.fg.1, r.fg.2,
        if r.bold { 700 } else { 400 },
        if r.dim { 0.6 } else { 1.0 },
        if r.italic { "italic" } else { "normal" },
        if r.underline { "underline" } else { "none" },
    )
}

/// The voice: a small button beside the cube, four thin bars. Off: short and dim. Listening: cyan, rising and falling
/// with the mic. Speaking: violet, bouncing in a staggered rhythm. Click to turn the voice on or off.
#[component]
pub fn VoiceButton(s: S, send: EventHandler<Action>) -> Element {
    let listening = (s.listening)();
    let speaking = (s.speaking)();
    let state = if speaking { "speaking" } else if listening { "on" } else { "off" };
    // the mic's last four levels, oldest first, for the bars' heights while listening
    let w = (s.wave)();
    let levels: Vec<f32> = w.iter().rev().take(4).rev().copied().collect();
    rsx! {
        button {
            class: "mic {state}",
            title: if listening { "Voice on. Click to stop listening" } else { "Voice off. Click to talk" },
            onclick: move |e| { e.stop_propagation(); send.call(Action::Listen(!listening)) },
            for (i, l) in levels.iter().enumerate() {
                span { key: "{i}", class: "bar", style: "--l: {l}; --i: {i}" }
            }
        }
    }
}

/// A layout's icon: a little window with the terminal's side shaded.
#[component]
fn LayoutIcon(which: &'static str) -> Element {
    let (x, y, w, h) = match which {
        "top" => (1.5, 1.5, 15.0, 4.0),
        "left" => (1.5, 1.5, 5.0, 11.0),
        "right" => (11.5, 1.5, 5.0, 11.0),
        _ => (1.5, 8.5, 15.0, 4.0),
    };
    rsx! {
        svg { width: "18", height: "14", view_box: "0 0 18 14", fill: "none",
            rect { x: "1", y: "1", width: "16", height: "12", rx: "2.5", stroke: "currentColor", stroke_width: "1" }
            rect { x: "{x}", y: "{y}", width: "{w}", height: "{h}", rx: "1.2", fill: "currentColor", opacity: "0.85" }
        }
    }
}

/// The terminal, docked bottom, left or right, carrying its header: the cube (spaces), where you are, the agent's
/// state, stop, the voice, the docks and (at the bottom) the collapse chevron. Behind the header, full bleed, the
/// voice as a scanlined waveform. Below it, open, the emulated screen (term.rs), the prompt on its last line.
#[component]
pub fn Terminal(s: S, send: EventHandler<Action>) -> Element {
    // a space's own terminal stands in for this one, header row and all (a hidden placeholder, never nothing: see above)
    let speaking = (s.speaking)();
    let busy = (s.thinking)() || speaking;
    let chat = (s.session)().map(|x| x.name).unwrap_or("void".into());
    let d = dock(s);
    let bottom = d == "bottom";
    // at a side the terminal is always open; at the bottom the chevron folds it to its header
    let open = (s.term_open)() || !bottom;
    let mut draft = s.draft;
    let wheel_acc = use_signal(|| 0.0f64);
    // a space's own terminal stands in for this one, header row and all, and none shows while the launch's space is on
    // its way (a hidden placeholder, never nothing: see above; after the hooks, so they run in the same order every
    // render)
    if crate::space_ui::waiting() || crate::space_ui::CURRENT().as_ref().is_some_and(|l| l.own_terminal()) {
        return rsx! { div { class: "term", style: "display: none" } };
    }
    let toggle = move || {
        let mut t = s.term_open;
        let on = !*t.peek();
        t.set(on);
        crate::app::save_setting("terminal", on.into());
        if on {
            crate::focus::focus("term-input");
        }
    };
    rsx! {
        div { class: if open { "term open {d}" } else { "term {d}" }, style: "{term_box(s)}",
            div { class: "grip", onmousedown: move |e| { e.stop_propagation(); if open { let mut g = s.drag; g.set(true); } } }
            div {
                class: "thead",
                onclick: move |_| if bottom { toggle() },
                // at a side the header is the window's top edge: it drags the window
                onmousedown: move |_| if !bottom { crate::win::drag() },
                div { class: "thl", onclick: move |e| e.stop_propagation(), CatButton { s } VoiceButton { s, send } }
                div { class: "where",
                    // the chat's name: a click lists the project's chats (sessions.rs)
                    button { class: "name chat", title: "Chats",
                        onclick: move |e| { e.stop_propagation(); let mut o = s.sessions_open; let on = !*o.peek(); o.set(on); },
                        "{chat}" span { class: "caret-d", "▾" }
                    }
                    // the agent's work shows in the transcript, under the person's message; the header says only that it speaks
                    if speaking {
                        span { class: "state", "speaking" }
                    }
                }
                div { class: "thr", onclick: move |e| e.stop_propagation(),
                    if busy {
                        button { class: "stop", title: "Stop (⌃C)", onclick: move |_| send.call(Action::Stop), span {} }
                    }
                    // the space's two tabs: its terminal and its files (the ⋯ menu went: the cat and a right-click open it)
                    SpaceTabs { s }
                    if bottom {
                        button { class: if open { "chev open" } else { "chev" }, title: "Terminal (⌃`)", onclick: move |_| toggle(), "⌃" }
                    }
                }
            }
            // keyed by the object: a switch mounts the pane afresh (diffing one object's nested file lists into another's
            // panicked dioxus-core 0.8 alpha finding an anchor)
            if open && (s.files_tab)() {
                FilesPane { key: "{(s.space)().map(|sp| sp.slug).unwrap_or_default()}", s, send }
            }
            if open && !(s.files_tab)() {
                div {
                    class: "tbody",
                    oncontextmenu: move |e| { e.prevent_default(); e.stop_propagation(); let p = e.client_coordinates(); send.call(Action::Menu(p.x, p.y, crate::app::MenuFor::Terminal)); },
                    // the native chrome sends no onresize: its size is measured (app.rs, term_size)
                    onmounted: move |e| crate::focus::keep("tbody", e),
                    // a click types (the keys back from the stage to the prompt); the screen holds still while the button is down
                    onclick: move |_| crate::focus::focus("term-input"),
                    onmousedown: move |_| { let mut sel = s.selecting; sel.set(true); let mut k = s.stage_keys; k.set(false); },
                    onmouseup: move |_| { let mut sel = s.selecting; sel.set(false); },
                    onresize: move |e| {
                        if let Ok(size) = e.get_content_box_size() {
                            let mut t = s.tsize;
                            if (size.width, size.height) != *t.peek() { t.set((size.width, size.height)); }
                        }
                    },
                    // a trackpad sends a few pixels at a time: they add up to lines (18 px each) instead of rounding to none
                    onwheel: move |e| {
                        let mut acc = wheel_acc;
                        let total = *acc.peek() + e.delta().strip_units().y;
                        let lines = (total / 18.0).trunc() as i32;
                        acc.set(total - lines as f64 * 18.0);
                        if lines != 0 {
                            let mut sc = s.scroll;
                            let now = *sc.peek();
                            sc.set((now - lines).max(0));
                        }
                    },
                    div { class: "tscreen",
                        // forty cells of the terminal's font, unseen, for its cell width (focus.rs measure_cell)
                        span { class: "tmeasure", onmounted: move |e| crate::focus::keep("tmeasure", e), "0123456789012345678901234567890123456789" }
                        for (i, row) in (s.screen)().into_iter().enumerate() {
                            div { key: "{i}", class: "trow",
                                for r in row {
                                    span { style: "{run_style(&r)}", "{r.text}" }
                                }
                            }
                        }
                    }
                    // typing: a hidden input (paste, accents and dictation work), mirrored on the prompt line
                    input {
                        id: "term-input",
                        key: "{crate::focus::epoch(\"term-input\")}",
                        onmounted: move |e| { crate::focus::keep("term-input", e); crate::focus::focus("term-input"); },
                        class: "ghost",
                        value: "{draft}",
                        autofocus: true,
                        spellcheck: false,
                        autocomplete: "off",
                        oninput: move |e| { draft.set(e.value()); let mut sel = s.selecting; if *sel.peek() { sel.set(false); } },
                        onkeydown: move |e: KeyboardEvent| {
                            let ctrl = e.modifiers().ctrl();
                            match e.key() {
                                Key::Enter => {
                                    e.prevent_default();
                                    let line = draft.peek().trim().to_string();
                                    if !line.is_empty() {
                                        send.call(Action::Submit(line, false));
                                        draft.set(String::new());
                                    }
                                }
                                Key::ArrowUp => { e.prevent_default(); send.call(Action::History(true)); }
                                Key::ArrowDown => { e.prevent_default(); send.call(Action::History(false)); }
                                Key::Tab => {
                                    e.prevent_default();
                                    let first = intent::suggest(&draft.peek()).first().map(|c| c.0.clone());
                                    if let Some(cmd) = first { draft.set(format!("{cmd} ")); }
                                }
                                Key::Character(c) if ctrl && c == "c" => { e.prevent_default(); send.call(Action::Stop); }
                                Key::Character(c) if ctrl && c == "l" => { e.prevent_default(); send.call(Action::Clear); }
                                Key::Character(c) if ctrl && c == "u" => { e.prevent_default(); draft.set(String::new()); }
                                _ => {}
                            }
                        },
                    }
                }
            }
        }
    }
}

#[component]
pub fn VoicePanel(s: S, send: EventHandler<Action>) -> Element {
    if !(s.voice_open)() {
        return rsx! { div { class: "slot", style: "display: none" } };
    }
    let current = (s.voice)();
    rsx! {
        div { class: "scrim", onclick: move |_| { let mut v = s.voice_open; v.set(false); },
            div { class: "sheet voice", onclick: move |e| e.stop_propagation(),
                div { class: "sheet-head", span { "Voice" } }
                div { class: "toggle-row",
                    span { "Listen" }
                    button {
                        class: if (s.listening)() { "switch on" } else { "switch" },
                        onclick: move |_| send.call(Action::Listen(!(s.listening)())),
                        span { class: "knob" }
                    }
                }
                div { class: "voices",
                    for v in Pocket::voices() {
                        button {
                            key: "{v}",
                            class: if current == v { "chip on" } else { "chip" },
                            onclick: {
                                let v = v.clone();
                                move |_| send.call(Action::SetVoice(v.clone()))
                            },
                            "{v}"
                        }
                    }
                }
                div { class: "fine",
                    p { "Silero VAD · Smart Turn v3 · Parakeet v3 · Pocket TTS, on ONNX Runtime" }
                    p { if (s.voice_ready)() { "Voice ready" } else { "Voice loading" } " · " {if (s.status)().is_empty() { "idle".to_string() } else { (s.status)() }} }
                    if !(s.brain_label)().is_empty() { p { "Brain · {s.brain_label}" } }
                }
            }
        }
    }
}

#[component]
pub fn ContextMenu(s: S, send: EventHandler<Action>) -> Element {
    let Some((x, y)) = (s.menu)() else { return rsx! { div { class: "slot", style: "display: none" } } };
    let close = move || { let mut m = s.menu; m.set(None); };
    let what = (s.menu_for)();
    let folder = (s.space)().map(|sp| sp.folder());
    let live = (s.native_on)();
    rsx! {
        div { class: "sheet menu", style: "left: {x}px; top: {y}px", onclick: move |e| e.stop_propagation(),
            match what {
                // the conversation and the prompt
                crate::app::MenuFor::Terminal => rsx! {
                    div { class: "label", "Terminal" }
                    // the terminal's text isn't selectable on the engine's chrome yet: Copy takes void's last reply
                    div { class: "item", onclick: move |_| { let last = s.lines.peek().iter().rev().find(|l| matches!(l.who, crate::app::Who::Void)).map(|l| l.text.clone()); if let Some(t) = last { copy(&t); } close(); }, "Copy" span { class: "key", "⌘C" } }
                    div { class: "item", onclick: move |_| { let mut d = s.draft; let t = paste(); if !t.is_empty() { let now = d.peek().clone(); d.set(format!("{now}{t}")); } crate::focus::focus("term-input"); close(); }, "Paste into the prompt" span { class: "key", "⌘V" } }
                    if (s.thinking)() {
                        div { class: "item", onclick: move |_| { send.call(Action::Stop); close(); }, "Stop void" span { class: "key", "⌃C" } }
                    }
                    div { class: "sep" }
                    div { class: "item", onclick: move |_| { send.call(Action::Clear); close(); }, "Clear the screen" span { class: "key", "⌃L" } }
                    div { class: "item", onclick: move |_| { send.call(Action::NewSession); close(); }, "New chat" }
                    div { class: "item", onclick: move |_| { let mut o = s.sessions_open; o.set(true); close(); }, "Chats…" }
                    div { class: "item", onclick: move |_| { let (mut f, mut t) = (s.files_tab, s.term_open); crate::app::refresh_assets(s); f.set(true); t.set(true); close(); }, "Show files" }
                    div { class: "sep" }
                    div { class: "label", "Dock the terminal" }
                    div { class: "seg",
                        for l in ["bottom", "left", "right"] {
                            button {
                                key: "{l}",
                                class: if (s.layout)() == l { "on" } else { "" },
                                title: "Terminal at the {l}",
                                onclick: move |_| { send.call(Action::Layout(l.to_string())); close(); },
                                LayoutIcon { which: l }
                            }
                        }
                    }
                },
                // one of the object's files
                crate::app::MenuFor::File(rel, path) => {
                    let (r1, r2, r3) = (rel.clone(), rel.clone(), rel.clone());
                    let (p1, p2, p3) = (path.clone(), path.clone(), path.clone());
                    rsx! {
                        div { class: "label", "{rel}" }
                        div { class: "item", onclick: move |_| { let _ = std::process::Command::new("open").arg(&p1).spawn(); close(); }, "Open" }
                        div { class: "item", onclick: move |_| { crate::assets::reveal(&p2); close(); }, "Show in Finder" }
                        div { class: "item", onclick: move |_| { copy(&p3.to_string_lossy()); close(); }, "Copy its path" }
                        div { class: "sep" }
                        div { class: "item", onclick: move |_| { let mut d = s.draft; let now = d.peek().clone(); d.set(format!("{}{r1} ", if now.is_empty() || now.ends_with(' ') { now.clone() } else { format!("{now} ") })); let (mut f, mut t) = (s.files_tab, s.term_open); f.set(false); t.set(true); crate::focus::focus("term-input"); close(); }, "Mention it in the chat" }
                        div { class: "item", onclick: move |_| { send.call(Action::Submit(format!("look at {r2} and tell me what's in it"), false)); close(); }, "Ask void to look at it" }
                        div { class: "sep" }
                        div { class: "item danger", onclick: move |_| { send.call(Action::RemoveAsset(r3.clone())); close(); }, "Remove from the object" }
                    }
                },
                // the object's files as a whole
                crate::app::MenuFor::Files => {
                    let (f1, f2) = (folder.clone(), folder.clone());
                    rsx! {
                        div { class: "label", "Files" }
                        div { class: "item", onclick: move |_| { spawn(async move { if let Some(p) = pick_files().await { send.call(Action::Import(p)); } }); close(); }, "Bring in files…" }
                        div { class: "item", onclick: move |_| { if let Some(f) = f1.clone() { let media = f.join("media"); crate::assets::reveal(if media.exists() { &media } else { &f }); } close(); }, "Show the folder in Finder" }
                        div { class: "item", onclick: move |_| { crate::app::refresh_assets(s); close(); }, "Refresh" }
                        div { class: "sep" }
                        div { class: "item", onclick: move |_| { if let Some(f) = f2.clone() { copy(&f.to_string_lossy()); } close(); }, "Copy the object's path" }
                    }
                },
                // the object playing on the stage
                crate::app::MenuFor::Stage => {
                    let f1 = folder.clone();
                    let playing = (s.playing)();
                    rsx! {
                        div { class: "label", {(s.space)().map(|sp| sp.name).unwrap_or("Stage".into())} }
                        div { class: "item", onclick: move |_| { if (s.native_on)() { crate::stage_native::control(if playing { crate::stage_native::Control::Pause } else { crate::stage_native::Control::Play }); } send.call(Action::Play(!playing)); close(); }, if playing { "Pause" } else { "Play" } }
                        div { class: "item", onclick: move |_| { if (s.native_on)() { crate::stage_native::control(crate::stage_native::Control::Seek(0)); } send.call(Action::Scrub(0)); close(); }, "Start over" }
                        if live {
                            div { class: "item", onclick: move |_| { let mut k = s.stage_keys; let on = !*k.peek(); k.set(on); close(); },
                                if (s.stage_keys)() { "Give the keys back to the terminal" } else { "Give the keys to the stage" }
                            }
                        }
                        if (s.native_on)() {
                            div { class: "item", onclick: move |_| { crate::stage_native::control(crate::stage_native::Control::Fullscreen(true)); close(); }, "Full screen" }
                        }
                        div { class: "sep" }
                        div { class: "item", onclick: move |_| { let (mut b, mut l) = (s.box_open, s.spaces); l.set(crate::spaces::list()); crate::app::refresh_objects(s); b.set(true); close(); }, "Versions and objects…" }
                        div { class: "item", onclick: move |_| { if let Some(f) = f1.clone() { crate::assets::reveal(&f); } close(); }, "Show the object's folder" }
                    }
                },
                // the window: the app's settings
                crate::app::MenuFor::Window => {
                    let autohide = (s.autohide)();
                    let listening = (s.listening)();
                    rsx! {
                        div { class: "item", onclick: move |_| { send.call(Action::Listen(!listening)); close(); },
                            if listening { "Stop listening" } else { "Start listening" }
                        }
                        div { class: "item", onclick: move |_| { let mut v = s.voice_open; v.set(true); close(); }, "Voice…" }
                        div { class: "sep" }
                        div { class: "item", onclick: move |_| { send.call(Action::Submit("/dock".into(), false)); close(); },
                            if autohide { "Keep composer visible" } else { "Hide composer automatically" }
                        }
                        div { class: "item", onclick: move |_| { let (mut b, mut l) = (s.box_open, s.spaces); l.set(crate::spaces::list()); b.set(true); close(); }, "Objects…" }
                        div { class: "item", onclick: move |_| { let mut c = s.chooser; c.set(true); close(); }, "Switch project…" }
                        div { class: "sep" }
                        div { class: "item", onclick: move |_| { send.call(Action::Submit("/clear".into(), false)); close(); }, "Clear conversation" }
                        div { class: "sep" }
                        div { class: "label", "Terminal" }
                        div { class: "seg",
                            for l in ["bottom", "left", "right"] {
                                button {
                                    key: "{l}",
                                    class: if (s.layout)() == l { "on" } else { "" },
                                    title: "Terminal at the {l}",
                                    onclick: move |_| { send.call(Action::Layout(l.to_string())); close(); },
                                    LayoutIcon { which: l }
                                }
                            }
                        }
                    }
                },
            }
        }
    }
}

/// Text onto the clipboard (pbcopy), and off it (pbpaste): the menus' copy and paste.
fn copy(text: &str) {
    use std::io::Write;
    if let Ok(mut c) = std::process::Command::new("pbcopy").stdin(std::process::Stdio::piped()).spawn() {
        if let Some(i) = c.stdin.as_mut() { let _ = i.write_all(text.as_bytes()); }
        let _ = c.wait();
    }
}
fn paste() -> String {
    std::process::Command::new("pbpaste").output().map(|o| String::from_utf8_lossy(&o.stdout).into_owned()).unwrap_or_default()
}

/// Files picked in the system's dialog (several at once), for "Bring in files…".
async fn pick_files() -> Option<Vec<std::path::PathBuf>> {
    let script = "set fs to choose file with prompt \"Bring files into this object\" with multiple selections allowed\nset out to \"\"\nrepeat with f in fs\nset out to out & POSIX path of f & linefeed\nend repeat\nout";
    let o = tokio::process::Command::new("osascript").arg("-e").arg(script).output().await.ok()?;
    if !o.status.success() { return None; }
    let paths: Vec<std::path::PathBuf> = String::from_utf8_lossy(&o.stdout).lines().filter(|l| !l.is_empty()).map(std::path::PathBuf::from).collect();
    (!paths.is_empty()).then_some(paths)
}

/// The space's pane has two tabs, Terminal and Files, in its header. A narrow pane shows them as icons, a wide one
/// with their names too (a container query on the pane, void.css). At the bottom, a tab opens a folded pane.
#[component]
pub fn SpaceTabs(s: S) -> Element {
    let files = (s.files_tab)();
    let pick = move |want: bool| {
        let (mut f, mut t) = (s.files_tab, s.term_open);
        if want { crate::app::refresh_assets(s); }
        f.set(want);
        crate::app::save_setting("tab", if want { "files" } else { "terminal" }.into());
        if !*t.peek() {
            t.set(true);
            crate::app::save_setting("terminal", true.into());
        }
        if !want { crate::focus::focus("term-input"); }
    };
    rsx! {
        div { class: "tabs",
            button {
                class: if files { "tab" } else { "tab on" },
                title: "Terminal",
                onclick: move |_| pick(false),
                svg { width: "15", height: "15", view_box: "0 0 24 24", fill: "none", stroke: "currentColor", stroke_width: "1.6", stroke_linecap: "round", stroke_linejoin: "round",
                    rect { x: "3", y: "4.5", width: "18", height: "15", rx: "2.5" }
                    path { d: "m7.5 9.5 3 2.5-3 2.5" }
                    path { d: "M12.5 15h4" }
                }
                span { class: "lbl", "Terminal" }
            }
            button {
                class: if files { "tab on" } else { "tab" },
                title: "Files",
                onclick: move |_| pick(true),
                svg { width: "15", height: "15", view_box: "0 0 24 24", fill: "none", stroke: "currentColor", stroke_width: "1.6", stroke_linecap: "round", stroke_linejoin: "round",
                    path { d: "M3 7.5A2.5 2.5 0 0 1 5.5 5h3.6l2 2.2h7.4A2.5 2.5 0 0 1 21 9.7v7.8a2.5 2.5 0 0 1-2.5 2.5h-13A2.5 2.5 0 0 1 3 17.5Z" }
                }
                span { class: "lbl", "Files" }
            }
        }
    }
}

/// The paths a drop or a file picker handed over.
fn picked(files: Vec<dioxus::html::FileData>) -> Vec<std::path::PathBuf> {
    files.iter().map(|f| f.path()).filter(|p| p.is_absolute()).collect()
}

/// One file's line under its thumbnail: its length (video, audio), size, linked or missing, transcribed.
fn asset_meta(a: &crate::assets::Asset, secs: Option<f64>) -> String {
    let mut meta = Vec::new();
    if let Some(t) = secs { meta.push(crate::assets::length_label(t)); }
    if a.broken { meta.push("missing".into()); } else {
        meta.push(crate::assets::size_label(a.bytes));
        if a.linked { meta.push("linked".into()); }
    }
    if a.words { meta.push("transcribed".into()); }
    meta.join(" · ")
}

/// One file in the Files tab: thumbnail (Quick Look through void://, the extension under it), name, a second line
/// (its details, or why a search found it), show in Finder, and remove (× then Remove; a link goes, never its target).
#[component]
fn AssetRow(a: crate::assets::Asset, path: std::path::PathBuf, line: String, i: usize, confirm: Signal<Option<String>>, send: EventHandler<Action>) -> Element {
    let ext = a.name.rsplit_once('.').map(|e| e.1.to_uppercase()).unwrap_or_default();
    let armed = confirm().as_deref() == Some(a.rel.as_str());
    let (rel, rel2, reveal_path, reveal2) = (a.rel.clone(), a.rel.clone(), path.clone(), path.clone());
    let mut confirm = confirm;
    rsx! {
        div {
            class: "row",
            class: if a.broken { "broken" },
            style: "--i: {i}",
            title: "{a.rel}",
            ondoubleclick: move |_| crate::assets::reveal(&reveal_path),
            oncontextmenu: {
                let (rel, path) = (a.rel.clone(), path.clone());
                move |e: MouseEvent| { e.prevent_default(); e.stop_propagation(); let p = e.client_coordinates(); send.call(Action::Menu(p.x, p.y, crate::app::MenuFor::File(rel.clone(), path.clone()))); }
            },
            div { class: "thumb",
                span { class: "ext", "{ext}" }
                if !a.broken {
                    img { src: "{crate::assets::thumb_url(&path)}", loading: "lazy", draggable: false }
                }
            }
            div { class: "text",
                span { class: "name", "{a.name}" }
                span { class: "meta", "{line}" }
            }
            button {
                class: "act",
                title: "Show in Finder",
                onclick: move |e: MouseEvent| { e.stop_propagation(); crate::assets::reveal(&reveal2) },
                svg { width: "13", height: "13", view_box: "0 0 24 24", fill: "none", stroke: "currentColor", stroke_width: "1.6", stroke_linecap: "round", stroke_linejoin: "round",
                    path { d: "M14 4h6v6" }
                    path { d: "M20 4 11 13" }
                    path { d: "M18 14v5a1 1 0 0 1-1 1H5a1 1 0 0 1-1-1V7a1 1 0 0 1 1-1h5" }
                }
            }
            button {
                class: "del",
                class: if armed { "armed" },
                title: if a.linked { "Remove the link (the file it points to stays)" } else { "Remove from the space" },
                onclick: move |e: MouseEvent| {
                    e.stop_propagation();
                    if confirm.peek().as_deref() == Some(rel.as_str()) {
                        send.call(Action::RemoveAsset(rel.clone()));
                        confirm.set(None);
                    } else {
                        confirm.set(Some(rel2.clone()));
                    }
                },
                if armed { "Remove" } else { "×" }
            }
        }
    }
}

/// The Files tab: the open space's files by kind (video, images, audio, 3D, documents, other), and a search over them
/// by meaning (assets.rs: what's in the pictures, what's said, what documents say, names). + (or a drop anywhere on
/// the window) brings files in through the import; video and audio are transcribed as `import_media` does.
#[component]
pub fn FilesPane(s: S, send: EventHandler<Action>) -> Element {
    let confirm = use_signal(|| None::<String>);
    let mut query = use_signal(String::new);
    let mut hits = use_signal(|| None::<Vec<crate::assets::Hit>>);
    let mut searching = use_signal(|| false);
    let mut asked = use_signal(|| 0u64);
    let space = (s.space)();
    let folder = space.as_ref().map(|sp| sp.folder());
    let all = s.assets.read().clone();
    let busy = (s.importing)();
    let secs = s.asset_secs.read().clone();
    let secs_of = |p: &std::path::Path| secs.get(&p.to_string_lossy().into_owned()).copied();
    // typing searches: words at once, then by meaning a moment after the last key (each model takes ~1 s to start)
    let run = {
        let folder = folder.clone();
        move |q: String| {
            let n = *asked.peek() + 1;
            asked.set(n);
            let Some(folder) = folder.clone() else { return };
            if q.trim().is_empty() {
                hits.set(None);
                searching.set(false);
                return;
            }
            spawn(async move {
                let (f, q2) = (folder.clone(), q.clone());
                if let Ok(h) = tokio::task::spawn_blocking(move || crate::assets::search(&f, &q2, false)).await {
                    if *asked.peek() == n { hits.set(Some(h)); }
                }
                tokio::time::sleep(std::time::Duration::from_millis(350)).await;
                if *asked.peek() != n { return; }
                searching.set(true);
                let h = tokio::task::spawn_blocking(move || {
                    let _ = crate::assets::reindex(&folder);
                    crate::assets::search(&folder, &q, true)
                }).await;
                if *asked.peek() == n {
                    if let Ok(h) = h { hits.set(Some(h)); }
                    searching.set(false);
                }
            });
        }
    };
    let shown_hits = hits();
    rsx! {
        div { class: "files",
            oncontextmenu: move |e| { e.prevent_default(); e.stop_propagation(); let p = e.client_coordinates(); send.call(Action::Menu(p.x, p.y, crate::app::MenuFor::Files)); },
            onclick: move |e| e.stop_propagation(),
            div { class: "fhead",
                div { class: "search",
                    svg { width: "13", height: "13", view_box: "0 0 24 24", fill: "none", stroke: "currentColor", stroke_width: "1.8", stroke_linecap: "round",
                        circle { cx: "11", cy: "11", r: "6.5" }
                        path { d: "m16 16 4.5 4.5" }
                    }
                    input {
                        id: "files-search",
                        placeholder: if space.is_some() { "Search by what's in them, what's said, or a name" } else { "Open a space first" },
                        value: "{query}",
                        spellcheck: false,
                        autocomplete: "off",
                        disabled: space.is_none(),
                        oninput: {
                            let mut run = run.clone();
                            move |e: FormEvent| { query.set(e.value()); run(e.value()); }
                        },
                        onkeydown: {
                            let mut run = run.clone();
                            move |e: KeyboardEvent| if e.key() == Key::Escape { e.stop_propagation(); query.set(String::new()); run(String::new()); }
                        },
                    }
                    if searching() { span { class: "spin" } }
                }
                span { class: "count", "{all.len()}" }
                if space.is_some() {
                    // a file input in a label: Dioxus answers its click with the native picker, several files at once
                    label { class: "plus", title: "Add files…",
                        "+"
                        input {
                            r#type: "file",
                            multiple: true,
                            class: "ghost",
                            onchange: move |e: FormEvent| {
                                let paths = picked(e.files());
                                if !paths.is_empty() { send.call(Action::Import(paths)); }
                            },
                        }
                    }
                }
            }
            div { class: "list",
                if space.is_none() {
                    div { class: "hint", "Open a space to bring files into it." }
                } else if all.is_empty() && busy.is_empty() {
                    div { class: "hint",
                        p { "Nothing here yet." }
                        p { "Drop images, video, audio, 3D models or documents anywhere on the window, or press +. They stay on this Mac." }
                    }
                }
                for (i, (name, state)) in busy.iter().enumerate() {
                    div { key: "busy-{name}", class: "row busy", style: "--i: {i}",
                        div { class: "thumb", span { class: "spin" } }
                        div { class: "text",
                            span { class: "name", "{name}" }
                            span { class: "meta", "{state}" }
                        }
                    }
                }
                // each arm starts with a node of its own: an arm of loops alone renders nothing for an object with no
                // files, and dioxus-core 0.8 alpha panics finding its anchor (switching objects with Files open)
                match shown_hits {
                    Some(found) => rsx! {
                        div { class: "slot", style: "display: none" }
                        if found.is_empty() && !searching() {
                            div { class: "hint", "Nothing matches “{query}”." }
                        }
                        for (i, h) in found.into_iter().enumerate() {
                            if let (Some(a), Some(f)) = (all.iter().find(|a| a.rel == h.rel).cloned(), folder.as_ref()) {
                                AssetRow { key: "hit-{h.rel}", path: f.join(&a.rel), a, line: format!("{:.0}% · {}", h.score * 100.0, h.why), i, confirm, send }
                            }
                        }
                    },
                    None => rsx! {
                        div { class: "slot", style: "display: none" }
                        for kind in crate::assets::Kind::ALL {
                            if all.iter().any(|a| a.kind == kind) {
                                div { key: "group-{kind.label()}", class: "group",
                                    span { "{kind.label()}" }
                                    span { class: "dir", "{kind.dir()}/" }
                                }
                                for (i, a) in all.iter().filter(|a| a.kind == kind).cloned().enumerate() {
                                    if let Some(f) = folder.as_ref() {
                                        AssetRow { key: "{a.rel}", path: f.join(&a.rel), line: asset_meta(&a, secs_of(&f.join(&a.rel))), a, i, confirm, send }
                                    }
                                }
                            }
                        }
                    },
                }
            }
        }
    }
}

/// While files are dragged over the window: a veil saying where they'll go. It takes the drop (the window's own
/// dragover shows it).
#[component]
pub fn DropZone(s: S, send: EventHandler<Action>) -> Element {
    if !(s.dropping)() {
        return rsx! { div { class: "slot", style: "display: none" } };
    }
    let name = (s.space)().map(|sp| sp.name);
    rsx! {
        div {
            class: "dropzone",
            ondragover: move |e| e.prevent_default(),
            ondragleave: move |_| { let mut d = s.dropping; d.set(false); },
            ondrop: move |e: DragEvent| {
                e.prevent_default();
                let mut d = s.dropping;
                d.set(false);
                let paths = picked(e.files());
                if !paths.is_empty() { send.call(Action::Import(paths)); }
            },
            div { class: "card",
                match name {
                    Some(n) => rsx! { p { class: "big", "Drop to add to {n}" } p { "Video, images and audio go to media/, 3D to models/, documents to docs/" } },
                    None => rsx! { p { class: "big", "Open a space first" } p { "Files are brought into the open space" } },
                }
            }
        }
    }
}
