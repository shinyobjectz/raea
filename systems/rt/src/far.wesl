// The sun's far shadow map (far.rs; docs/realism.feature LT21): the sun's view in texels, each one ray from above
// everything down along the sun, holding how high along the sun it first hit a shadow caster (-1e9 where it hit
// nothing). Texel (i, j) is the column through u (i + 0.5) + v (j + 0.5) texels, kept at (i mod n, j mod n), so the map
// follows the eye and a move traces only the texels new to it. raea-sys-lod's shadows.wesl reads it past the last
// cascade (raea_far_sun_shadow). A dispatch traces far.region.
enable wgpu_ray_query;

// as raea's bevy_pbr mesh_view_bindings.wesl declares it for the lighting
struct Far {
    /// toward the sun (xyz), metres a texel (w)
    dir: vec4<f32>,
    /// the map's x axis (xyz), the depth bias (w, m)
    u: vec4<f32>,
    /// its y axis (xyz), unused
    v: vec4<f32>,
    /// where along the sun the rays start (x, m) and how far they go (y), unused
    ray: vec4<f32>,
    /// texels a side (x), the texel at the map's centre (yz), unused
    map: vec4<i32>,
    /// the texels traced: the first (xy) and how many (zw)
    region: vec4<i32>,
}

@group(0) @binding(0) var<uniform> far: Far;
@group(0) @binding(1) var tlas: acceleration_structure;
@group(0) @binding(2) var map: texture_storage_2d<r32float, write>;

/// shadow casters only (lib.rs CASTS)
const CASTS: u32 = 1u;

// how far a ray goes to the nearest shadow caster, or -1 (the query in its own function: traced_ao.wesl says why)
fn first_hit(o: vec3<f32>, d: vec3<f32>, reach: f32) -> f32 {
    var rq: ray_query;
    rayQueryInitialize(&rq, tlas, RayDesc(0u, CASTS, 0.0, reach, o, d));
    rayQueryProceed(&rq);
    let hit = rayQueryGetCommittedIntersection(&rq);
    return select(-1.0, hit.t, hit.kind != RAY_QUERY_INTERSECTION_NONE);
}

@compute @workgroup_size(8, 8, 1)
fn main(@builtin(global_invocation_id) id: vec3<u32>) {
    let r = far.region;
    if (i32(id.x) >= r.z || i32(id.y) >= r.w) {
        return;
    }
    let ij = r.xy + vec2<i32>(id.xy);
    let texel = far.dir.w;
    let o = far.u.xyz * ((f32(ij.x) + 0.5) * texel) + far.v.xyz * ((f32(ij.y) + 0.5) * texel) + far.dir.xyz * far.ray.x;
    let t = first_hit(o, -far.dir.xyz, far.ray.y);
    let n = vec2(far.map.x);
    textureStore(map, ((ij % n) + n) % n, vec4(select(-1e9, far.ray.x - t, t >= 0.0)));
}
