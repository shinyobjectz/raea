// LightBench's lighting (docs/lightbench.feature LB1, LB3): each pixel of Bevy's G-buffer (the depth prepass and the
// deferred prepass's albedo and normal) lit by point lights, every light (the reference) or K of them chosen at random,
// each through one shadow ray in LightBench's own BVH. Both use `unshadowed` and `visible`, so they differ only in
// which lights they look at. With --bounce (LB5) each pixel also follows a cosine-chosen ray through the BVH to the
// nearest surface and lights that point the same way (one light sample in the sampled modes, every light in the
// reference), so the reference and the sampled path still differ only in the lights they look at. The result goes
// into the camera's HDR main texture, before Bevy's tonemapping.

struct Params {
    world_from_clip: mat4x4<f32>,
    // xyz: the camera, w: exposure
    eye: vec4<f32>,
    // width, height, lights, frame
    size: vec4<u32>,
    // mode (0 every light, 1 uniform, 2 weighted), K, view (0 lit, 1 normal, 2 albedo, 3 depth, 4 rays, 5 history,
    // 6 lod, 7 bounce: the bounced light alone),
    // brute force
    mode: vec4<u32>,
    // the previous frame's camera, to find where a point was
    prev_clip_from_world: mat4x4<f32>,
    // xyz: the previous camera
    prev_eye: vec4<f32>,
    // raw (no history), reset (a camera cut, or the first frame), the history's length in frames, 0
    temporal: vec4<u32>,
    // the light grid: its corner (xyz) and cell size (w); its cells along x, y, z, and whether the sampled modes use it
    grid_min: vec4<f32>,
    grid_dims: vec4<u32>,
    // diffuse bounces a pixel follows (0 none), whether its bounce reads the radiance cache instead (cache.wgsl),
    // whether the bounce is traced at half resolution and upsampled (half.wgsl), 0
    gi: vec4<u32>,
    // the sun (LB6): xyz the unit direction toward it, w its angular radius in radians; its irradiance (rgb, 0 off)
    sun: vec4<f32>,
    sun_e: vec4<f32>,
    // the sky's radiance (rgb): what a bounce ray that leaves the scene brings back
    sky: vec4<f32>,
    // the radiance cache: a cell's size at 1 m from the camera, the samples a cell holds at most, 0, 0
    cache: vec4<f32>,
}

struct Node { mn: vec3<f32>, a: u32, mx: vec3<f32>, count: u32 }
struct Light { pos: vec3<f32>, range: f32, power: vec3<f32>, radius: f32 }

@group(0) @binding(0) var<uniform> p: Params;
@group(0) @binding(1) var depth_tex: texture_depth_2d;
@group(0) @binding(2) var gbuf: texture_2d<u32>;
@group(0) @binding(3) var<storage, read> nodes: array<Node>;
// three a triangle: v0, v1 - v0, v2 - v0; their w the triangle's albedo (linear r, g, b), for the bounce
@group(0) @binding(4) var<storage, read> tris: array<vec4<f32>>;
@group(0) @binding(5) var<storage, read> lights: array<Light>;
@group(0) @binding(6) var out: texture_storage_2d<rgba16float, write>;
// the light so far at each pixel (rgb, before albedo) and how many frames it holds (a); the surface it was for
// (its normal, and its distance from that frame's camera); last frame's to read, this frame's to write
@group(0) @binding(7) var hist_in: texture_2d<f32>;
@group(0) @binding(8) var geo_in: texture_2d<f32>;
// (in f32: a long history's 1 / n steps are below an f16's resolution, and its mean stops moving)
@group(0) @binding(9) var hist_out: texture_storage_2d<rgba32float, write>;
@group(0) @binding(10) var geo_out: texture_storage_2d<rgba16float, write>;
// this frame's shadow rays, lit pixels and the most rays any pixel traced (LB4's rays a pixel)
@group(0) @binding(11) var<storage, read_write> stats: array<atomic<u32>, 4>;

