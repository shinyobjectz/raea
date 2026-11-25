//! The contract's `input` events in, as the window gets them. Headless there is no NSWindow and no winit, so each event
//! is written as the messages bevy_winit writes for it (its `forward_bevy_events`): the typed message (CursorMoved,
//! MouseButtonInput, MouseWheel, KeyboardInput, Ime, CursorLeft, WindowResized) and its `WindowEvent` mirror, with the
//! window's cursor position kept as winit keeps it. Bevy's input, focus, picking and text editing read those exactly as
//! they would from a real window. One substitution: the window's pixels are a render-target image (no surface), so the
//! pointer's picking location names that image, which is what bevy_picking's `mouse_pick_events` would write had the
//! camera drawn to a window (that system is turned off; `pointer` below is its twin).
//!
//! Ordering: events go in at the start of the frame, before picking. After a button goes down or up the rest of the
//! batch waits for the next frame, so a press is picked (and focus moves) before the keys typed after it are dispatched.
//!
//! With the window on screen (bevy_winit) the same tool still works: its events are written as winit writes them
//! (a trackpad's deltas in physical pixels, the cursor at the window's scale) and reach picking the way the window's
//! own do, through `window_pointer`.

use std::collections::VecDeque;
use std::sync::mpsc::Sender;
use std::time::{Duration, Instant};

use bevy::camera::NormalizedRenderTarget;
use bevy::input::ButtonState;
use bevy::input::keyboard::{Key, KeyCode, KeyboardInput, NativeKeyCode};
use bevy::input::mouse::{MouseButton, MouseButtonInput, MouseScrollUnit, MouseWheel};
use bevy::input::touch::TouchPhase;
use bevy::picking::pointer::{Location, PointerAction, PointerButton, PointerId, PointerInput};
use bevy::prelude::*;
use bevy::render::render_resource::Extent3d;
use bevy::window::{CursorLeft, CursorMoved, Ime, PrimaryWindow, WindowEvent, WindowResized};
use serde_json::Value;

use crate::mcp::Answer;

/// The frame counter: one per app update, the number `input` returns and `ui` reports.
#[derive(Resource, Default)]
pub struct Frame(pub u64);

/// The window's pixels: the image the UI camera draws into, at `scale` pixels a point; or, `windowed`, the window on
/// screen (its own scale, its surface: `image` unused).
#[derive(Resource, Clone)]
pub struct Target {
    pub image: Handle<Image>,
    pub norm: NormalizedRenderTarget,
    pub scale: f32,
    pub windowed: bool,
}

#[derive(Resource, Default)]
pub struct Pending {
    pub cursor: Option<Vec2>,
    /// input events taken (src/brp.rs `input`) and not yet applied, with the frame each lands before
    inputs: VecDeque<(Value, u64)>,
    pub png: Vec<Sender<Answer>>,
    /// modifiers held by a `key` with mods (released after it)
    held: Vec<(KeyCode, Key)>,
    /// the latest press's click count as AppKit counts it (2: a double click's second press)
    pub clicks: u32,
    /// presses written here that `window_pointer` hasn't seen yet (their count is the tool's, not the clock's)
    injected: u32,
}

/// Presses on the window on screen counted into clicks as AppKit counts them (winit doesn't pass its clickCount): the
/// next press within `gap` and 4 pt of the last is one more.
#[derive(Resource)]
pub struct Clicks {
    gap: Duration,
    last: Option<(Instant, Vec2)>,
    n: u32,
}

impl Clicks {
    pub fn new(gap: Duration) -> Clicks {
        Clicks { gap, last: None, n: 0 }
    }

    fn press(&mut self, at: Vec2) -> u32 {
        let now = Instant::now();
        self.n = match self.last {
            Some((t, a)) if now - t <= self.gap && a.distance(at) <= 4.0 => self.n + 1,
            _ => 1,
        };
        self.last = Some((now, at));
        self.n
    }
}

