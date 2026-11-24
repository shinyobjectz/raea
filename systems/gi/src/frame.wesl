// What raea GI's screen probes (probes.wesl) and its screen pass (screen.wesl) share, group 2 of both (lib.rs): the
// view's surfaces from its prepasses, the screen the main pass reads, the probes (this frame's and last frame's) and
// the frame's parameters.
import bevy_render::view::View;

@group(2) @binding(0) var<uniform> view: View;
@group(2) @binding(1) var depth_tex: texture_depth_2d;
@group(2) @binding(2) var normal_tex: texture_2d<f32>;
@group(2) @binding(3) var out_tex: texture_storage_2d<rgba16float, write>;

// A probe: where it stands (xyz) and how many frames its mean holds (w; 0 where its tile shows only sky), its
// surface's normal, and the light arriving there (irradiance, lux), less the sun on the probe itself (raster draws it).
struct Probe {
    at: vec4<f32>,
    n: vec4<f32>,
    e: vec4<f32>,
}
@group(2) @binding(4) var<storage, read_write> now: array<Probe>;
@group(2) @binding(5) var<storage, read> then: array<Probe>;

struct Params {
    /// toward the sun (xyz)
    sun_dir: vec4<f32>,
    /// the sun's light (rgb, lux), as the cache's cells were lit by it
    sun: vec4<f32>,
    /// the frame's salt; probes across and down; 1 when `then` holds last frame's probes
    run: vec4<u32>,
    /// last frame's clip_from_world, where a probe finds its history
    then_clip: mat4x4<f32>,
}
@group(2) @binding(6) var<uniform> p: Params;
// this frame's probes filtered in probe space (probes.wesl `filter`), which the screen pass reads; `now` keeps each
// probe's own mean, so the filter doesn't feed back into its history
@group(2) @binding(7) var<storage, read_write> soft: array<Probe>;

/// pixels a probe, across and down
const TILE: u32 = 16u;

/// A pixel's surface: where it is (xyz; w 1, or 0 where the pixel shows sky or lies outside the view) and its normal.
struct Surface {
    at: vec4<f32>,
    n: vec3<f32>,
}
fn surface(px: vec2<i32>) -> Surface {
    let uv = (vec2<f32>(px) + 0.5 - view.viewport.xy) / view.viewport.zw;
    let d = textureLoad(depth_tex, px, 0);
    // sky (reverse z: 0 is infinitely far), or outside the view
    if (d <= 0.0 || any(uv < vec2(0.0)) || any(uv > vec2(1.0))) {
        return Surface(vec4(0.0), vec3(0.0, 1.0, 0.0));
    }
    let w = view.world_from_clip * vec4(uv.x * 2.0 - 1.0, 1.0 - uv.y * 2.0, d, 1.0);
    let n = normalize(textureLoad(normal_tex, px, 0).xyz * 2.0 - 1.0);
    return Surface(vec4(w.xyz / w.w, 1.0), n);
}
