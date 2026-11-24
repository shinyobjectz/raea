//! Composition passes: which of a piece's passes drew each pixel, so frame-wide effects can leave some of the scene as
//! drawn (compose.rs); and a frame's passes in place of its picture, RAEA_PASS: depth, normal, albedo (passes.rs).

use std::collections::BTreeMap;

use bevy::camera::RenderTarget;
use bevy::prelude::*;
use raea_core_program::ir::V;
use raea_sys_frame::*;
use raea_sys_frame::bevy_api;
use raea_sys_rt as rt;

pub mod compose;
pub mod passes;
// each frame's motion vectors and depth read back for a pass after the render (RT1)
pub mod aov;
