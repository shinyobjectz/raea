//! Light baked by raea (docs/bake.feature): bounce light baked into lightmaps on raea's own ray tracer, with no
//! Blender. A bake is a pure function of what it's asked: the static parts, the sky, the lights it carries and the
//! settings. Its maps lie over the atlas raea/wear's cause maps already use (BK1: raea_sys_atlas::layout), so a
//! model has one second UV set. Built: each texel's place in the world (BK2), the light that bounces (BK3, in
//! progress). Next: a clean map (BK4), and the bake in a composition (BK5).

// each texel's ray origin and the texels inside other geometry (BK2)
pub mod texels;
// the light that bounces, traced on the GPU (BK3)
pub mod trace;
