//! App screens (`<Screen app="kit.tsx">`, docs/blitz-bevy.feature H12): each one's module, its default export a React
//! component given `{width, height}`, runs on @raea/ui's react-dom in a Blitz document of its own at the screen's
//! size, on a page App of its own (raea-page's BlitzPage), so it is stepped exactly once a frame however often the
//! composition's page is synced or drawn. A step sends the input routed to it as Blitz's pointer, wheel and key
//! events, then runs one update that runs its JS (its frame, timers and commits) with the JS clock held at the
//! frame's time (raea-runtime-jsengine `at_clock`), and paints it. A render and its play step it on the same frames
//! with the same input, so the screen shows the same picture in both. Its picture goes where a nested <Screen>'s
//! goes: its texture on the shared device in play (Engine::copy_screen), read back in a render.
//!
//! An app's own timers and performance.now follow the frames; its Date doesn't (a module reading the wall clock draws
//! another time in a render).

use std::path::{Path, PathBuf};

use bevy::input::ButtonState;
use bevy::input::mouse::{MouseButton, MouseButtonInput, MouseScrollUnit, MouseWheel};
use bevy::input::touch::TouchPhase;
use bevy::prelude::*;
use bevy::render::settings::RenderCreation;
use bevy::window::{CursorLeft, CursorMoved, WindowEvent};
use blitz_traits::events::{BlitzKeyEvent, KeyState, UiEvent};
use keyboard_types::{Code, Key, Location, Modifiers};
use raea_page::motion::BlitzPage;
use raea_runtime_dom::host::{self, WindowInput};

type R<T> = Result<T, String>;

/// What a prebuilt app screen's script is called beside its module (`kit.tsx` -> `kit.tsx.surface.js`): an exported
/// game's, which bundles nothing (apps/game gather.rs). Its CSS is beside it as [`BUNDLED_CSS`].
pub const BUNDLED: &str = ".surface.js";
pub const BUNDLED_CSS: &str = ".surface.css";

/// One frame's input for an app screen: the pointer on its picture (its px) and whether it's down, the keys held
/// for it (while it has the focus), and the wheel's turns over it (CSS px, down positive, as the DOM's deltaY).
#[derive(Default, Clone, Debug, PartialEq)]
pub struct Input {
    pub pointer: Option<(f64, f64, bool)>,
    pub keys: Vec<String>,
    pub wheel: Vec<f64>,
}

/// What reached the app screens in a frame, from live play's routing (runtime/page screen.rs route) or a render's input
/// log of it: the screen with the focus and the keys held for it, the pointer on a screen (its name, x and y on its
/// picture, whether it's down) and the wheel's turns there (`Wheel:Y DY`).
#[derive(Default, Clone, Debug, PartialEq)]
pub struct Routed {
    pub focus: Option<String>,
    pub keys: Vec<String>,
    pub pointer: Option<(String, f64, f64, bool)>,
    pub wheel: Vec<String>,
}

impl Routed {
    /// App screen `name`'s input: the pointer when it's on it (with the wheel turned there), the keys while it has the
    /// focus (a DOM key's name; the live wire's marked keys, "Mouse:Right" or "Pad0:South", aren't keys to it).
    pub fn input_of(&self, name: &str) -> Input {
        let on = self.pointer.as_ref().filter(|p| p.0 == name);
        let focused = self.focus.as_deref() == Some(name);
        Input {
            pointer: on.map(|p| (p.1, p.2, p.3)),
            keys: self.keys.iter().filter(|k| focused && (k.chars().count() == 1 || !k.contains(':'))).cloned().collect(),
            wheel: self.wheel.iter().filter(|_| on.is_some()).filter_map(|w| w.strip_prefix("Wheel:Y ")?.trim().parse().ok()).collect(),
        }
    }

    /// As the input log keeps it (`apps`); None when nothing reached an app screen.
    pub fn logged(&self) -> Option<serde_json::Value> {
        let any = self.focus.is_some() || self.pointer.is_some() || !self.wheel.is_empty();
        any.then(|| serde_json::json!({ "focus": self.focus, "keys": self.keys, "pointer": self.pointer.as_ref().map(|p| serde_json::json!([p.0, p.1, p.2, p.3])), "wheel": self.wheel }))
    }

    /// From a log's entry (`logged`'s); nothing reached them when there's none.
    pub fn from_log(v: Option<&serde_json::Value>) -> Routed {
        let Some(v) = v else { return Routed::default() };
        let texts = |k: &str| v[k].as_array().into_iter().flatten().filter_map(|s| s.as_str().map(String::from)).collect();
        let p = v["pointer"].as_array();
        let pointer = p.and_then(|p| Some((p.first()?.as_str()?.to_string(), p.get(1)?.as_f64()?, p.get(2)?.as_f64()?, p.get(3)?.as_bool()?)));
        Routed { focus: v["focus"].as_str().map(String::from), keys: texts("keys"), pointer, wheel: texts("wheel") }
    }
}