impl Pending {
    /// An input event queued for the frame `next`, or later: behind a button going down or up the rest wait a frame
    /// each (see `pump`). Returns the frame it lands before.
    pub fn queue(&mut self, v: Value, next: u64) -> u64 {
        let frame = match self.inputs.back() {
            Some((last, f)) => (*f).max(next) + edge(last) as u64,
            None => next,
        };
        self.inputs.push_back((v, frame));
        frame
    }
}

/// An input event queued in `world` (Pending::queue), and known to the `ui` answers that wait for it.
pub fn queue(world: &mut World, v: Value, next: u64) -> u64 {
    let f = world.resource_mut::<Pending>().queue(v, next);
    LAST_INPUT.fetch_max(f, std::sync::atomic::Ordering::SeqCst);
    f
}

/// The frame the host is on.
pub fn frame(world: &World) -> u64 {
    world.resource::<Frame>().0
}

/// A button going down or up: the events after it wait for the next frame, so the press is picked (and focus moves)
/// before the keys typed after it are dispatched.
fn edge(v: &Value) -> bool {
    matches!(v.get("type").or_else(|| v.get("kind")).or_else(|| v.get("event")).and_then(Value::as_str), Some("down" | "up" | "mousedown" | "mouseup"))
}

/// The frame the latest input landed before: a `ui` answer must show a frame at least this late (src/mcp.rs).
pub static LAST_INPUT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

pub fn tick(mut f: ResMut<Frame>) {
    f.0 += 1;
}

/// Applies the input events due this frame (src/brp.rs queued them, each with the frame it was promised).
pub fn pump(world: &mut World) {
    let frame = world.resource::<Frame>().0;
    loop {
        let due = world.resource::<Pending>().inputs.front().is_some_and(|(_, f)| *f <= frame);
        if !due {
            break;
        }
        let (v, _) = world.resource_mut::<Pending>().inputs.pop_front().unwrap();
        if let Err(e) = apply(world, &v) {
            warn!("raea-page-host: input {v}: {e}");
        }
    }
}

fn f(v: &Value, k: &str) -> Option<f32> {
    v.get(k).and_then(Value::as_f64).map(|x| x as f32)
}

fn window(world: &mut World) -> Result<Entity, String> {
    world.query_filtered::<Entity, With<PrimaryWindow>>().single(world).map_err(|e| e.to_string())
}

fn pointer(world: &mut World, action: PointerAction, at: Vec2) {
    let t = world.resource::<Target>();
    // on screen, `window_pointer` makes it from the window event written beside it, as it does the window's own
    if t.windowed {
        return;
    }
    let target = t.norm.clone();
    // a point on an edge between two boxes is in the one below or right of it, as a browser hit-tests ([min, max)):
    // Bevy's UI picking leaves edges out of both, so an injected whole-pixel point is nudged just inside
    let at = at + Vec2::splat(1.0 / 256.0);
    world.write_message(PointerInput::new(PointerId::Mouse, Location { target, position: at }, action));
}

/// Pixels a point: the image's, or the window's on screen.
fn scale_of(world: &World, win: Entity) -> f32 {
    let t = world.resource::<Target>();
    if t.windowed { world.get::<Window>(win).map(|w| w.scale_factor()).unwrap_or(1.0) } else { t.scale }
}

fn move_to(world: &mut World, win: Entity, at: Vec2) {
    let last = world.resource::<Pending>().cursor;
    let scale = scale_of(world, win);
    let moved = CursorMoved { window: win, position: at, delta: last.map(|l| at - l) };
    world.write_message(moved.clone());
    world.write_message(WindowEvent::CursorMoved(moved));
    if let Some(mut w) = world.get_mut::<Window>(win) {
        w.set_physical_cursor_position(Some((at * scale).as_dvec2()));
    }
    world.resource_mut::<Pending>().cursor = Some(at);
    pointer(world, PointerAction::Move { delta: at - last.unwrap_or(at) }, at);
}

fn at_of(world: &mut World, win: Entity, v: &Value) {
    if let (Some(x), Some(y)) = (f(v, "x"), f(v, "y")) {
        let at = Vec2::new(x, y);
        if world.resource::<Pending>().cursor != Some(at) {
            move_to(world, win, at);
        }
    }
}

