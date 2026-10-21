// A flame drawn as a volume (fire.rs): each pixel of the fire's box marches its ray through the box and adds up the
// light of a flame that is a pure function of the point and the row's time. The flame is a tapering envelope (wide at
// the foot, a point at the top) whose edge is pushed in and out by rising, domain-warped 3D gradient noise, more so
// the higher it is, which tears its top into tongues; its temperature sets its colour, deep red at the edges, the
// row's colour in the body and yellow-white in the core. Everything is seeded by the row, nothing by the frame count
// or a previous frame, so a frame rendered alone equals the same frame in play.

#import bevy_pbr::forward_io::VertexOutput
#import bevy_pbr::mesh_view_bindings::view
#import rdb::froxel_fog::fog_at
#ifdef DEPTH_PREPASS
#import bevy_pbr::prepass_utils
#endif

struct FireParams {
    center: vec3<f32>,
    time: f32,
    size: vec3<f32>,
    intensity: f32,
    color: vec4<f32>,
    seed: f32,
    speed: f32,
    steps: f32,
    opacity: f32,
};

@group(#{MATERIAL_BIND_GROUP}) @binding(0) var<uniform> p: FireParams;

// pcg3d (Jarzynski and Olano 2020), as a gradient in -1..1
fn grad(c: vec3<i32>) -> vec3<f32> {
    var v = bitcast<vec3<u32>>(c) * 1664525u + 1013904223u;
    v.x += v.y * v.z;
    v.y += v.z * v.x;
    v.z += v.x * v.y;
    v = v ^ (v >> vec3<u32>(16u));
    v.x += v.y * v.z;
    v.y += v.z * v.x;
    v.z += v.x * v.y;
    return vec3<f32>(v) * (2.0 / 4294967295.0) - 1.0;
}

// 3D gradient noise, about -0.8..0.8
fn noise(x: vec3<f32>) -> f32 {
    let i = vec3<i32>(floor(x));
    let f = fract(x);
    let u = f * f * f * (f * (f * 6.0 - 15.0) + 10.0);
    let a = dot(grad(i), f);
    let b = dot(grad(i + vec3<i32>(1, 0, 0)), f - vec3<f32>(1.0, 0.0, 0.0));
    let c = dot(grad(i + vec3<i32>(0, 1, 0)), f - vec3<f32>(0.0, 1.0, 0.0));
    let d = dot(grad(i + vec3<i32>(1, 1, 0)), f - vec3<f32>(1.0, 1.0, 0.0));
    let e = dot(grad(i + vec3<i32>(0, 0, 1)), f - vec3<f32>(0.0, 0.0, 1.0));
    let g = dot(grad(i + vec3<i32>(1, 0, 1)), f - vec3<f32>(1.0, 0.0, 1.0));
    let h = dot(grad(i + vec3<i32>(0, 1, 1)), f - vec3<f32>(0.0, 1.0, 1.0));
    let k = dot(grad(i + vec3<i32>(1, 1, 1)), f - vec3<f32>(1.0, 1.0, 1.0));
    return mix(mix(mix(a, b, u.x), mix(c, d, u.x), u.y), mix(mix(e, g, u.x), mix(h, k, u.x), u.y), u.z);
}

// three octaves, each turned so their grids don't line up
fn fbm(x0: vec3<f32>) -> f32 {
    let m = mat3x3<f32>(0.00, 0.80, 0.60, -0.80, 0.36, -0.48, -0.60, -0.48, 0.64);
    var x = x0;
    var s = 0.0;
    var a = 0.5;
    for (var i = 0; i < 3; i++) {
        s += a * noise(x);
        x = m * x * 2.03;
        a *= 0.5;
    }
    return s;
}

// the flame at q (q.y from the foot (0) to the top of the box (1), q.xz across it (-1..1)): x how much of it is
// flame (sharp at its edge), y how hot it is (highest deep inside, low down)
fn flame(q: vec3<f32>, rise: f32, sway_t: f32) -> vec2<f32> {
    let h = q.y;
    if h <= 0.0 || h >= 1.0 {
        return vec2<f32>(0.0);
    }
    let s = p.seed * 7.31 + 3.1;
    // the column leans with slow noise, more toward the top
    // (sines with seeded phases: noise here ran for every sample, even those far outside the flame)
    let u = h * 1.1 - sway_t;
    let sway = vec2<f32>(sin(u * 2.3 + s) + 0.5 * sin(u * 5.1 + s * 1.7), sin(u * 1.9 + s * 2.9) + 0.5 * sin(u * 4.3 + s * 0.6)) * 0.2 * h;
    let r = length(q.xz - sway);
    // the envelope: most of the box's width at the foot, a point at the top
    let env = 0.62 * pow(1.0 - h, 0.85);
    let amp = 0.2 + 0.8 * h;
    if r > env + amp * 0.7 {
        return vec2<f32>(0.0);
    }
    // noise rising through the flame, its domain warped by a coarser noise so the tongues curl
    // the noise's height grows faster than the flame's toward the top, so the same rise in noise space moves the base
    // about three times as fast as the tips: quick licks at the foot, slow tongues above
    let hy = 3.0 * h * (0.45 + 0.55 * h);
    let w = vec3<f32>(q.x * 2.0, hy - rise, q.z * 2.0) + vec3<f32>(s, s * 0.37, s * 1.9);
    // (across only: a warp along the rise would just shift it)
    let warp = vec3<f32>(noise(w * 0.8 + vec3<f32>(0.0, 0.0, 11.0)), 0.0, noise(w * 0.8 + vec3<f32>(9.1, 0.0, 7.7)));
    let n = fbm(w * 2.3 + warp * 1.2);
    let d = env - r + n * amp;
    // outside the flame's edge: nothing here, and the tearing noise below needn't be looked up
    if d <= 0.0 {
        return vec2<f32>(0.0);
    }
    let soft = 0.006 + 0.022 * (1.0 - h);
    let foot = smoothstep(0.0, 0.1, h);
    // higher up, the flame tears into separate tongues and licks that rise and go out: a coarse rising noise opens
    // gaps in it, wider the higher they are
    let tear = noise(w * vec3<f32>(0.9, 0.55, 0.9) + vec3<f32>(17.0, 0.0, 3.0)) + 0.5 - 0.75 * h;
    let m = clamp(d / soft, 0.0, 1.0) * smoothstep(0.0, 0.04, tear) * foot * (1.0 - smoothstep(0.8, 1.0, h));
    // hottest deep in the body and low down, cooling as it rises
    let heat = clamp(d / (0.03 + 0.3 * env), 0.0, 1.0) * pow(1.0 - h, 0.7) * foot;
    return vec2<f32>(m, heat);
}

// a temperature's light: deep red at the edge, the row's colour in the body, yellow-white in the core
fn glow(t: f32) -> vec3<f32> {
    let base = p.color.rgb;
    let peak = max(max(base.r, base.g), base.b);
    // saturated, since a filmic tone map (agx) pales bright orange toward cream
    var c = mix(base * vec3<f32>(0.8, 0.25, 0.1), base, smoothstep(0.0, 0.5, t));
    c = mix(c, vec3<f32>(1.0, 0.4, 0.07) * peak, smoothstep(0.45, 0.8, t));
    // the core: white-yellow and well past 1 in HDR, so a <Bloom> blooms it
    // (only the hottest few percent of the body, so a small fire isn't all core and white)
    let core = smoothstep(0.85, 1.0, t);
    return c * mix(0.3, 1.8, t * t) + vec3<f32>(1.0, 0.82, 0.5) * peak * 6.0 * core * core;
}

@fragment
fn fragment(in: VertexOutput) -> @location(0) vec4<f32> {
    let ro = view.world_position;
    let rd = normalize(in.world_position.xyz - ro);
    let hs = p.size * 0.5;
    let lo = p.center - hs;
    let hi = p.center + hs;
    // where the ray enters and leaves the box
    let inv = 1.0 / rd;
    let ta = (lo - ro) * inv;
    let tb = (hi - ro) * inv;
    let tn = min(ta, tb);
    let tf = max(ta, tb);
    let t0 = max(max(max(tn.x, tn.y), tn.z), 0.0);
    var t1 = min(min(tf.x, tf.y), tf.z);
#ifdef DEPTH_PREPASS
    // stop at the scene's surface: a log inside the box hides the flame behind it
    let depth = prepass_utils::prepass_depth(in.position, 0u);
    if depth > 0.0 {
        let uv = (in.position.xy - view.viewport.xy) / view.viewport.zw;
        let w = view.world_from_clip * vec4<f32>(uv.x * 2.0 - 1.0, 1.0 - uv.y * 2.0, depth, 1.0);
        t1 = min(t1, dot(w.xyz / w.w - ro, rd));
    }
#endif
    if t1 <= t0 {
        discard;
    }
    let n = max(p.steps, 1.0);
    let dt = (t1 - t0) / n;
    // each pixel starts a fixed fraction of a step in (interleaved gradient noise of the pixel, the same every frame)
    let j = fract(52.9829189 * fract(dot(floor(in.position.xy), vec2<f32>(0.06711056, 0.00583715))));
    // a big fire's tongues rise and sway more slowly than a candle's
    let tt = p.time * p.speed / sqrt(max(p.size.y, 0.05));
    let rise = tt * 2.4;
    let sway_t = tt * 0.7;
    let width = max(min(p.size.x, p.size.z), 1e-3);
    // how much light a metre of the hottest flame gives, so `intensity` is about what a ray through the core sees,
    // and how much of what's behind it the flame dims
    let k = p.intensity / width;
    let sigma = 3.5 * p.opacity / width;
    var light = vec3<f32>(0.0);
    var trans = 1.0;
    // where along the ray the flame's light comes from (weighted by it), for the froxel fog in front of it
    var lt = 0.0;
    var lw = 0.0;
    for (var i = 0; i < i32(n); i++) {
        let t = t0 + (f32(i) + j) * dt;
        let x = ro + rd * t;
        let q = vec3<f32>((x.x - p.center.x) / hs.x, (x.y - lo.y) / p.size.y, (x.z - p.center.z) / hs.z);
        let f = flame(q, rise, sway_t);
        if f.x > 0.0 {
            light += glow(f.y) * (f.x * k * trans * dt);
            lt += f.x * trans * t;
            lw += f.x * trans;
            trans *= exp(-f.x * sigma * dt);
            // nothing further along the ray shows through
            if trans < 0.005 {
                break;
            }
        }
    }
    // where next to nothing of the flame is, nothing is drawn, so its box's depth is written only where it shows
    if trans > 0.97 && max(max(light.r, light.g), light.b) < 0.01 * p.intensity {
        discard;
    }
    // like a material's emissive (Bevy's emissive_exposure_weight 0): not scaled by the camera's exposure
    // dimmed by the froxel fog in front of it, which lays its own light over the flame's body
    let fog = fog_at(in.position.xy, ro + rd * select(0.5 * (t0 + t1), lt / lw, lw > 0.0));
    return vec4<f32>(light * fog.a + fog.rgb * (1.0 - trans), 1.0 - trans);
}
