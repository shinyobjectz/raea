//! Elements the app reaches by name: each registers its mounted handle when it mounts, and focus and measurement go
//! through Dioxus native's document (an autofocus re-mount, the node's bounding rect). No JavaScript: void's chrome is
//! drawn by the RAEA engine (WebKit deleted 2026-10-17).

use std::collections::HashMap;
use std::rc::Rc;

use dioxus::prelude::*;

static MOUNTED: GlobalSignal<HashMap<&'static str, Rc<MountedData>>> = Signal::global(HashMap::new);
/// Focus asked of an element, by name: on the native chrome the element is mounted again with `autofocus` (Blitz focuses
/// an autofocus element as it's inserted; its set_focus borrows the document mutably, which a task can't), keyed by this.
static ASKED: GlobalSignal<HashMap<&'static str, u64>> = Signal::global(HashMap::new);

/// The key a focusable element carries, so a focus request mounts it again (native chrome; the webview never changes it).
pub fn epoch(name: &'static str) -> u64 {
    ASKED.read().get(name).copied().unwrap_or(0)
}

/// The terminal font's cell, in points, as measured (the webview's SF Mono at 12.5 px is 7.53 wide; the native chrome's
/// fallback font isn't), and its line height.
static CELL: std::sync::Mutex<(f64, f64)> = std::sync::Mutex::new((7.53, 18.0));
pub fn cell() -> (f64, f64) {
    *CELL.lock().unwrap()
}
/// The cell measured from a run of 40 characters in the terminal's font ("tmeasure").
pub async fn measure_cell() {
    if let Some((_, _, w, h)) = rect("tmeasure").await {
        if w > 40.0 && h > 4.0 {
            *CELL.lock().unwrap() = (w / 40.0, (h * 1.44).max(h));
        }
    }
}

/// An element mounted: kept under its name (the terminal's input, the native stage's box, a sheet's key catcher).
pub fn keep(name: &'static str, e: MountedEvent) {
    MOUNTED.write().insert(name, e.data());
}

/// Keyboard focus to a kept element, if it's mounted. (The native chrome takes focus from `autofocus` and clicks for
/// now: its set_focus borrows the document mutably, which a task polled inside the document's own update can't.)
pub fn focus(name: &'static str) {
    let next = ASKED.peek().get(name).copied().unwrap_or(0) + 1;
    ASKED.write().insert(name, next);
}

/// A kept element's box in the window's content, in points: (x, y, width, height). Read through a shared borrow
/// (try_doc), so a task never collides with the document's update.
pub async fn rect(name: &'static str) -> Option<(f64, f64, f64, f64)> {
    let m = MOUNTED.peek().get(name).cloned()?;
    let h = m.downcast::<dioxus_native::NodeHandle>()?;
    let doc = h.try_doc()?;
    let r = doc.get_client_bounding_rect(h.node_id())?;
    Some((r.x, r.y, r.width, r.height))
}
