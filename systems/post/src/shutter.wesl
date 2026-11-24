// raea's motion blur (shutter.rs): what a camera records while its shutter is open, each pixel gathered along the
// longest motion near it (shutter_tiles.wesl). A box shutter spreads each moving pixel's light evenly along its path,
// so a pixel x gets from a tap y on that path 1/|v_y| a pixel of path, if y's path reaches x:
// - a tap nearer the camera than x (in front of it): by y's own motion. So a still wall beside a moving ball takes the
//   share of the open shutter the ball is over it, which Bevy's MotionBlur (gathering by each pixel's own motion,
//   none on the wall) drew as no blur at all, the streak only half as long as it is;
// - a tap as far as x or farther: by x's motion, the same surface along x's own path, or what x moved off to show.
// Whatever share of the shutter nothing reaches x is x itself, still. (What a nearer still surface hides in the frame,
// such as a wall's streak sliding out from behind a ball the camera holds still, no tap can find: there the streak stops
// short at the ball's edge.) Each tap stands for |v_n| / n pixels of path, so
// a pixel inside a moving surface gathers a whole one from its own path, and one a moving edge passes the share of
// the open shutter the edge is over it, as a camera has it (McGuire et al. 2012's tiles and depth split, its cone
// weights replaced by the box shutter's own).
import bevy_core_pipeline::fullscreen_vertex_shader::FullscreenVertexOutput;

struct Params {
    open: f32,
    taps: u32,
    tile: u32,
    frame: u32,
}

@group(0) @binding(0) var colour_tex: texture_2d<f32>;
// the prepasses, multisampled under MSAA: textureLoad's 0 is then their first sample, as it's the mip otherwise
@if(MULTISAMPLED) {
@group(0) @binding(1) var motion_tex: texture_multisampled_2d<f32>;
@group(0) @binding(2) var depth_tex: texture_depth_multisampled_2d;
}
@else {
@group(0) @binding(1) var motion_tex: texture_2d<f32>;
@group(0) @binding(2) var depth_tex: texture_depth_2d;
}
@group(0) @binding(3) var near_tex: texture_2d<f32>;
@group(0) @binding(4) var<uniform> p: Params;

// how much nearer than its depth a surface must be to count as in front of it: a share of the distance
const FRONT_SOFT = 0.02;

// a motion prepass's value (uv a frame) while the shutter is open, in pixels, no longer than two tiles
fn opened_of(mv: vec2<f32>, size: vec2<f32>) -> vec2<f32> {
    let v = mv * size * p.open;
    let l = length(v);
    let most = 2.0 * f32(p.tile);
    return select(v, v * (most / l), l > most);
}

fn opened(q: vec2<i32>, size: vec2<f32>) -> vec2<f32> {
    return opened_of(textureLoad(motion_tex, q, 0).xy, size);
}

// the pixel's own depth and motion: under MSAA its nearest sample's, so an edge pixel whose colour is part the
// moving surface counts as that surface (its first sample alone, the background behind, drew a ring where the
// surface's edge was: the pixel kept its part-surface colour as still background)
struct Own {
    depth: f32,
    motion: vec2<f32>,
}
fn own(x: vec2<i32>) -> Own {
    var o = Own(textureLoad(depth_tex, x, 0), textureLoad(motion_tex, x, 0).xy);
    @if(MULTISAMPLED) {
        for (var s = 1; s < i32(textureNumSamples(depth_tex)); s++) {
            let z = textureLoad(depth_tex, x, s);
            if z > o.depth { o = Own(z, textureLoad(motion_tex, x, s).xy); }
        }
    }
    return o;
}

// 1 where depth a (reverse Z: larger is nearer, 0 the sky) is in front of depth b by FRONT_SOFT of b's distance or
// more, 0 where it's as far or farther
fn in_front(a: f32, b: f32) -> f32 {
    return saturate((a - b) / (FRONT_SOFT * max(a, b) + 1e-30));
}

// 1 per pixel of path a motion of length l spreads its light over, where its path reaches d pixels away (an edge a
// pixel soft); a motion shorter than a tap's stretch counts as the tap's own
fn spread(l: f32, d: f32, stretch: f32) -> f32 {
    return saturate(0.5 + 0.5 * l - d) / max(l, stretch);
}

// interleaved gradient noise (Jimenez 2014), moved each frame: where along its stretch each tap falls
fn noise(q: vec2<i32>, frame: u32) -> f32 {
    let x = vec2<f32>(q) + 5.588238 * f32(frame % 64u);
    return fract(52.9829189 * fract(0.06711056 * x.x + 0.00583715 * x.y));
}

@fragment
fn gather(in: FullscreenVertexOutput) -> @location(0) vec4<f32> {
    let dims = vec2<i32>(textureDimensions(colour_tex));
    let size = vec2<f32>(dims);
    let x = vec2<i32>(in.position.xy);
    let centre = textureLoad(colour_tex, x, 0);
    let tiles = vec2<i32>(textureDimensions(near_tex));
    let vn = textureLoad(near_tex, clamp(x / i32(p.tile), vec2(0), tiles - 1), 0).xy;
    let ln = length(vn);
    // nothing near moves a pixel while the shutter is open
    if ln < 1.0 { return centre; }

    let o = own(x);
    let lx = length(opened_of(o.motion, size));
    let zx = o.depth;
    // three taps every two pixels of the path at most: a short blur (a frame played at 120 Hz moves a few pixels)
    // needs no more (one a pixel left the tests' streaks a quarter noisier)
    let n = clamp(u32(ceil(1.5 * ln)), 2u, max(p.taps, 2u));
    let stretch = ln / f32(n);
    let j = noise(x, p.frame);
    var sum = vec3(0.0);
    var got = 0.0;
    for (var i = 0u; i < n; i++) {
        let along = vn * ((f32(i) + j) / f32(n) - 0.5);
        let y = vec2<i32>(floor(vec2<f32>(x) + 0.5 + along));
        if any(y < vec2(0)) || any(y >= dims) { continue; }
        let d = length(along);
        let front = in_front(textureLoad(depth_tex, y, 0), zx);
        let ly = length(opened(y, size));
        let w = stretch * (front * spread(ly, d, stretch) + (1.0 - front) * spread(lx, d, stretch));
        sum += w * textureLoad(colour_tex, y, 0).rgb;
        got += w;
    }
    // the rest of the open shutter is the pixel itself
    let rgb = (sum + max(1.0 - got, 0.0) * centre.rgb) / max(got, 1.0);
    return vec4(rgb, centre.a);
}
