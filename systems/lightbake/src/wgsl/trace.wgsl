// The light that bounces, per texel (docs/bake.feature BK3): paths from the texel's origin through the model's own
// triangles (one BLAS, one TLAS) by wgpu's ray queries. What a texel keeps is the mean light its cosine-weighted rays
// bring back, so the lightmap times a surface's albedo is the light it sends on. At each hit the surface's emission
// is added and the path goes on weighted by its albedo (Lambert, diffuse.wgsl); a miss sees the sky; after the
// second bounce Russian roulette ends paths by their weight, unbiased. Passes add to `accum` (rgb, samples).

struct Params {
    sky: vec4<f32>,
    count: u32,
    pass_index: u32,
    per_pass: u32,
    bounces: u32,
    offset: f32,
    // the first texel of this dispatch (a bake's texels go in chunks)
    first: u32,
    _pad1: f32,
    _pad2: f32,
}

struct Surface {
    albedo: vec4<f32>,
    emission: vec4<f32>,
}

@group(0) @binding(0) var tlas: acceleration_structure;
@group(0) @binding(1) var<storage, read> positions: array<vec4<f32>>;
@group(0) @binding(2) var<storage, read> indices: array<u32>;
@group(0) @binding(3) var<storage, read> surfaces: array<Surface>;
// per texel: origin (w: 1 when inside other geometry) and normal
@group(0) @binding(4) var<storage, read> texels: array<vec4<f32>>;
@group(0) @binding(5) var<storage, read_write> accum: array<vec4<f32>>;
@group(0) @binding(6) var<uniform> params: Params;

// The nearest hit. Its own function: a ray_query declared in a loop is zeroed again each turn, which naga writes for
// Metal as an assignment Metal's intersection_query deletes.
fn nearest(o: vec3<f32>, d: vec3<f32>) -> RayIntersection {
    var rq: ray_query;
    rayQueryInitialize(&rq, tlas, RayDesc(0u, 0xffu, 0.0, 1.0e30, o, d));
    while (rayQueryProceed(&rq)) {}
    return rayQueryGetCommittedIntersection(&rq);
}

@compute @workgroup_size(64)
fn main(@builtin(global_invocation_id) gid: vec3<u32>) {
    let i = params.first + gid.x;
    if (i >= params.count) {
        return;
    }
    let at = texels[2u * i];
    if (at.w > 0.5) {
        return;
    }
    let n0 = texels[2u * i + 1u].xyz;
    var sum = vec3<f32>(0.0);
    for (var s = 0u; s < params.per_pass; s = s + 1u) {
        var rng = raea_hash(i * 0x9e3779b9u ^ raea_hash(params.pass_index * params.per_pass + s));
        var o = at.xyz;
        var n = n0;
        var weight = vec3<f32>(1.0);
        var light = vec3<f32>(0.0);
        for (var b = 0u; b < params.bounces; b = b + 1u) {
            let d = raea_diffuse_sample(n, vec2<f32>(raea_rand(&rng), raea_rand(&rng)));
            let hit = nearest(o, d);
            if (hit.kind == RAY_QUERY_INTERSECTION_NONE) {
                light = light + weight * params.sky.rgb;
                break;
            }
            let p = hit.primitive_index;
            let srf = surfaces[p];
            light = light + weight * srf.emission.rgb;
            let a = positions[indices[3u * p]].xyz;
            let bb = positions[indices[3u * p + 1u]].xyz;
            let c = positions[indices[3u * p + 2u]].xyz;
            var ng = normalize(cross(bb - a, c - a));
            if (dot(ng, d) > 0.0) {
                ng = -ng;
            }
            let w = hit.barycentrics;
            o = a * (1.0 - w.x - w.y) + bb * w.x + c * w.y + ng * params.offset;
            n = ng;
            weight = weight * srf.albedo.rgb;
            if (b >= 2u) {
                let q = max(weight.r, max(weight.g, weight.b));
                if (raea_rand(&rng) >= q) {
                    break;
                }
                weight = weight / q;
            }
        }
        sum = sum + light;
    }
    accum[i] = accum[i] + vec4<f32>(sum, f32(params.per_pass));
}