// the light grid's two lists in one: cell c's lights are grid[grid[c]..grid[c + 1]] (its starts count from the buffer's
// beginning, the lights' indices following the starts)
@group(0) @binding(12) var<storage, read> grid: array<u32>;

var<workgroup> wg_rays: atomic<u32>;
var<workgroup> wg_px: atomic<u32>;
var<workgroup> wg_max: atomic<u32>;

const PI: f32 = 3.14159265;
const MAX_K: u32 = 16u;
const STACK: u32 = 48u;

var<private> seed: u32;
var<private> rays: u32;

// Bevy's octahedral_decode (bevy_render utils), for the deferred prepass's normal
fn oct_decode(v: vec2<f32>) -> vec3<f32> {
    let f = v * 2.0 - 1.0;
    var n = vec3(f.xy, 1.0 - abs(f.x) - abs(f.y));
    let t = saturate(-n.z);
    let w = select(vec2(t), vec2(-t), n.xy >= vec2(0.0));
    n = vec3(n.xy + w, n.z);
    return normalize(n);
}

// a light's light at a point before its shadow: its power times the cosine and the falloff (an inverse square that
// stops growing inside its radius and fades to nothing at its range); all zero where it can't reach
fn unshadowed(l: Light, pos: vec3<f32>, n: vec3<f32>) -> vec3<f32> {
    let v = l.pos - pos;
    let d2 = max(dot(v, v), 1e-8);
    let d = sqrt(d2);
    let ndl = dot(n, v / d);
    if ndl <= 0.0 || d >= l.range {
        return vec3(0.0);
    }
    let x = d / l.range;
    let win = saturate(1.0 - x * x * x * x);
    return l.power * (ndl * win * win / max(d2, l.radius * l.radius));
}

fn visible(o: vec3<f32>, l: Light) -> f32 {
    let v = l.pos - o;
    let d = length(v);
    return select(1.0, 0.0, occluded(o, v / d, d - 1e-3));
}

// the candidate lights at a point: (first, count) into grid_idx, or with the grid off every light (count with first
// = 0xFFFFFFFF); outside the grid no light reaches
fn candidates(pos: vec3<f32>) -> vec2<u32> {
    if p.grid_dims.w == 0u {
        return vec2(0xFFFFFFFFu, p.size.z);
    }
    let c = vec3<i32>(floor((pos - p.grid_min.xyz) / p.grid_min.w));
    let d = vec3<i32>(p.grid_dims.xyz);
    if any(c < vec3(0)) || any(c >= d) {
        return vec2(0u, 0u);
    }
    let cell = u32((c.z * d.y + c.y) * d.x + c.x);
    return vec2(grid[cell], grid[cell + 1u] - grid[cell]);
}

fn candidate(range: vec2<u32>, j: u32) -> u32 {
    return select(grid[range.x + j], j, range.x == 0xFFFFFFFFu);
}

fn lum(c: vec3<f32>) -> f32 {
    return dot(c, vec3(0.2126, 0.7152, 0.0722));
}

