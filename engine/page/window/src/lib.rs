//! raea's desktop window around a page (raea-page-host --window, the window raea.app runs): its chrome (chrome.rs) and
//! the native calls a page makes through host calls (native.rs).

pub mod badge;
pub mod chrome;
#[cfg(feature = "window")]
pub mod idle;
pub mod native;
pub mod scheme;
#[cfg(all(windows, feature = "window"))]
mod win;

use bevy::prelude::*;

/// The window's side of the app: the native calls (live only on screen, else recorded), the chrome it was opened with
/// and, on a Mac window with --chrome inset, its unified toolbar. `window.drag` and `window.zoom` are installed with
/// the page (chrome::install, at mount).
pub struct DesktopPlugin {
    pub chrome: chrome::Chrome,
    pub live: bool,
}

impl Plugin for DesktopPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(native::NativePlugin { live: self.live }).insert_resource(chrome::ChromeIs(self.chrome));
        #[cfg(all(target_os = "macos", feature = "window"))]
        if self.live {
            app.add_systems(Update, chrome::toolbar);
        }
    }
}
