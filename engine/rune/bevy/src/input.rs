//! Input to a layer's tree. [`Input`] is what any caller sends (`AnyTree::input`): a game forwarding a HUD's clicks, a
//! check's script. A layer with [`RuneOverlay`] fills a window: its viewport follows the window's logical size and
//! scale, and the window's pointer, wheel and keys reach it as Rune's own shell sends them (keys.rs maps Bevy's keys).

use bevy::input::ButtonState;
use bevy::input::keyboard::KeyboardInput;
use bevy::input::mouse::{MouseButtonInput, MouseScrollUnit, MouseWheel};
use bevy::prelude::*;
use bevy::window::{CursorLeft, CursorMoved, PrimaryWindow};
use std::collections::HashMap;
use rune::{KeyEvent, Runtime};

use crate::{RuneLayer, Trees};

/// Pointer, wheel and keys, in the tree's points.
#[derive(Clone, Debug)]
pub enum Input {
    Move(f32, f32),
    Down(f32, f32),
    Up(f32, f32),
    /// the secondary (right) button
    SecondaryDown(f32, f32),
    SecondaryUp(f32, f32),
    Leave,
    /// a wheel at a point: `dy` points, positive moves the content down (Rune's `wheel`)
    Wheel(f32, f32, f32),
    Key(KeyEvent),
    KeyUp(KeyEvent),
}

/// One input into `rt`, through Rune's own pointer and key paths (hit-testing, focus, text editing, on_key).
pub fn apply<S>(rt: &mut Runtime<S>, ev: Input) {
    match ev {
        Input::Move(x, y) => rt.pointer_move(x, y),
        Input::Down(x, y) => rt.pointer_down(x, y),
        Input::Up(x, y) => rt.pointer_up(x, y),
        Input::SecondaryDown(x, y) => rt.secondary_down(x, y),
        Input::SecondaryUp(x, y) => rt.secondary_up(x, y),
        Input::Leave => rt.pointer_leave(),
        Input::Wheel(x, y, dy) => rt.wheel(x, y, dy),
        Input::Key(k) => rt.key(k),
        Input::KeyUp(k) => {
            rt.key_up(k);
        }
    }
}

/// A layer that fills `window` (the primary window when none): sized to it and given its input.
#[derive(Component, Clone, Copy, Default)]
pub struct RuneOverlay {
    pub window: Option<Entity>,
}


/// PostUpdate, before the draw: each overlay's tree at its window's logical size and scale.
pub(crate) fn overlay_size(overlays: Query<(Entity, &RuneOverlay), With<RuneLayer>>, windows: Query<&Window>, primary: Query<Entity, With<PrimaryWindow>>, mut trees: NonSendMut<Trees>) {
    let primary = primary.iter().next();
    for (e, o) in &overlays {
        let Some(w) = o.window.or(primary).and_then(|w| windows.get(w).ok()) else { continue };
        let (size, scale) = ((w.resolution.width(), w.resolution.height()), w.resolution.scale_factor());
        if let Some(t) = trees.0.get_mut(&e)
            && t.viewport() != (size, scale)
        {
            t.resize(size, scale);
        }
    }
}

/// PreUpdate: the windows' pointer, wheel and keys to the overlays that fill them.
#[allow(clippy::too_many_arguments)]
pub(crate) fn forward(
    overlays: Query<(Entity, &RuneOverlay), With<RuneLayer>>,
    primary: Query<Entity, With<PrimaryWindow>>,
    windows: Query<&Window>,
    mut moved: MessageReader<CursorMoved>,
    mut left: MessageReader<CursorLeft>,
    mut buttons: MessageReader<MouseButtonInput>,
    mut wheels: MessageReader<MouseWheel>,
    mut keys: MessageReader<KeyboardInput>,
    held: Res<ButtonInput<KeyCode>>,
    mut trees: NonSendMut<Trees>,
    mut at: Local<HashMap<Entity, Vec2>>,
) {
    let primary = primary.iter().next();
    let targets: Vec<(Entity, Entity)> = overlays.iter().filter_map(|(e, o)| Some((e, o.window.or(primary)?))).collect();
    let trace = raea_io_trace::stat("RAEA_RUNE_BEVY_TRACE");
    let mut send = |window: Entity, ev: Input| {
        if trace {
            eprintln!("raea-rune-bevy: {window} {ev:?}");
        }
        for (e, _) in targets.iter().filter(|(_, w)| *w == window) {
            if let Some(t) = trees.0.get_mut(e) {
                t.input(ev.clone());
            }
        }
    };
    for m in moved.read() {
        at.insert(m.window, m.position);
        send(m.window, Input::Move(m.position.x, m.position.y));
    }
    for m in left.read() {
        at.remove(&m.window);
        send(m.window, Input::Leave);
    }
    // where the pointer last moved to in the window (CursorMoved's logical points), else where the window says it is
    let cursor = |w: Entity| at.get(&w).copied().or_else(|| windows.get(w).ok().and_then(Window::cursor_position));
    for b in buttons.read() {
        let Some(p) = cursor(b.window) else { continue };
        let ev = match (b.button, b.state) {
            (MouseButton::Left, ButtonState::Pressed) => Input::Down(p.x, p.y),
            (MouseButton::Left, ButtonState::Released) => Input::Up(p.x, p.y),
            (MouseButton::Right, ButtonState::Pressed) => Input::SecondaryDown(p.x, p.y),
            (MouseButton::Right, ButtonState::Released) => Input::SecondaryUp(p.x, p.y),
            _ => continue,
        };
        send(b.window, ev);
    }
    for w in wheels.read() {
        let Some(p) = cursor(w.window) else { continue };
        let scale = windows.get(w.window).map_or(1.0, |w| w.resolution.scale_factor());
        // as Rune's shell reads winit's deltas (runtime/scroll/x.rs): a line is 40 points, pixels are physical
        let dy = match w.unit {
            MouseScrollUnit::Line => w.y * 40.0,
            MouseScrollUnit::Pixel => w.y / scale,
        };
        send(w.window, Input::Wheel(p.x, p.y, dy));
    }
    let mods = crate::keys::mods(&held);
    for k in keys.read() {
        let Some(ev) = crate::keys::event(k, mods) else { continue };
        send(k.window, if k.state == ButtonState::Pressed { Input::Key(ev) } else { Input::KeyUp(ev) });
    }
}
