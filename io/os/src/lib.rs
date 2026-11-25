//! The system's host calls (docs/targets.feature T2d, agreed on 11-06), answered the same on
//! both app targets: runtime/js os.rs and engine/rune js.rs each hand a call here with how to answer.
//!
//!   os.open {url} -> null                 http, https or mailto in the system's handler; any other scheme refused
//!   net.clipboard.read {} -> text         the system clipboard, text only (under net because @raea/ui's platform layer
//!   net.clipboard.write {text} -> null    has called them that since P17: navigator.clipboard)
//!   fs.pick {multiple?, accept?: [ext]} -> [{id, name, size}]     the system's open dialog
//!   fs.save {name, accept?} -> {id} | null                        the save dialog
//!   fs.read {id, binary?} -> text | base64, fs.write {id, data, binary?} -> null
//!                                         only files picked or saved this run, by opaque id, at most 64 MB
//!   dialog.message {title, text, kind: "info" | "warning" | "error", buttons?: [label]} -> the pressed button's index
//!
//! Headless (a host's check, `set_headless`) touches nothing of the system's: the clipboard is the process's own,
//! os.open is recorded (`opened`), and a pick, save or dialog takes the answer a check step queued (`script`) or is
//! refused. Dialogs run on a thread of their own, never the caller's frame.

mod dialog;
mod fs;

use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock};

use serde_json::{Value, json};

/// Every call this crate answers.
pub const CALLS: [&str; 8] = ["os.open", "net.clipboard.read", "net.clipboard.write", "fs.pick", "fs.save", "fs.read", "fs.write", "dialog.message"];

/// A call's answer, once, from any thread.
pub type Done = Box<dyn FnOnce(Result<Value, String>) + Send>;

static HEADLESS: AtomicBool = AtomicBool::new(false);

/// This process is a headless check: nothing reaches the system (see the module's notes).
pub fn set_headless() {
    HEADLESS.store(true, Ordering::Relaxed);
}

/// Also always on iOS, which builds none of the system's side (Cargo.toml).
fn headless() -> bool {
    cfg!(target_os = "ios") || HEADLESS.load(Ordering::Relaxed)
}

/// A check step's answer for the next dialog of its kind.
#[derive(Debug, Clone, PartialEq)]
pub enum Scripted {
    /// the files the next fs.pick picks
    Pick(Vec<PathBuf>),
    /// the file the next fs.save saves to
    Save(PathBuf),
    /// the button the next dialog.message's user presses
    Dialog(usize),
}

fn queue() -> &'static Mutex<VecDeque<Scripted>> {
    static Q: OnceLock<Mutex<VecDeque<Scripted>>> = OnceLock::new();
    Q.get_or_init(Mutex::default)
}

/// Queue `s` for the next pick, save or dialog (a check's step).
pub fn script(s: Scripted) {
    queue().lock().unwrap().push_back(s);
}

/// The next queued answer `pick` takes, if any.
pub(crate) fn scripted<T>(pick: impl Fn(&Scripted) -> Option<T>) -> Option<T> {
    let mut q = queue().lock().unwrap();
    let i = q.iter().position(|s| pick(s).is_some())?;
    q.remove(i).and_then(|s| pick(&s))
}

fn opened_list() -> &'static Mutex<Vec<String>> {
    static O: OnceLock<Mutex<Vec<String>>> = OnceLock::new();
    O.get_or_init(Mutex::default)
}

/// Why `name` will be refused here, when it's a headless pick, save or dialog with no step queued for it: a host
/// lists it with the calls it didn't answer.
pub fn unscripted(name: &str) -> Option<String> {
    let kind = match name {
        "fs.pick" => "pick",
        "fs.save" => "save",
        "dialog.message" => "dialog",
        _ => return None,
    };
    let queued = queue().lock().unwrap().iter().any(|s| matches!((s, kind), (Scripted::Pick(_), "pick") | (Scripted::Save(_), "save") | (Scripted::Dialog(_), "dialog")));
    (headless() && !queued).then(|| format!("{name}: headless, and no {kind} step answers it"))
}

/// The URLs os.open was asked for while headless, in order.
pub fn opened() -> Vec<String> {
    opened_list().lock().unwrap().clone()
}