/// Set for the one update a step runs the page's JS in (the draw's later updates, while faces and images load, only
/// paint).
#[derive(Resource, Default)]
struct StepNow(bool);

fn step_once(world: &mut World) {
    if std::mem::take(&mut world.resource_mut::<StepNow>().0) {
        host::step(world);
    }
}

/// An app screen's document, open.
pub struct AppScreen {
    page: BlitzPage,
    root: Entity,
    /// the module, as the composition names it
    pub module: String,
    pub size: (u32, u32),
    window: WindowInput,
    was: Input,
    stepped: Option<i64>,
}

/// Frame `f`'s time in ms at `fps`.
fn ms_at(f: i64, fps: f64) -> f64 {
    f as f64 * 1000.0 / fps.max(1.0)
}

impl AppScreen {
    /// `module` (a path from `dir`, the composition's folder) opened `w` x `h` at frame `f` of `fps`: its script (see
    /// [`code_of`]) mounted on a Blitz page on the device `render` makes (None: Bevy's own), in raea's bundled faces.
    pub fn open(dir: &Path, module: &str, w: u32, h: u32, f: i64, fps: f64, render: Option<RenderCreation>) -> R<AppScreen> {
        let (code, css) = code_of(&dir.join(module))?;
        let fonts = raea_io_home::home().join("core/fonts");
        let mut page = BlitzPage::new_on(w.max(1), h.max(1), Some(&fonts), render)?;
        page.not_dumped();
        let app = page.app();
        app.init_resource::<StepNow>();
        app.add_systems(PreUpdate, step_once.in_set(raea_sys_page::PageSystems::Step));
        let world = app.world_mut();
        world.init_resource::<raea_runtime_js::HostCalls>();
        world.insert_resource(raea_runtime_js::UiFrame(f.max(0) as u64));
        let root = raea_runtime_jsengine::at_clock(ms_at(f, fps), || host::mount(world, w.max(1), h.max(1), &code, &[], Some(raea_sys_fonts::context(&fonts))))?;
        // as raea's page host mounts an app (engine/page/host page.rs)
        host::add_style(world, root, "html, body { height: 100%; }");
        if !css.trim().is_empty() {
            host::add_style(world, root, &css);
        }
        page.show(root);
        Ok(AppScreen { page, root, module: module.to_string(), size: (w, h), window: WindowInput::default(), was: Input::default(), stepped: None })
    }

    /// Stepped to frame `f` of `fps` with `input`: its events, then one update running its JS on the frame's clock, drawn.
    /// Once a frame: stepped again at the same frame, nothing happens.
    pub fn step(&mut self, f: i64, fps: f64, input: &Input) {
        if self.stepped == Some(f) {
            return;
        }
        self.stepped = Some(f);
        let events = self.events(input);
        self.was = input.clone();
        let root = self.root;
        let page = &mut self.page;
        raea_runtime_jsengine::at_clock(ms_at(f, fps), || {
            page.at_frame(f, fps);
            let world = page.app().world_mut();
            world.insert_resource(raea_runtime_js::UiFrame(f.max(0) as u64));
            for (press, ev) in events {
                // each press a click of its own (a double click needs presses closer than a frame apart)
                if press {
                    host::set_clicks(world, root, 1);
                }
                host::input(world, root, ev);
            }
            world.resource_mut::<StepNow>().0 = true;
            page.draw();
        });
    }

    /// Its picture as last drawn: its texture on the page's device (sRGB-encoded premultiplied RGBA, as a nested
    /// <Screen>'s).
    pub fn texture(&self) -> Option<bevy::render::render_resource::Texture> {
        self.page.texture()
    }

    /// Its picture as last drawn, read back: RGBA rows at its size.
    pub fn rgba(&mut self) -> R<Vec<u8>> {
        self.page.frame()
    }

    /// The events that take the document from the last step's input to `now`'s, as Blitz takes them, each marked
    /// when it's a press: the pointer's move, press and release (a release, then a leave, when it goes off the
    /// picture), the wheel, the keys let go and the keys pressed.
    fn events(&mut self, now: &Input) -> Vec<(bool, UiEvent)> {
        let window = Entity::PLACEHOLDER;
        let was = self.was.clone();
        let mut out = Vec::new();
        let mut put = |w: &mut WindowInput, ev: WindowEvent, press: bool| {
            if let Some(e) = w.event(&ev) {
                out.push((press, e));
            }
        };
        let button = |state| WindowEvent::MouseButtonInput(MouseButtonInput { button: MouseButton::Left, state, window });
        let held = was.pointer.is_some_and(|p| p.2);
        match now.pointer {
            Some((x, y, down)) => {
                if was.pointer.is_none_or(|(bx, by, _)| bx != x || by != y) {
                    put(&mut self.window, WindowEvent::CursorMoved(CursorMoved { window, position: Vec2::new(x as f32, y as f32), delta: None }), false);
                }
                if down && !held {
                    put(&mut self.window, button(ButtonState::Pressed), true);
                }
                if !down && held {
                    put(&mut self.window, button(ButtonState::Released), false);
                }
            }
            None if was.pointer.is_some() => {
                if held {
                    put(&mut self.window, button(ButtonState::Released), false);
                }
                put(&mut self.window, WindowEvent::CursorLeft(CursorLeft { window }), false);
            }
            None => {}
        }
        for dy in &now.wheel {
            // Blitz's wheel is Bevy's way up (raea-runtime-dom gives the DOM -y)
            put(&mut self.window, WindowEvent::MouseWheel(MouseWheel { unit: MouseScrollUnit::Pixel, x: 0.0, y: -(*dy as f32), window, phase: TouchPhase::Moved }), false);
        }
        let mods = mods_of(&now.keys);
        for k in was.keys.iter().filter(|k| !now.keys.contains(k)) {
            out.push((false, UiEvent::KeyUp(key_event(k, mods, false))));
        }
        for k in now.keys.iter().filter(|k| !was.keys.contains(k)) {
            out.push((false, UiEvent::KeyDown(key_event(k, mods, true))));
        }
        out
    }
}

