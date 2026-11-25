//! The window on screen's pointer (inject::window_pointer): what its events become for picking.

use std::time::Duration;

use bevy::camera::NormalizedRenderTarget;
use bevy::input::ButtonState;
use bevy::input::mouse::MouseButtonInput;
use bevy::picking::pointer::{PointerAction, PointerInput};
use bevy::prelude::*;
use bevy::window::{CursorLeft, CursorMoved, WindowEvent};
use raea_page_host::inject::{Clicks, Pending, Target, window_pointer};

fn app() -> App {
    let mut app = App::new();
    let norm = NormalizedRenderTarget::None { width: 100, height: 100 };
    app.add_message::<WindowEvent>().add_message::<PointerInput>();
    app.insert_resource(Target { image: Handle::default(), norm, scale: 1.0, windowed: true });
    app.insert_resource(Clicks::new(Duration::from_millis(500))).init_resource::<Pending>();
    app.add_systems(Update, window_pointer);
    app
}

/// The window's events of one frame, and the pointer actions they became.
fn frame(app: &mut App, evs: Vec<WindowEvent>) -> Vec<(Vec2, PointerAction)> {
    for e in evs {
        app.world_mut().write_message(e);
    }
    app.update();
    app.world_mut().resource_mut::<Messages<PointerInput>>().drain().map(|i| (i.location.position, i.action)).collect()
}

fn moved(x: f32, y: f32) -> WindowEvent {
    CursorMoved { window: Entity::PLACEHOLDER, position: Vec2::new(x, y), delta: None }.into()
}

fn button(state: ButtonState) -> WindowEvent {
    MouseButtonInput { button: MouseButton::Left, state, window: Entity::PLACEHOLDER }.into()
}

fn left() -> WindowEvent {
    CursorLeft { window: Entity::PLACEHOLDER }.into()
}

fn releases(got: &[(Vec2, PointerAction)]) -> Vec<Vec2> {
    let mut at = vec![];
    for (p, act) in got {
        if let PointerAction::Release(_) = act {
            at.push(*p);
        }
    }
    at
}

#[test]
fn a_held_pointer_leaves_on_its_release() {
    let mut app = app();
    frame(&mut app, vec![moved(40.0, 30.0), button(ButtonState::Pressed)]);
    // the mouse itself is reported outside the window while the press is held: nothing yet
    assert!(frame(&mut app, vec![left()]).is_empty());
    let got = frame(&mut app, vec![button(ButtonState::Released)]);
    assert_eq!(releases(&got), vec![Vec2::new(40.0, 30.0)], "the release is where the press was");
    assert!(matches!(got.last(), Some((p, PointerAction::Move { .. })) if p.x < 0.0), "then it leaves: {got:?}");
    assert_eq!(app.world().resource::<Pending>().cursor, None, "an injected press at the same place moves there first");
}

#[test]
fn a_held_pointer_back_in_the_window_stays() {
    let mut app = app();
    frame(&mut app, vec![moved(40.0, 30.0), button(ButtonState::Pressed), left(), moved(50.0, 30.0)]);
    let got = frame(&mut app, vec![button(ButtonState::Released)]);
    assert_eq!(got.len(), 1, "its release alone: {got:?}");
    assert_eq!(app.world().resource::<Pending>().cursor, Some(Vec2::new(50.0, 30.0)));
}

#[test]
fn a_pointer_with_no_button_down_leaves_at_once() {
    let mut app = app();
    let got = frame(&mut app, vec![moved(40.0, 30.0), left()]);
    assert!(matches!(got.last(), Some((p, PointerAction::Move { .. })) if p.x < 0.0), "{got:?}");
    assert_eq!(app.world().resource::<Pending>().cursor, None);
}