fn button(v: &Value) -> (MouseButton, PointerButton) {
    match v.get("button").and_then(|b| b.as_str().map(str::to_string).or_else(|| b.as_i64().map(|n| n.to_string()))).as_deref() {
        Some("right") | Some("secondary") | Some("2") => (MouseButton::Right, PointerButton::Secondary),
        Some("middle") | Some("1") => (MouseButton::Middle, PointerButton::Middle),
        _ => (MouseButton::Left, PointerButton::Primary),
    }
}

/// A DOM key name as winit hands it: the physical key, the logical key, and the text it types.
fn key_of(name: &str) -> (KeyCode, Key, Option<String>) {
    let named = |c: KeyCode, k: Key| (c, k, None);
    match name {
        "Enter" | "Return" => named(KeyCode::Enter, Key::Enter),
        "Backspace" => named(KeyCode::Backspace, Key::Backspace),
        "Escape" | "Esc" => named(KeyCode::Escape, Key::Escape),
        "Tab" => named(KeyCode::Tab, Key::Tab),
        " " | "Space" | "Spacebar" => (KeyCode::Space, Key::Space, Some(" ".into())),
        "ArrowLeft" | "Left" => named(KeyCode::ArrowLeft, Key::ArrowLeft),
        "ArrowRight" | "Right" => named(KeyCode::ArrowRight, Key::ArrowRight),
        "ArrowUp" | "Up" => named(KeyCode::ArrowUp, Key::ArrowUp),
        "ArrowDown" | "Down" => named(KeyCode::ArrowDown, Key::ArrowDown),
        "Delete" => named(KeyCode::Delete, Key::Delete),
        "Home" => named(KeyCode::Home, Key::Home),
        "End" => named(KeyCode::End, Key::End),
        "Shift" => named(KeyCode::ShiftLeft, Key::Shift),
        "Meta" | "Cmd" | "Command" | "Super" => named(KeyCode::SuperLeft, Key::Super),
        "Control" | "Ctrl" => named(KeyCode::ControlLeft, Key::Control),
        "Alt" | "Option" => named(KeyCode::AltLeft, Key::Alt),
        s => {
            let mut cs = s.chars();
            let (Some(c), None) = (cs.next(), cs.next()) else {
                return (KeyCode::Unidentified(NativeKeyCode::Unidentified), Key::Character(s.into()), Some(s.into()));
            };
            let code = match c.to_ascii_lowercase() {
                'a' => KeyCode::KeyA, 'b' => KeyCode::KeyB, 'c' => KeyCode::KeyC, 'd' => KeyCode::KeyD, 'e' => KeyCode::KeyE,
                'f' => KeyCode::KeyF, 'g' => KeyCode::KeyG, 'h' => KeyCode::KeyH, 'i' => KeyCode::KeyI, 'j' => KeyCode::KeyJ,
                'k' => KeyCode::KeyK, 'l' => KeyCode::KeyL, 'm' => KeyCode::KeyM, 'n' => KeyCode::KeyN, 'o' => KeyCode::KeyO,
                'p' => KeyCode::KeyP, 'q' => KeyCode::KeyQ, 'r' => KeyCode::KeyR, 's' => KeyCode::KeyS, 't' => KeyCode::KeyT,
                'u' => KeyCode::KeyU, 'v' => KeyCode::KeyV, 'w' => KeyCode::KeyW, 'x' => KeyCode::KeyX, 'y' => KeyCode::KeyY,
                'z' => KeyCode::KeyZ, '0' => KeyCode::Digit0, '1' => KeyCode::Digit1, '2' => KeyCode::Digit2, '3' => KeyCode::Digit3,
                '4' => KeyCode::Digit4, '5' => KeyCode::Digit5, '6' => KeyCode::Digit6, '7' => KeyCode::Digit7, '8' => KeyCode::Digit8,
                '9' => KeyCode::Digit9, '/' | '?' => KeyCode::Slash, '.' | '>' => KeyCode::Period, ',' | '<' => KeyCode::Comma,
                '-' | '_' => KeyCode::Minus, '=' | '+' => KeyCode::Equal, ';' | ':' => KeyCode::Semicolon, '\'' | '"' => KeyCode::Quote,
                '[' | '{' => KeyCode::BracketLeft, ']' | '}' => KeyCode::BracketRight, '\\' | '|' => KeyCode::Backslash,
                '`' | '~' => KeyCode::Backquote,
                _ => KeyCode::Unidentified(NativeKeyCode::Unidentified),
            };
            (code, Key::Character(s.into()), Some(s.into()))
        }
    }
}

