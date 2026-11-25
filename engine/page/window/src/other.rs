//! Off AppKit. Linux's and Windows' window (feature "window" off the Mac) opens links and uses the clipboard through
//! raea-io-os, as the page host's text fields already paste: opener (xdg-open, or opener's own copy of it where the
//! system has none; Windows' shell) and arboard. Until 11-23 these were stubs everywhere but the Mac, so on Linux and
//! Windows every link the app opened (signing in's among them) and every Copy did nothing and answered that it had.
//! Without a window (the benches, CI) Live is false and none of these run. Notifications, the share sheet, the Dock
//! badge and raea:// links are still the Mac's alone (Windows' taskbar overlay is win.rs).

use super::Answer;
#[cfg(feature = "window")]
pub fn clipboard_write(t: &str) -> Result<(), String> {
    raea_io_os::set_clipboard_text(t.to_string())
}
#[cfg(feature = "window")]
pub fn clipboard_read() -> String {
    raea_io_os::clipboard_text().unwrap_or_default()
}
#[cfg(feature = "window")]
pub use raea_io_os::open_url as open;
#[cfg(not(feature = "window"))]
pub fn clipboard_write(text: &str) -> Result<(), String> {
    // nothing to write to without a window; answered as written, as before
    let _unwritten = text.len();
    Ok(())
}
#[cfg(not(feature = "window"))]
pub fn clipboard_read() -> String {
    String::new()
}
#[cfg(not(feature = "window"))]
pub use self::clipboard_write as open;
pub fn badge(_: &str) {}
pub fn notify(_: &str, _: &str) -> Result<(), String> {
    Err("notifications need raea's window on a Mac".into())
}
pub fn ask_permission(a: Answer) {
    a.err("notifications need raea's window on a Mac")
}
pub fn listen_for_urls() {}
pub fn share(paths: &[String], _: f64, _: f64) -> Result<(), String> {
    Err(format!("sharing {} files needs raea's window on a Mac", paths.len()))
}