/// The modifiers the held keys (by their DOM names) make.
fn mods_of(keys: &[String]) -> Modifiers {
    let mut m = Modifiers::empty();
    for (name, bit) in [("Shift", Modifiers::SHIFT), ("Control", Modifiers::CONTROL), ("Alt", Modifiers::ALT), ("Meta", Modifiers::META)] {
        if keys.iter().any(|k| k == name) {
            m |= bit;
        }
    }
    m
}

/// Key `name` (the DOM's: a character, or a named key like "Enter") pressed or let go: a character types itself
/// unless Control or Meta is held.
fn key_event(name: &str, modifiers: Modifiers, down: bool) -> BlitzKeyEvent {
    let name = if name == "Space" { " " } else { name };
    let key = name.parse::<Key>().unwrap_or_else(|_| Key::Character(name.into()));
    let types = matches!(key, Key::Character(_)) && !modifiers.intersects(Modifiers::CONTROL | Modifiers::META);
    BlitzKeyEvent {
        key,
        code: Code::Unidentified,
        modifiers,
        location: Location::Standard,
        is_auto_repeating: false,
        is_composing: false,
        state: if down { KeyState::Pressed } else { KeyState::Released },
        text: (types && down).then(|| blitz_traits::SmolStr::new(name)),
    }
}

/// The module's script and CSS: its prebuilt bundle beside it (an exported game's, [`BUNDLED`]), or [`build`] now.
pub fn code_of(file: &Path) -> R<(String, String)> {
    let pre = PathBuf::from(format!("{}{BUNDLED}", file.display()));
    if pre.is_file() {
        let js = std::fs::read_to_string(&pre).map_err(|e| format!("{}: {e}", pre.display()))?;
        let css = std::fs::read_to_string(format!("{}{BUNDLED_CSS}", file.display())).unwrap_or_default();
        return Ok((js, css));
    }
    build(file)
}

/// `file` bundled as an app screen (raea-runtime-bundle-lib's app bundle, its default export given the screen's size):
/// its script, and its CSS, what its modules import, then Tailwind's for the classes its folder uses, as `raea app`
/// builds an app's (apps/targets web.rs).
pub fn build(file: &Path) -> R<(String, String)> {
    if !file.is_file() {
        return Err(format!("{}: no such module", file.display()));
    }
    let (js, imported) = raea_runtime_bundle::inproc::app::bundle(file, &raea_io_home::home(), raea_runtime_bundle::inproc::app::SCREEN)?;
    let tw = raea_runtime_bundle::styles::tailwind(file.parent().unwrap_or(Path::new(".")))?;
    Ok((js, format!("{imported}\n{tw}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_type_characters_but_not_with_control() {
        let a = key_event("a", Modifiers::empty(), true);
        assert_eq!(a.text.as_deref(), Some("a"));
        assert!(key_event("a", Modifiers::CONTROL, true).text.is_none());
        assert!(key_event("a", Modifiers::empty(), false).text.is_none());
        let enter = key_event("Enter", Modifiers::empty(), true);
        assert!(matches!(enter.key, Key::Named(_)) && enter.text.is_none());
        assert_eq!(key_event("Space", Modifiers::empty(), true).text.as_deref(), Some(" "));
        assert_eq!(mods_of(&["Shift".into(), "a".into()]), Modifiers::SHIFT);
    }

    #[test]
    fn the_logged_input_comes_back_the_same() {
        let r = Routed { focus: Some("kit".into()), keys: vec!["a".into(), "Pad0:South".into()], pointer: Some(("kit".into(), 12.5, 40.0, true)), wheel: vec!["Wheel:Y 120".into()] };
        let v = r.logged().unwrap();
        let back = Routed::from_log(Some(&v));
        assert_eq!(back, r);
        assert_eq!(back.input_of("kit"), Input { pointer: Some((12.5, 40.0, true)), keys: vec!["a".into()], wheel: vec![120.0] });
        assert_eq!(back.input_of("other"), Input::default());
        assert_eq!(Routed::from_log(None).logged(), None);
    }
}