// the light reaching a point (pos, its normal n, o just off it) from the lights: every light with its shadow ray, or
// k shadow rays to lights chosen uniformly or by their light from the point's cell
fn lit_at(pos: vec3<f32>, n: vec3<f32>, o: vec3<f32>, k: u32) -> vec3<f32> {
    let nl = p.size.z;
    var c = vec3(0.0);
    if p.mode.x == 0u {
        for (var i = 0u; i < nl; i++) {
            let u = unshadowed(lights[i], pos, n);
            if lum(u) > 0.0 {
                c += u * visible(o, lights[i]);
            }
        }
    } else if p.mode.x == 1u {
        // uniform: each candidate 1 / their number
        let cand = candidates(pos);
        let m = cand.y;
        if m > 0u {
            for (var s = 0u; s < k; s++) {
                let i = candidate(cand, min(u32(rnd() * f32(m)), m - 1u));
                let u = unshadowed(lights[i], pos, n);
                if lum(u) > 0.0 {
                    c += u * visible(o, lights[i]) * f32(m);
                }
            }
            c /= f32(k);
        }
    } else {
        // weighted: K reservoirs over every light by its unshadowed light, so any light that adds light can be chosen
        var pick: array<u32, MAX_K>;
        var pw: array<f32, MAX_K>;
        var wsum = 0.0;
        let cand = candidates(pos);
        for (var j = 0u; j < cand.y; j++) {
            let i = candidate(cand, j);
            let wi = lum(unshadowed(lights[i], pos, n));
            if wi <= 0.0 {
                continue;
            }
            wsum += wi;
            for (var s = 0u; s < k; s++) {
                if rnd() * wsum < wi {
                    pick[s] = i;
                    pw[s] = wi;
                }
            }
        }
        if wsum > 0.0 {
            for (var s = 0u; s < k; s++) {
                let l = lights[pick[s]];
                c += unshadowed(l, pos, n) * visible(o, l) * (wsum / pw[s]);
            }
            c /= f32(k);
        }
    }
    // the sun, the same in every mode: one shadow ray to a point on its disc, so its shadows soften with its size
    let ns = dot(n, p.sun.xyz);
    if ns > 0.0 && lum(p.sun_e.rgb) > 0.0 && !occluded(o, cone_dir(p.sun.xyz, cos(p.sun.w)), 1e4) {
        c += p.sun_e.rgb * ns;
    }
    return c;
}

// the light reaching a point by way of other surfaces, p.gi.x bounces deep: from the point a cosine-chosen ray to the
// nearest triangle, that point lit as a pixel is (one light sample, or every light in the reference), and on from
// there. A cosine-chosen ray's pdf cancels the cosine and the 1 / PI of the hit's diffuse light, so each bounce adds
// the albedos so far times the light at the hit. A ray that leaves the scene brings the sky: PI times its radiance,
// for the same reason.
fn bounced(n0: vec3<f32>, o0: vec3<f32>) -> vec3<f32> {
    var e = vec3(0.0);
    var through = vec3(1.0);
    var n = n0;
    var o = o0;
    for (var b = 0u; b < p.gi.x; b++) {
        let d = cosine_dir(n);
        let h = closest(o, d, 1e4);
        if h.i == 0xFFFFFFFFu {
            e += through * PI * p.sky.rgb;
            break;
        }
        let v0 = tris[3u * h.i];
        let e1 = tris[3u * h.i + 1u];
        let e2 = tris[3u * h.i + 2u];
        var nh = normalize(cross(e1.xyz, e2.xyz));
        if dot(nh, d) > 0.0 {
            nh = -nh;
        }
        let hp = o + d * h.t;
        let oh = hp + nh * (2e-3 + 5e-4 * length(hp - p.eye.xyz));
        through *= vec3(v0.w, e1.w, e2.w);
        e += through * lit_at(hp, nh, oh, 1u);
        n = nh;
        o = oh;
    }
    return e;
}