fn key_event(world: &mut World, win: Entity, code: KeyCode, key: Key, text: Option<String>, state: ButtonState) {
    let ev = KeyboardInput { key_code: code, logical_key: key, state, text: if state == ButtonState::Pressed { text.map(Into::into) } else { None }, repeat: false, window: win };
    world.write_message(ev.clone());
    world.write_message(WindowEvent::KeyboardInput(ev));
}

fn ime(world: &mut World, ev: Ime) {
    world.write_message(ev.clone());
    world.write_message(WindowEvent::Ime(ev));
}

/// One event. Ok(true) when it was a button going down or up (the rest of the batch waits a frame).
fn apply(world: &mut World, v: &Value) -> Result<bool, String> {
    let win = window(world)?;
    let kind = v.get("type").or_else(|| v.get("kind")).or_else(|| v.get("event")).and_then(Value::as_str).unwrap_or("").to_string();
    match kind.as_str() {
        "move" | "mousemove" => {
            let at = Vec2::new(f(v, "x").ok_or("move needs x")?, f(v, "y").ok_or("move needs y")?);
            move_to(world, win, at);
            Ok(false)
        }
        "down" | "up" | "mousedown" | "mouseup" => {
            at_of(world, win, v);
            let at = world.resource::<Pending>().cursor.unwrap_or_default();
            let (mb, pb) = button(v);
            let state = if kind.ends_with("down") { ButtonState::Pressed } else { ButtonState::Released };
            if state == ButtonState::Pressed {
                let windowed = world.resource::<Target>().windowed;
                let mut p = world.resource_mut::<Pending>();
                p.clicks = v.get("clicks").and_then(Value::as_u64).unwrap_or(1) as u32;
                if windowed {
                    p.injected += 1;
                }
            }
            let ev = MouseButtonInput { button: mb, state, window: win };
            world.write_message(ev);
            world.write_message(WindowEvent::MouseButtonInput(ev));
            pointer(world, if state == ButtonState::Pressed { PointerAction::Press(pb) } else { PointerAction::Release(pb) }, at);
            Ok(true)
        }
        "leave" | "mouseleave" => {
            let ev = CursorLeft { window: win };
            world.write_message(ev.clone());
            world.write_message(WindowEvent::CursorLeft(ev));
            if let Some(mut w) = world.get_mut::<Window>(win) {
                w.set_physical_cursor_position(None);
            }
            // bevy_picking keeps a pointer's last location when it leaves a window (winit's CursorLeft has no picking
            // counterpart), so hover would stick; a location off every camera's viewport ends it
            world.resource_mut::<Pending>().cursor = None;
            pointer(world, PointerAction::Move { delta: Vec2::ZERO }, Vec2::splat(-1.0e6));
            Ok(false)
        }
        "wheel" | "scroll" => {
            at_of(world, win, v);
            let at = world.resource::<Pending>().cursor.unwrap_or_default();
            let unit = if v.get("unit").and_then(Value::as_str) == Some("line") { MouseScrollUnit::Line } else { MouseScrollUnit::Pixel };
            let phase = match v.get("phase").and_then(Value::as_str) {
                Some("began") | Some("start") | Some("started") => TouchPhase::Started,
                Some("ended") | Some("end") => TouchPhase::Ended,
                Some("cancelled") | Some("canceled") => TouchPhase::Canceled,
                _ => TouchPhase::Moved,
            };
            // dy as AppKit's scrollingDeltaY hands it (natural scrolling already applied by the OS): positive moves the
            // content down, toward its top, which is what winit passes and Bevy's scroll area takes
            let (x, y) = (f(v, "dx").unwrap_or(0.0), f(v, "dy").unwrap_or(0.0));
            // on screen, a trackpad's pixels as winit writes them: physical (window_pointer makes them points again)
            let k = if world.resource::<Target>().windowed && unit == MouseScrollUnit::Pixel { scale_of(world, win) } else { 1.0 };
            let ev = MouseWheel { unit, x: x * k, y: y * k, window: win, phase };
            world.write_message(ev);
            world.write_message(WindowEvent::MouseWheel(ev));
            pointer(world, PointerAction::Scroll { x, y, unit, phase }, at);
            Ok(false)
        }
        "key" | "keydown" | "keyup" => {
            let name = v.get("key").and_then(Value::as_str).ok_or("key needs key")?;
            let state = v.get("state").and_then(Value::as_str).map(str::to_string).unwrap_or_else(|| kind.trim_start_matches("key").to_string());
            let mods: Vec<String> = v.get("mods").and_then(Value::as_array).map(|a| a.iter().filter_map(|m| m.as_str().map(str::to_string)).collect()).unwrap_or_default();
            let mods: Vec<(KeyCode, Key)> = mods
                .iter()
                .map(|m| match m.to_lowercase().as_str() {
                    "shift" => (KeyCode::ShiftLeft, Key::Shift),
                    "ctrl" | "control" => (KeyCode::ControlLeft, Key::Control),
                    "alt" | "option" | "opt" => (KeyCode::AltLeft, Key::Alt),
                    _ => (KeyCode::SuperLeft, Key::Super),
                })
                .collect();
            let (code, key, text) = key_of(name);
            // a command chord types nothing
            let text = if mods.iter().any(|(c, _)| matches!(c, KeyCode::SuperLeft | KeyCode::ControlLeft)) { None } else { text };
            let down = state != "up";
            let up = state != "down";
            if down {
                for (c, k) in &mods {
                    key_event(world, win, *c, k.clone(), None, ButtonState::Pressed);
                }
                key_event(world, win, code, key.clone(), text, ButtonState::Pressed);
                world.resource_mut::<Pending>().held = mods.clone();
            }
            if up {
                key_event(world, win, code, key, None, ButtonState::Released);
                let held = std::mem::take(&mut world.resource_mut::<Pending>().held);
                for (c, k) in if mods.is_empty() { held } else { mods } {
                    key_event(world, win, c, k, None, ButtonState::Released);
                }
            }
            Ok(false)
        }
        "text" | "insert" => {
            let text = v.get("text").and_then(Value::as_str).ok_or("text needs text")?.to_string();
            // AppKit's insertText: with the IME allowed (a text field has focus, bevy_ui_widgets turned it on), winit hands
            // it in as a commit; otherwise each character arrives as a key
            let ime_on = world.get::<Window>(win).is_some_and(|w| w.ime_enabled);
            if ime_on {
                ime(world, Ime::Commit { window: win, value: text });
            } else {
                for c in text.chars() {
                    let (code, key, t) = key_of(&c.to_string());
                    key_event(world, win, code, key.clone(), t, ButtonState::Pressed);
                    key_event(world, win, code, key, None, ButtonState::Released);
                }
            }
            Ok(false)
        }
        "ime" => {
            if let Some(m) = v.get("marked").and_then(Value::as_str) {
                let n = m.len();
                ime(world, Ime::Preedit { window: win, value: m.to_string(), cursor: if m.is_empty() { None } else { Some((n, n)) } });
            }
            if let Some(c) = v.get("commit").and_then(Value::as_str) {
                ime(world, Ime::Preedit { window: win, value: String::new(), cursor: None });
                ime(world, Ime::Commit { window: win, value: c.to_string() });
            }
            Ok(false)
        }
        "resize" => {
            let (w, h) = (f(v, "w").ok_or("resize needs w")?.max(1.0), f(v, "h").ok_or("resize needs h")?.max(1.0));
            resize(world, win, w, h);
            Ok(false)
        }
        k => Err(format!("unknown input type {k:?}")),
    }
}

