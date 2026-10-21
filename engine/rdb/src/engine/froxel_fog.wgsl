// The froxel grid as a material reads it (froxel_glass.wgsl, fire.wgsl): bound at 100-102 of the material's group
// (froxel_glass.rs FogBindings), and the fog in front of a point of a surface drawn after the grid's apply pass.
#define_import_path rdb::froxel_fog

#import bevy_pbr::mesh_view_bindings::view

@group(#{MATERIAL_BIND_GROUP}) @binding(100) var froxel_grid: texture_3d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(101) var froxel_sampler: sampler;
// texel 0: cells across, down, slices, tile (px); 1: near, far, ln(far / near), 1 when the grid is drawn this frame;
// 2: the viewport (froxel.rs prepare_textures writes it each frame)
@group(#{MATERIAL_BIND_GROUP}) @binding(102) var froxel_params: texture_2d<f32>;

// froxel_grid.wgsl's sample_grid: cubic B-spline across the frame in four bilinear taps, linear in depth
fn sample_grid(uv: vec2<f32>, w: f32, size: vec2<f32>) -> vec4<f32> {
    let t = uv * size - 0.5;
    let i = floor(t);
    let f = t - i;
    let f2 = f * f;
    let f3 = f2 * f;
    let w0 = (1.0 / 6.0) * (-f3 + 3.0 * f2 - 3.0 * f + 1.0);
    let w1 = (1.0 / 6.0) * (3.0 * f3 - 6.0 * f2 + 4.0);
    let w2 = (1.0 / 6.0) * (-3.0 * f3 + 3.0 * f2 + 3.0 * f + 1.0);
    let w3 = (1.0 / 6.0) * f3;
    let g0 = w0 + w1;
    let g1 = w2 + w3;
    let h0 = (i - 0.5 + w1 / g0) / size;
    let h1 = (i + 1.5 + w3 / g1) / size;
    let a = textureSampleLevel(froxel_grid, froxel_sampler, vec3(h0.x, h0.y, w), 0.0);
    let b = textureSampleLevel(froxel_grid, froxel_sampler, vec3(h1.x, h0.y, w), 0.0);
    let c = textureSampleLevel(froxel_grid, froxel_sampler, vec3(h0.x, h1.y, w), 0.0);
    let d = textureSampleLevel(froxel_grid, froxel_sampler, vec3(h1.x, h1.y, w), 0.0);
    return g0.y * (g0.x * a + g1.x * b) + g1.y * (g0.x * c + g1.x * d);
}

// the fog in front of a point: the light it has gathered (rgb) and what of the point shows (a), as the apply pass
// reads it for a pixel at that depth
fn fog_at(px: vec2<f32>, world: vec3<f32>) -> vec4<f32> {
    let g = textureLoad(froxel_params, vec2(0, 0), 0);
    let slab = textureLoad(froxel_params, vec2(1, 0), 0);
    let viewport = textureLoad(froxel_params, vec2(2, 0), 0);
    if (slab.w < 0.5) {
        return vec4(0.0, 0.0, 0.0, 1.0);
    }
    let depth = -(view.view_from_world * vec4(world, 1.0)).z;
    let size = g.xy;
    let uv = (px - viewport.xy) / (g.w * size);
    let first = slab.x * exp(slab.z / g.z);
    if (depth < first) {
        return mix(vec4(0.0, 0.0, 0.0, 1.0), sample_grid(uv, 0.5 / g.z, size), max(depth, 0.0) / first);
    }
    let c = g.z * log(depth / slab.x) / slab.z - 1.0;
    return sample_grid(uv, (c + 0.5) / g.z, size);
}