// one pixel: its light, its history; (shadow rays traced, 1 if it is a lit pixel)
fn shade(id: vec3<u32>) -> vec2<u32> {
    if id.x >= p.size.x || id.y >= p.size.y {
        return vec2(0u);
    }
    let px = vec2<i32>(id.xy);
    let depth = textureLoad(depth_tex, px, 0);
    // the sky (reverse Z: far is 0) keeps Bevy's clear colour, and holds no history
    if depth <= 0.0 {
        textureStore(hist_out, px, vec4(0.0));
        textureStore(geo_out, px, vec4(0.0, 0.0, 0.0, -1.0));
        return vec2(0u);
    }
    let g = textureLoad(gbuf, px, 0);
    let albedo = pow(unpack4x8unorm(g.r).rgb, vec3(2.2));
    let n = oct_decode(vec2(f32(g.a & 0xFFFu), f32((g.a >> 12u) & 0xFFFu)) / 4095.0);
    let uv = (vec2<f32>(id.xy) + 0.5) / vec2<f32>(p.size.xy);
    let w = p.world_from_clip * vec4(uv.x * 2.0 - 1.0, 1.0 - uv.y * 2.0, depth, 1.0);
    let pos = w.xyz / w.w;
    let dist = length(pos - p.eye.xyz);
    let view = p.mode.z;
    if view == 1u {
        textureStore(out, px, vec4(n * 0.5 + 0.5, 1.0));
        return vec2(0u);
    }
    if view == 2u || view == 6u {
        textureStore(out, px, vec4(albedo, 1.0));
        return vec2(0u);
    }
    if view == 3u {
        textureStore(out, px, vec4(vec3(dist / 20.0), 1.0));
        return vec2(0u);
    }
    seed = pcg(id.x + id.y * p.size.x + p.size.w * p.size.x * p.size.y);
    rays = 0u;
    // off the surface along its normal, more the farther it is (depth precision)
    let o = pos + n * (2e-3 + 5e-4 * dist);
    var c = vec3(0.0);
    if view != 7u {
        c = lit_at(pos, n, o, clamp(p.mode.y, 1u, MAX_K));
    }
    if p.gi.z != 0u {
        c += upsampled(px, pos, n, dist);
    } else if p.gi.y != 0u {
        c += cached(n, o);
    } else if p.gi.x > 0u {
        c += bounced(n, o);
    }
    // the history: where this point was last frame, kept only if the same surface was there
    var held = 0.0;
    var acc = c;
    if p.temporal.x == 0u && p.temporal.y == 0u {
        let was = p.prev_clip_from_world * vec4(pos, 1.0);
        let ndc = was.xy / was.w;
        let at = vec2<i32>(floor(vec2(ndc.x * 0.5 + 0.5, 0.5 - ndc.y * 0.5) * vec2<f32>(p.size.xy)));
        if was.w > 0.0 && all(at >= vec2(0)) && all(at < vec2<i32>(p.size.xy)) {
            let geo = textureLoad(geo_in, at, 0);
            let expect = length(pos - p.prev_eye.xyz);
            if geo.w > 0.0 && abs(geo.w - expect) < 0.02 * expect + 0.01 && dot(geo.xyz, n) > 0.9 {
                let h = textureLoad(hist_in, at, 0);
                held = min(h.a, f32(p.temporal.z) - 1.0);
                acc = mix(h.rgb, c, 1.0 / (held + 1.0));
            }
        }
    }
    textureStore(hist_out, px, vec4(acc, held + 1.0));
    textureStore(geo_out, px, vec4(n, dist));
    if view == 4u {
        textureStore(out, px, vec4(vec3(f32(rays) / 16.0), 1.0));
        return vec2(rays, 1u);
    }
    if view == 5u {
        textureStore(out, px, vec4(vec3((held + 1.0) / f32(p.temporal.z)), 1.0));
        return vec2(rays, 1u);
    }
    textureStore(out, px, vec4(acc * albedo / PI * p.eye.w, 1.0));
    return vec2(rays, 1u);
}

@compute @workgroup_size(8, 8, 1)
fn main(@builtin(global_invocation_id) id: vec3<u32>, @builtin(local_invocation_index) li: u32) {
    let r = shade(id);
    atomicAdd(&wg_rays, r.x);
    atomicAdd(&wg_px, r.y);
    atomicMax(&wg_max, r.x);
    workgroupBarrier();
    if li == 0u {
        atomicAdd(&stats[0], atomicLoad(&wg_rays));
        atomicAdd(&stats[1], atomicLoad(&wg_px));
        atomicMax(&stats[2], atomicLoad(&wg_max));
    }
}
