// Traced ambient occlusion (traced_ao.rs): how much of each point's hemisphere is open within a reach, cosine-weighted
// rays against the scene's TLAS, written where Bevy's PBR reads its SSAO, so it shadows the environment light alone.
enable wgpu_ray_query;

struct Params {
    world_from_clip: mat4x4<f32>,
    /// rays a pixel, reach (mm)
    frame: vec4<u32>,
}

@group(0) @binding(0) var<uniform> params: Params;
@group(0) @binding(1) var depth_texture: texture_depth_2d;
@group(0) @binding(2) var normal_texture: texture_2d<f32>;
@group(0) @binding(3) var tlas: acceleration_structure;
#ifdef R16FLOAT
@group(0) @binding(4) var ao: texture_storage_2d<r16float, write>;
#else
@group(0) @binding(4) var ao: texture_storage_2d<r32float, write>;
#endif

const PI = 3.14159265;

fn hash(v: u32) -> u32 {
    let s = v * 747796405u + 2891336453u;
    let w = ((s >> ((s >> 28u) + 4u)) ^ s) * 277803737u;
    return (w >> 22u) ^ w;
}
fn unit(h: u32) -> f32 {
    return f32(h >> 8u) / 16777216.0;
}

// a point's turn of its ray pattern, keyed to where it is in the world (2 mm cells), so the noise stays on the surface
fn world_turn(p: vec3<f32>) -> f32 {
    let c = vec3<i32>(floor(p * 500.0));
    return unit(hash(u32(c.x) * 73856093u ^ hash(u32(c.y) * 19349663u ^ hash(u32(c.z) * 83492791u)))) * 2.0 * PI;
}

fn basis(n: vec3<f32>) -> mat3x3<f32> {
    let up = select(vec3(1.0, 0.0, 0.0), vec3(0.0, 1.0, 0.0), abs(n.y) < 0.9);
    let t = normalize(cross(up, n));
    return mat3x3(t, cross(n, t), n);
}

@compute @workgroup_size(8, 8, 1)
fn main(@builtin(global_invocation_id) id: vec3<u32>) {
    let size = textureDimensions(depth_texture);
    if (id.x >= size.x || id.y >= size.y) {
        return;
    }
    let ip = vec2<i32>(id.xy);
    let d = textureLoad(depth_texture, ip, 0);
    if (d <= 0.0) {
        textureStore(ao, ip, vec4(1.0));
        return;
    }
    let px = vec2<f32>(id.xy) + 0.5;
    let ndc = vec2((px.x / f32(size.x)) * 2.0 - 1.0, 1.0 - (px.y / f32(size.y)) * 2.0);
    let w = params.world_from_clip * vec4(ndc, d, 1.0);
    let p = w.xyz / w.w;
    let n = normalize(textureLoad(normal_texture, ip, 0).xyz * 2.0 - 1.0);
    let b = basis(n);
    let rays = max(params.frame.x, 1u);
    let reach = f32(params.frame.y) / 1000.0;
    let turn = world_turn(p);
    let origin = p + n * 0.002;
    var open = 0u;
    for (var k = 0u; k < rays; k++) {
        let r = sqrt((f32(k) + 0.5) / f32(rays));
        let a = f32(k) * 2.39996323 + turn;
        let dir = normalize(b * vec3(r * cos(a), r * sin(a), sqrt(max(1.0 - r * r, 0.0))));
        var rq: ray_query;
        rayQueryInitialize(&rq, tlas, RayDesc(RAY_FLAG_TERMINATE_ON_FIRST_HIT, 0xffu, 0.0005, reach, origin, dir));
        rayQueryProceed(&rq);
        if (rayQueryGetCommittedIntersection(&rq).kind == RAY_QUERY_INTERSECTION_NONE) {
            open++;
        }
    }
    textureStore(ao, ip, vec4(f32(open) / f32(rays)));
}
