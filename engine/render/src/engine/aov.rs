//! RT1 (docs/raytracing.feature): each frame's motion vectors and depth, the guide data a neural or denoising pass
//! after the render needs (MLX-DLSS's run-sequence, MetalFX). Asked for with `want_aov`, before the first draw: the 3D
//! cameras then draw Bevy's depth and motion-vector prepasses (MSAA off on every frame: they're read single-sampled), and
//! each frame's first draw reads both back (`take_aov`). A redraw of the same frame keeps the first draw's: drawn
//! again, every mesh's last pose is its pose now, and its motion would read zero.

use super::*;
pub use raea_sys_compose::aov::Aov;

/// The guide data is wanted; `frame` is the frame last read, `last` what it read.
#[derive(Resource, Default)]
pub(crate) struct AovWanted {
    frame: Option<i64>,
    last: Option<Aov>,
}

impl Engine {
    /// Draw the guide data from now on (before the first draw: a camera's prepass is set up as it's extracted).
    pub fn want_aov(&mut self) {
        if self.app.world().contains_resource::<AovWanted>() {
            return;
        }
        self.app.init_resource::<AovWanted>();
        self.app.add_systems(Update, raea_sys_compose::aov::prepasses);
    }

    /// Frame f's guide data, read after its first draw (Engine::draw calls this).
    pub(crate) fn capture_aov(&mut self, f: i64) {
        let Some(w) = self.app.world().get_resource::<AovWanted>() else { return };
        if w.frame == Some(f) {
            return;
        }
        let (fw, fh) = (self.w, self.h);
        let got = self.app.get_sub_app_mut(bevy::render::RenderApp).ok_or_else(|| "no render app".to_string()).and_then(|a| raea_sys_compose::aov::read(a.world_mut(), fw, fh));
        let mut w = self.app.world_mut().resource_mut::<AovWanted>();
        w.frame = Some(f);
        match got {
            Ok(a) => w.last = Some(a),
            Err(e) => {
                w.last = None;
                eprintln!("raea: frame {f}: no motion vectors or depth: {e}");
            }
        }
    }

    /// The guide data the last frame drawn read, taken (None when it wasn't asked for or couldn't be read).
    pub fn take_aov(&mut self) -> Option<Aov> {
        self.app.world_mut().get_resource_mut::<AovWanted>()?.last.take()
    }
}