pub fn resize(world: &mut World, win: Entity, w: f32, h: f32) {
    let t = world.resource::<Target>().clone();
    if let Some(mut wn) = world.get_mut::<Window>(win) {
        wn.resolution.set(w, h);
    }
    // on screen, the window is asked for the size and winit reports it (WindowResized) once it has it
    if t.windowed {
        return;
    }
    let size = Extent3d { width: (w * t.scale).round() as u32, height: (h * t.scale).round() as u32, depth_or_array_layers: 1 };
    if let Some(mut img) = world.resource_mut::<Assets<Image>>().get_mut(&t.image) {
        img.resize(size);
    }
    let ev = WindowResized { window: win, width: w, height: h };
    world.write_message(ev.clone());
    world.write_message(WindowEvent::WindowResized(ev));
}

/// The window on screen: its pointer events (and those `input` writes beside them) into picking, as bevy_picking's
/// `mouse_pick_events` writes them (turned off: PointerInputSettings), with two differences. A trackpad's deltas are
/// points, as AppKit's scrollingDeltaY hands them, natural scrolling already in them: winit makes them physical pixels,
/// which ScrollArea and onWheel would move twice as far on a 2x screen. And a press's click count is kept (Clicks), for
/// onPointerDown's `detail`: the header's double click zooms the window.
///
/// A pointer with a button down leaves the window on its release, as Chrome's does: AppKit reports the mouse itself
/// outside the window between an assistive client's press and its release, and the release was dropped with the
/// pointer's place (a press on a card opened nothing, 11-24).
pub fn window_pointer(mut evs: MessageReader<WindowEvent>, target: Res<Target>, windows: Query<&Window>, mut out: MessageWriter<PointerInput>, mut pending: ResMut<Pending>, mut clicks: ResMut<Clicks>, mut at: Local<Option<Vec2>>, mut down: Local<Vec<MouseButton>>, mut gone: Local<bool>) {
    // off every camera's viewport, so hover ends (picking keeps a pointer's last place when it leaves)
    const OFF: Vec2 = Vec2::splat(-1.0e6);
    let mut put = |position: Vec2, action: PointerAction| {
        out.write(PointerInput::new(PointerId::Mouse, Location { target: target.norm.clone(), position }, action));
    };
    for ev in evs.read() {
        match ev {
            WindowEvent::CursorMoved(e) => {
                let delta = at.map(|a| e.position - a).unwrap_or(Vec2::ZERO);
                *at = Some(e.position);
                *gone = false;
                // where an injected press is (`at_of`): it moves there first unless it's the pointer's place now
                pending.cursor = *at;
                put(e.position, PointerAction::Move { delta });
            }
            WindowEvent::MouseButtonInput(e) => {
                let Some(p) = *at else { continue };
                let b = match e.button {
                    MouseButton::Left => PointerButton::Primary,
                    MouseButton::Right => PointerButton::Secondary,
                    MouseButton::Middle => PointerButton::Middle,
                    _ => continue,
                };
                match e.state {
                    ButtonState::Pressed => {
                        if pending.injected > 0 {
                            pending.injected -= 1;
                        } else if b == PointerButton::Primary {
                            pending.clicks = clicks.press(p);
                        }
                        if !down.contains(&e.button) {
                            down.push(e.button);
                        }
                        put(p, PointerAction::Press(b));
                    }
                    ButtonState::Released => {
                        down.retain(|d| *d != e.button);
                        put(p, PointerAction::Release(b));
                        if down.is_empty() && std::mem::take(&mut *gone) {
                            *at = None;
                            pending.cursor = None;
                            put(OFF, PointerAction::Move { delta: Vec2::ZERO });
                        }
                    }
                }
            }
            WindowEvent::MouseWheel(e) => {
                let Some(p) = *at else { continue };
                let k = if e.unit == MouseScrollUnit::Pixel { windows.get(e.window).map(|w| w.scale_factor()).unwrap_or(1.0) } else { 1.0 };
                put(p, PointerAction::Scroll { x: e.x / k, y: e.y / k, unit: e.unit, phase: e.phase });
            }
            WindowEvent::CursorLeft(_) if !down.is_empty() => *gone = true,
            WindowEvent::CursorLeft(_) => {
                *at = None;
                pending.cursor = None;
                put(OFF, PointerAction::Move { delta: Vec2::ZERO });
            }
            _ => {}
        }
    }
}
