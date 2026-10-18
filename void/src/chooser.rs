//! The workspace chooser: which folder void works in (workspace.rs). It covers the window at launch when there's
//! nothing to reopen, and comes back from the cat's menu (Switch workspace…). The recent workspaces, newest first
//! (the legacy global spaces among them), then Open folder… and New workspace…. ↑ ↓ and return pick a recent one,
//! ⌘O opens a folder, ⌘N makes a new workspace, escape goes back to the open one.

use dioxus::prelude::*;

use crate::app::{Action, S};
use crate::workspace::{self, Recent};

fn ago(t: u64) -> String {
    if t == 0 {
        return "not opened yet".into();
    }
    let d = workspace::now().saturating_sub(t);
    match d {
        0..60 => "just now".into(),
        60..3600 => format!("{}m ago", d / 60),
        3600..86400 => format!("{}h ago", d / 3600),
        86400..172800 => "yesterday".into(),
        _ => format!("{}d ago", d / 86400),
    }
}

/// Ask for a folder, then open it (or make it a new workspace).
fn pick(send: EventHandler<Action>, new: bool) {
    spawn(async move {
        let picked = if new {
            // a save dialog: a name and where to put it (the folder is made), or an existing folder to start in
            workspace::pick_new("Name the new workspace").await
        } else {
            workspace::pick_folder("Open a folder as a workspace").await
        };
        if let Some(p) = picked {
            send.call(Action::OpenWorkspace(p, new));
        }
    });
}

#[component]
pub fn Chooser(s: S, send: EventHandler<Action>) -> Element {
    let mut at = use_signal(|| 0usize);
    if !(s.chooser)() {
        return rsx! { div { class: "slot", style: "display: none" } };
    }
    let list: Vec<Recent> = workspace::choices(&workspace::recent_file(), &workspace::legacy_root());
    let n = list.len();
    let pickd = at().min(n.saturating_sub(1));
    let current = (s.workspace)().map(|w| w.root);
    let can_close = current.is_some();
    let paths: Vec<std::path::PathBuf> = list.iter().map(|r| r.path.clone()).collect();
    rsx! {
        div { class: "chooser",
            // a click anywhere keeps the keys here (the window would otherwise hand typing to the hidden terminal)
            onclick: move |e| {
                e.stop_propagation();
                crate::focus::focus("chooser-keys");
            },
            oncontextmenu: move |e| e.stop_propagation(),
            div { class: "drag", onmousedown: move |_| crate::win::drag() }
            div { class: "chooser-col",

                div { class: "chooser-mark",
                    svg { width: "34", height: "34", view_box: "0 0 24 24", fill: "none", stroke: "currentColor", stroke_width: "1.2", stroke_linejoin: "round", stroke_linecap: "round",
                        polygon { points: "6.156,1 8.219,1 9.594,7.875 11.656,3.062 13.719,3.062 14.406,5.125 19.906,3.75 20.594,13.375 19.219,14.75 21.969,18.875 21.281,20.938 15.094,18.188 13.031,18.875 11.656,23 9.594,23 8.906,18.188 6.844,16.812 4.094,20.938 2.031,20.25 3.406,12 6.156,8.562" }
                        path { d: "M14.75 8.863v1.462" }
                        path { d: "M18.188 8.863v1.462" }
                    }
                }
                p { class: "chooser-title", "Where are we working?" }
                p { class: "chooser-sub", "A project is a folder. Its objects (anything that renders) live in objects/, one folder each." }
                // the keys: arrows move, return opens, ⌘O and ⌘N
                input {
                    id: "chooser-keys",
                    key: "{crate::focus::epoch(\"chooser-keys\")}",
                    autofocus: true,
                    onmounted: move |e| { crate::focus::keep("chooser-keys", e); crate::focus::focus("chooser-keys"); },
                    class: "ghost",
                    autocomplete: "off",
                    spellcheck: false,
                    value: "",
                    onkeydown: {
                        let paths = paths.clone();
                        move |e: KeyboardEvent| {
                            // the window's own keys (escape focuses the terminal, ⌃` toggles it) stay out of the chooser
                            e.stop_propagation();
                            let cmd = e.modifiers().meta();
                            match e.key() {
                                Key::ArrowDown => { e.prevent_default(); if n > 0 { at.set((pickd + 1) % n); } }
                                Key::ArrowUp => { e.prevent_default(); if n > 0 { at.set((pickd + n - 1) % n); } }
                                Key::Enter => { e.prevent_default(); if let Some(p) = paths.get(pickd) { send.call(Action::OpenWorkspace(p.clone(), false)); } }
                                Key::Escape if can_close => { let mut c = s.chooser; c.set(false); }
                                Key::Character(c) if cmd && c == "o" => { e.prevent_default(); pick(send, false); }
                                Key::Character(c) if cmd && c == "n" => { e.prevent_default(); pick(send, true); }
                                _ => {}
                            }
                        }
                    },
                }
                if n > 0 {
                    div { class: "chooser-label", "Recent" }
                    div { class: "chooser-list",
                        for (i, r) in list.into_iter().enumerate() {
                            div {
                                key: "{r.path.display()}",
                                class: if i == pickd { "crow picked" } else { "crow" },
                                class: if current.as_ref() == Some(&r.path) { "current" },
                                style: "--i: {i}",
                                onmouseenter: move |_| at.set(i),
                                onclick: {
                                    let p = r.path.clone();
                                    move |_| send.call(Action::OpenWorkspace(p.clone(), false))
                                },
                                div { class: "cname",
                                    span { "{r.name}" }
                                    if r.name == workspace::LEGACY_NAME { span { class: "tag", "before workspaces" } }
                                    if current.as_ref() == Some(&r.path) { span { class: "tag", "open" } }
                                }
                                div { class: "cpath", "{workspace::tilde(&r.path)}" }
                                span { class: "cmeta", "{ago(r.opened)}" }
                                if r.opened > 0 {
                                    button {
                                        class: "cforget",
                                        title: "Remove from recent (the folder stays)",
                                        onclick: {
                                            let p = r.path.clone();
                                            move |e: MouseEvent| { e.stop_propagation(); send.call(Action::ForgetWorkspace(p.clone())); }
                                        },
                                        "×"
                                    }
                                }
                            }
                        }
                    }
                }
                div { class: "chooser-actions",
                    button { class: "cbtn", onclick: move |_| pick(send, false),
                        span { "Open folder…" } span { class: "key", "⌘O" }
                    }
                    button { class: "cbtn", onclick: move |_| pick(send, true),
                        span { "New workspace…" } span { class: "key", "⌘N" }
                    }
                }
                if can_close {
                    button { class: "chooser-back", onclick: move |_| { let mut c = s.chooser; c.set(false); }, "Back to {(s.workspace)().map(|w| w.meta.name).unwrap_or_default()}  ·  esc" }
                }
            }
        }
    }
}
