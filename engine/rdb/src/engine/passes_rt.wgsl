// A frame's traced passes (passes.rs, rt.rs): each pixel's point and normal from the prepass, rays from it against the
// scene's TLAS. A fraction written as a byte, as the linear value the frame's sRGB output encodes back to that byte.
enable wgpu_ray_query;

#import bevy_core_pipeline::fullscreen_vertex_shader::FullscreenVertexOutput

struct Params {
    view_from_clip: mat4x4<f32>,
    world_from_clip: mat4x4<f32>,
    sun: vec4<f32>,
    frame: vec4<u32>,
}

@group(0) @binding(0) var<uniform> params: Params;
@group(0) @binding(1) var depth_texture: texture_depth_2d;
@group(0) @binding(2) var normal_texture: texture_2d<f32>;
@group(0) @binding(3) var kept: texture_2d<f32>;
@group(0) @binding(4) var tlas: acceleration_structure;

const PI = 3.14159265;

fn byte(f: f32) -> vec4<f32> {
    let b = round(clamp(f, 0.0, 1.0) * 255.0) / 255.0;
    let l = select(pow((b + 0.055) / 1.055, 2.4), b / 12.92, b <= 0.04045);
    return vec4(l, l, l, 1.0);
}

// PCG, for each point's own turn of its ray pattern
fn hash(v: u32) -> u32 {
    let s = v * 747796405u + 2891336453u;
    let w = ((s >> ((s >> 28u) + 4u)) ^ s) * 277803737u;
    return (w >> 22u) ^ w;
}
fn unit(h: u32) -> f32 {
    return f32(h >> 8u) / 16777216.0;
}

struct Surface {
    p: vec3<f32>,
    n: vec3<f32>,
    ok: bool,
}

fn surface(px: vec2<f32>) -> Surface {
    let ip = vec2<i32>(px);
    let d = textureLoad(depth_texture, ip, 0);
    var s: Surface;
    s.ok = d > 0.0;
    let size = vec2<f32>(textureDimensions(depth_texture));
    let ndc = vec2((px.x / size.x) * 2.0 - 1.0, 1.0 - (px.y / size.y) * 2.0);
    let w = params.world_from_clip * vec4(ndc, d, 1.0);
    s.p = w.xyz / w.w;
    s.n = normalize(textureLoad(normal_texture, ip, 0).xyz * 2.0 - 1.0);
    return s;
}

// a point's turn, keyed to where it is in the world (2 mm cells), not to its pixel: the ray pattern's noise stays on the
// surface as the camera moves instead of sliding over it
fn world_turn(p: vec3<f32>) -> f32 {
    let c = vec3<i32>(floor(p * 500.0));
    return unit(hash(u32(c.x) * 73856093u ^ hash(u32(c.y) * 19349663u ^ hash(u32(c.z) * 83492791u)))) * 2.0 * PI;
}

fn basis(n: vec3<f32>) -> mat3x3<f32> {
    let up = select(vec3(1.0, 0.0, 0.0), vec3(0.0, 1.0, 0.0), abs(n.y) < 0.9);
    let t = normalize(cross(up, n));
    return mat3x3(t, cross(n, t), n);
}

// rt.rs's instance masks: CASTS (it casts shadows), ALL
const CASTS = 1u;
const ALL = 2u;

// is anything (of `mask`) in the way from p along dir within t_max
fn blocked(p: vec3<f32>, dir: vec3<f32>, t_max: f32, mask: u32) -> bool {
    var rq: ray_query;
    rayQueryInitialize(&rq, tlas, RayDesc(RAY_FLAG_TERMINATE_ON_FIRST_HIT, mask, 0.0005, t_max, p, dir));
    rayQueryProceed(&rq);
    return rayQueryGetCommittedIntersection(&rq).kind != RAY_QUERY_INTERSECTION_NONE;
}

// how much of the sun's disc the point sees: rays spread evenly over the disc (a stratified spiral turned per pixel)
@fragment
fn sun(in: FullscreenVertexOutput) -> @location(0) vec4<f32> {
    let s = surface(in.position.xy);
    if (!s.ok) {
        return byte(1.0);
    }
    let l = normalize(params.sun.xyz);
    if (dot(s.n, l) <= 0.0) {
        return byte(0.0);
    }
    let b = basis(l);
    let n = max(params.frame.y, 1u);
    let turn = world_turn(s.p);
    let sin_max = sqrt(1.0 - params.sun.w * params.sun.w);
    let origin = s.p + s.n * 0.002;
    var open = 0u;
    for (var k = 0u; k < n; k++) {
        let r = sqrt((f32(k) + 0.5) / f32(n)) * sin_max;
        let a = f32(k) * 2.39996323 + turn;
        let dir = normalize(b * vec3(r * cos(a), r * sin(a), 1.0));
        if (!blocked(origin, dir, 1000.0, CASTS)) {
            open++;
        }
    }
    return byte(f32(open) / f32(n));
}

// how much of the hemisphere is open: cosine-weighted rays out to the reach (params.frame.z, mm)
@fragment
fn sky(in: FullscreenVertexOutput) -> @location(0) vec4<f32> {
    let s = surface(in.position.xy);
    if (!s.ok) {
        return byte(1.0);
    }
    let b = basis(s.n);
    let n = max(params.frame.y, 1u);
    let turn = world_turn(s.p);
    let origin = s.p + s.n * 0.002;
    var open = 0u;
    for (var k = 0u; k < n; k++) {
        let r = sqrt((f32(k) + 0.5) / f32(n));
        let a = f32(k) * 2.39996323 + turn;
        let dir = normalize(b * vec3(r * cos(a), r * sin(a), sqrt(max(1.0 - r * r, 0.0))));
        if (!blocked(origin, dir, f32(params.frame.z) / 1000.0, ALL)) {
            open++;
        }
    }
    return byte(f32(open) / f32(n));
}
