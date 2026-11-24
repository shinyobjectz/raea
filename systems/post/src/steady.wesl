// raea's temporal filter (steady.rs; docs/realism.feature LT12): this frame blended with the frames before it, the
// past fetched where each pixel's surface was and kept inside the spread of its neighbourhood now.
import bevy_core_pipeline::fullscreen_vertex_shader::FullscreenVertexOutput;

@group(0) @binding(0) var now_tex: texture_2d<f32>;
@group(0) @binding(1) var past_tex: texture_2d<f32>;
@group(0) @binding(2) var motion_tex: texture_2d<f32>;
@group(0) @binding(3) var depth_tex: texture_depth_2d;
@group(0) @binding(4) var lin: sampler;

struct Params {
    /// the smallest share of this frame a still pixel takes, and a moving one's at `px` pixels a frame or more
    still: f32,
    moving: f32,
    px: f32,
    /// 1: no history yet (the view's first frame, or a new size)
    reset: u32,
}
@group(0) @binding(5) var<uniform> p: Params;

struct Out {
    @location(0) colour: vec4<f32>,
    /// this frame blended with the past (rgb, linear) and how many frames the pixel has been in view (a)
    @location(1) past: vec4<f32>,
}

fn peak(c: vec3<f32>) -> f32 { return max(c.r, max(c.g, c.b)); }
// HDR into 0-1 and back, for the neighbourhood's box, so one bright pixel doesn't stretch it (the blend itself is
// linear, as MSAA's resolve is: blended squashed, a bright roof slope and its shaded one averaged a quarter too dark)
fn squash(c: vec3<f32>) -> vec3<f32> { return c / (1.0 + peak(c)); }
fn unsquash(c: vec3<f32>) -> vec3<f32> { return c / max(1.0 - peak(c), 1e-4); }
fn to_ycocg(c: vec3<f32>) -> vec3<f32> {
    return vec3(0.25 * c.r + 0.5 * c.g + 0.25 * c.b, 0.5 * c.r - 0.5 * c.b, -0.25 * c.r + 0.5 * c.g - 0.25 * c.b);
}
fn to_rgb(c: vec3<f32>) -> vec3<f32> { return vec3(c.x + c.y - c.z, c.x + c.z, c.x - c.y - c.z); }

// the history at uv through a Catmull-Rom filter, from five bilinear fetches (its four corner weights are small and
// left out), so it doesn't blur a little more every frame
fn past_at(uv: vec2<f32>, size: vec2<f32>) -> vec3<f32> {
    let pos = uv * size;
    let c = floor(pos - 0.5) + 0.5;
    let f = pos - c;
    let w0 = f * (-0.5 + f * (1.0 - 0.5 * f));
    let w1 = 1.0 + f * f * (-2.5 + 1.5 * f);
    let w2 = f * (0.5 + f * (2.0 - 1.5 * f));
    let w3 = f * f * (-0.5 + 0.5 * f);
    let w12 = w1 + w2;
    let t0 = (c - 1.0) / size;
    let t3 = (c + 2.0) / size;
    let t12 = (c + w2 / w12) / size;
    var s = textureSampleLevel(past_tex, lin, vec2(t12.x, t0.y), 0.0).rgb * (w12.x * w0.y);
    s += textureSampleLevel(past_tex, lin, vec2(t0.x, t12.y), 0.0).rgb * (w0.x * w12.y);
    s += textureSampleLevel(past_tex, lin, t12, 0.0).rgb * (w12.x * w12.y);
    s += textureSampleLevel(past_tex, lin, vec2(t3.x, t12.y), 0.0).rgb * (w3.x * w12.y);
    s += textureSampleLevel(past_tex, lin, vec2(t12.x, t3.y), 0.0).rgb * (w12.x * w3.y);
    let w = w12.x * w0.y + w0.x * w12.y + w12.x * w12.y + w3.x * w12.y + w12.x * w3.y;
    return max(s / w, vec3(0.0));
}

@fragment
fn fragment(in: FullscreenVertexOutput) -> Out {
    let dims = vec2<i32>(textureDimensions(now_tex));
    let size = vec2<f32>(dims);
    let px = vec2<i32>(in.uv * size);
    let here = textureLoad(now_tex, px, 0);
    var out: Out;
    out.colour = here;
    out.past = vec4(here.rgb, 1.0);
    if (p.reset != 0u) {
        return out;
    }
    // the 3x3 neighbourhood now: its mean and spread, and the nearest surface among the pixel and its four diagonal
    // neighbours (reverse z: larger is nearer), whose motion moves an edge with what's in front of it
    var best = px;
    var near = textureLoad(depth_tex, px, 0);
    var m1 = vec3(0.0);
    var m2 = vec3(0.0);
    for (var y = -1; y <= 1; y++) {
        for (var x = -1; x <= 1; x++) {
            let q = clamp(px + vec2(x, y), vec2(0), dims - 1);
            let c = to_ycocg(squash(textureLoad(now_tex, q, 0).rgb));
            m1 += c;
            m2 += c * c;
            if (x != 0 && y != 0) {
                let d = textureLoad(depth_tex, q, 0);
                if (d > near) {
                    near = d;
                    best = q;
                }
            }
        }
    }
    let motion = textureLoad(motion_tex, best, 0).xy;
    let then = in.uv - motion;
    if (any(then != saturate(then))) {
        return out;
    }
    let fast = saturate(length(motion * size) / p.px);
    let mean = m1 / 9.0;
    // a slow pixel keeps more of its past (a looser box), a fast one less
    let box = sqrt(max(m2 / 9.0 - mean * mean, vec3(0.0))) * mix(1.5, 1.0, fast);
    // the past clipped toward the box's centre along one line, not per channel, so its colour isn't shifted
    let v = to_ycocg(squash(past_at(then, size))) - mean;
    let worst = max(max(abs(v.x) / max(box.x, 1e-5), abs(v.y) / max(box.y, 1e-5)), abs(v.z) / max(box.z, 1e-5));
    let past = unsquash(clamp(to_rgb(mean + v / max(worst, 1.0)), vec3(0.0), vec3(0.999)));
    // frames in view: one more than the pixel had, capped where its rate floors, and fewer the further its past was
    // outside the box (a surface just come into view starts its average again)
    let age = textureLoad(past_tex, clamp(vec2<i32>(then * size), vec2(0), dims - 1), 0).a;
    let n = max(min(age + 1.0, 1.0 / mix(p.still, p.moving, fast)) / max(worst, 1.0), 1.0);
    let blended = mix(past, here.rgb, 1.0 / n);
    out.colour = vec4(blended, here.a);
    out.past = vec4(blended, n);
    return out;
}
