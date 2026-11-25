//! Light or dark: the window's colour scheme, as `@media (prefers-color-scheme)` and matchMedia give it to the page. It
//! follows the system's until the page picks one, with the host call `window.scheme {pick: "system" | "light" | "dark"}`
//! -> {scheme, pick}; a pick also sets the window's own theme, so its title bar and menus match. Headless, light until
//! the page picks.

use bevy::prelude::*;
use bevy::window::{PrimaryWindow, WindowTheme, WindowThemeChanged};
use blitz_traits::shell::ColorScheme;
use raea_runtime_js::HostCalls;
use raea_sys_page::PageScheme;
use serde_json::json;

/// The page's pick (None: the system's), and the system's scheme as the window last said it.
#[derive(Resource, Default)]
pub struct Scheme {
    pick: Option<WindowTheme>,
    system: Option<WindowTheme>,
}

impl Scheme {
    fn now(&self) -> WindowTheme {
        self.pick.or(self.system).unwrap_or(WindowTheme::Light)
    }
}

fn name(t: WindowTheme) -> &'static str {
    match t {
        WindowTheme::Light => "light",
        WindowTheme::Dark => "dark",
    }
}

/// `window.scheme`, on the primary window.
pub fn install(world: &mut World) {
    world.init_resource::<HostCalls>();
    world.init_resource::<Scheme>();
    world.init_resource::<PageScheme>();
    world.resource_mut::<HostCalls>().on("window.scheme", |w, args, answer| {
        let pick = match args.get("pick").and_then(|p| p.as_str()) {
            Some("system") => Some(None),
            Some("light") => Some(Some(WindowTheme::Light)),
            Some("dark") => Some(Some(WindowTheme::Dark)),
            Some(_) => return answer.err("pick system, light or dark"),
            None => None,
        };
        if let Some(pick) = pick
            && w.resource::<Scheme>().pick != pick
        {
            w.resource_mut::<Scheme>().pick = pick;
            let mut q = w.query_filtered::<&mut Window, With<PrimaryWindow>>();
            if let Ok(mut win) = q.single_mut(w) {
                win.window_theme = pick;
            }
        }
        let s = w.resource::<Scheme>();
        let (now, pick) = (name(s.now()), s.pick.map_or("system", name));
        answer.ok(json!({"scheme": now, "pick": pick}))
    });
}

/// The window's theme, first and each time it changes, as the page's scheme.
pub fn follow(mut scheme: ResMut<Scheme>, mut page: ResMut<PageScheme>, win: Query<&Window, With<PrimaryWindow>>, mut evs: MessageReader<WindowThemeChanged>) {
    // the window made: bevy_winit writes the theme it opened in
    if scheme.system.is_none()
        && scheme.pick.is_none()
        && let Some(t) = win.iter().next().and_then(|w| w.window_theme)
    {
        scheme.system = Some(t);
    }
    for e in evs.read() {
        // what a pick makes the window say isn't the system's
        if scheme.pick.is_none() {
            scheme.system = Some(e.theme);
        }
    }
    let now = match scheme.now() {
        WindowTheme::Light => ColorScheme::Light,
        WindowTheme::Dark => ColorScheme::Dark,
    };
    page.set_if_neq(PageScheme(now));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_page_follows_the_windows_theme_until_it_picks_one() {
        let mut app = App::new();
        app.add_message::<WindowThemeChanged>().add_systems(Update, follow);
        install(app.world_mut());
        let win = app.world_mut().spawn((Window { window_theme: Some(WindowTheme::Dark), ..default() }, PrimaryWindow)).id();
        app.update();
        let scheme = |app: &App| app.world().resource::<PageScheme>().0;
        assert_eq!(scheme(&app), ColorScheme::Dark);
        app.world_mut().write_message(WindowThemeChanged { window: win, theme: WindowTheme::Light });
        app.update();
        assert_eq!(scheme(&app), ColorScheme::Light);
        // picked: the system's changes don't reach the page, and what the pick makes the window say isn't the system's
        app.world_mut().resource_mut::<Scheme>().pick = Some(WindowTheme::Dark);
        app.world_mut().write_message(WindowThemeChanged { window: win, theme: WindowTheme::Dark });
        app.update();
        assert_eq!(scheme(&app), ColorScheme::Dark);
        app.world_mut().resource_mut::<Scheme>().pick = None;
        app.update();
        assert_eq!(scheme(&app), ColorScheme::Light);
    }
}