/// A check step as JSON (`{"pick": [file]}`, `{"save": file}`, `{"dialog": index}`), if it's one of these.
pub fn step(s: &Value) -> Option<Scripted> {
    if let Some(files) = s.get("pick").and_then(Value::as_array) {
        return Some(Scripted::Pick(files.iter().filter_map(Value::as_str).map(PathBuf::from).collect()));
    }
    if let Some(f) = s.get("save").and_then(Value::as_str) {
        return Some(Scripted::Save(PathBuf::from(f)));
    }
    s.get("dialog").and_then(Value::as_u64).map(|i| Scripted::Dialog(i as usize))
}

/// `name` (one of CALLS) with `args`, answered through `done`.
pub fn call(name: &str, args: Value, done: Done) {
    match name {
        "os.open" => done(open(&args)),
        "net.clipboard.read" => done(clipboard(None)),
        "net.clipboard.write" => done(clipboard(Some(args["text"].as_str().unwrap_or("").to_string()))),
        "fs.pick" | "fs.save" | "fs.read" | "fs.write" => fs::call(name, &args, done),
        "dialog.message" => dialog::message(&args, done),
        _ => done(Err(format!("{name} isn't an os call"))),
    }
}

/// `url` (http, https or mailto) in the system's handler, recorded when headless: os.open's own, for a caller with no
/// call to answer. raea's window opens its links through it on Linux and Windows (engine/page/window native.rs app.open).
pub fn open_url(url: &str) -> Result<(), String> {
    open(&json!({ "url": url })).map(|_| ())
}

fn open(args: &Value) -> Result<Value, String> {
    let url = args["url"].as_str().ok_or("os.open: no url")?;
    let scheme = url.split_once(':').map_or("", |(s, _)| s).to_ascii_lowercase();
    if !["http", "https", "mailto"].contains(&scheme.as_str()) {
        return Err(format!("os.open: the {scheme:?} scheme isn't opened (http, https or mailto only)"));
    }
    if headless() {
        opened_list().lock().unwrap().push(url.to_string());
        return Ok(Value::Null);
    }
    #[cfg(not(target_os = "ios"))]
    return opener::open(url).map(|()| Value::Null).map_err(|e| format!("os.open {url}: {e}"));
    #[cfg(target_os = "ios")]
    unreachable!("headless on iOS");
}

/// The clipboard's text ("" when it holds none): the system's, or the process's own when headless. A page's text
/// field pastes from it (engine/page/host shell.rs, Blitz's shell provider).
pub fn clipboard_text() -> Result<String, String> {
    clipboard(None).map(|v| v.as_str().unwrap_or_default().to_string())
}

/// `text` onto the clipboard: the system's, or the process's own when headless.
pub fn set_clipboard_text(text: String) -> Result<(), String> {
    clipboard(Some(text)).map(|_| ())
}

/// The clipboard's text, or `text` written to it: the process's own when headless.
fn clipboard(text: Option<String>) -> Result<Value, String> {
    static OWN: Mutex<String> = Mutex::new(String::new());
    if headless() {
        let mut own = OWN.lock().unwrap();
        return Ok(match text {
            Some(t) => {
                *own = t;
                Value::Null
            }
            None => json!(*own),
        });
    }
    #[cfg(not(target_os = "ios"))]
    return system_clipboard(text);
    #[cfg(target_os = "ios")]
    unreachable!("headless on iOS");
}

#[cfg(not(target_os = "ios"))]
fn system_clipboard(text: Option<String>) -> Result<Value, String> {
    static SYSTEM: Mutex<Option<arboard::Clipboard>> = Mutex::new(None);
    // kept open: on X11 a clipboard's text lives as long as the process that set it holds it
    let mut sys = SYSTEM.lock().unwrap();
    if sys.is_none() {
        *sys = Some(arboard::Clipboard::new().map_err(|e| format!("the clipboard: {e}"))?);
    }
    let c = sys.as_mut().unwrap();
    match text {
        Some(t) => c.set_text(t).map(|()| Value::Null),
        // an empty clipboard reads as "", as a browser's readText does
        None => match c.get_text() {
            Err(arboard::Error::ContentNotAvailable) => Ok(json!("")),
            r => r.map(|t| json!(t)),
        },
    }
    .map_err(|e| format!("the clipboard: {e}"))
}

#[cfg(feature = "recorded")]
pub mod recorded;
