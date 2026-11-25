//! The window at rest (on by default since 0.1.6; RAEA_IDLE_GATE=0 draws every frame). Bevy's default runs the whole
//! frame 60 times a second, focused or behind other windows, though a page painted the same isn't drawn again (draw.rs):
//! the page's step, its restyle and paint, and Bevy's render and present of a picture that didn't change were 0.29 of a
//! core with the app's feed open and still (11-16). After half a second in which no page was drawn and none is
//! animating, the window waits for input before its next frame (or 50 ms when focused, 250 ms behind other windows, so
//! a page's timers and answers still arrive); the first frame that draws a page puts it back to every frame. A playing
//! video, a game on the stage and any view draw their page every frame, so they never rest.

use std::time::Duration;

use bevy::prelude::*;
use bevy::winit::{UpdateMode, WinitSettings};

/// Frames in a row with nothing drawn before the window rests.
const QUIET: u32 = 30;

pub struct IdlePlugin;

impl Plugin for IdlePlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Last, rest);
    }
}

fn rest(settings: Option<ResMut<WinitSettings>>, docs: Option<NonSend<raea_sys_page::Docs>>, mut last: Local<u64>, mut quiet: Local<u32>) {
    let Some(mut settings) = settings else { return };
    let drawn = raea_sys_page::DRAWS.load(std::sync::atomic::Ordering::Relaxed);
    let moved = drawn != *last || docs.is_some_and(|d| d.0.values().any(|d| d.is_animating()));
    *last = drawn;
    *quiet = quieter(*quiet, moved);
    let (focused, unfocused) = modes(*quiet);
    if settings.focused_mode != focused || settings.unfocused_mode != unfocused {
        settings.focused_mode = focused;
        settings.unfocused_mode = unfocused;
    }
}

/// The frames in a row with nothing drawn after this one: none when a page was drawn or one is animating.
fn quieter(quiet: u32, moved: bool) -> u32 {
    if moved { 0 } else { quiet.saturating_add(1) }
}

/// The window's update modes, focused and behind other windows, `quiet` frames into a run with nothing drawn: every
/// frame until QUIET, then waiting for input (or 50 ms focused, 250 ms behind, for a page's timers and answers).
fn modes(quiet: u32) -> (UpdateMode, UpdateMode) {
    if quiet >= QUIET {
        (UpdateMode::reactive(Duration::from_millis(50)), UpdateMode::reactive_low_power(Duration::from_millis(250)))
    } else {
        (UpdateMode::Continuous, UpdateMode::Continuous)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_window_rests_after_half_a_second_with_nothing_drawn_and_wakes_on_the_next_draw() {
        let mut quiet = 0;
        // a page drawn every frame (a video playing, a game): never rests
        for _ in 0..100 {
            quiet = quieter(quiet, true);
            assert_eq!(modes(quiet).0, UpdateMode::Continuous);
        }
        // nothing drawn: every frame for QUIET frames, then waiting
        for n in 1..=QUIET {
            quiet = quieter(quiet, false);
            assert_eq!(modes(quiet).0 == UpdateMode::Continuous, n < QUIET, "frame {n} of the quiet run");
        }
        assert_eq!(modes(quiet), (UpdateMode::reactive(Duration::from_millis(50)), UpdateMode::reactive_low_power(Duration::from_millis(250))));
        // the first frame that draws (a key, an answer, a timer's change) puts it back to every frame at once
        quiet = quieter(quiet, true);
        assert_eq!(modes(quiet), (UpdateMode::Continuous, UpdateMode::Continuous));
    }
}
