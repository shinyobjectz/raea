// The froxel grid's integration and its apply (froxel.rs). integrate: each column of cells front to back from the
// light pass's atlas into the 3D grid, each texel what light has gathered (rgb) and what of the scene still shows (a)
// at its cell's far side. apply: each pixel reads the grid at its depth (a tricubic B-spline across the frame, linear
// in depth) and is blended as scene x a + rgb.

#import bevy_core_pipeline::fullscreen_vertex_shader::FullscreenVertexOutput

struct Params {
    view_from_clip: mat4x4<f32>,
    grid: vec4<u32>,
    slab: vec4<f32>,
    viewport: vec4<f32>,
    ambient: vec4<f32>,
    counts: vec4<u32>,
    heroes: vec4<u32>,
}

@group(0) @binding(0) var<uniform> params: Params;
@group(0) @binding(1) var atlas: texture_2d<f32>;
@group(0) @binding(2) var grid_out: texture_storage_3d<rgba16float, write>;
@group(0) @binding(3) var grid: texture_3d<f32>;
@group(0) @binding(4) var grid_sampler: sampler;
#ifdef MULTISAMPLED
@group(0) @binding(5) var depth_texture: texture_depth_multisampled_2d;
#else
@group(0) @binding(5) var depth_texture: texture_depth_2d;
#endif

// the far side of slice s (the last's is the grid's far depth)
fn boundary(s: f32) -> f32 {
    return params.slab.y * exp(params.slab.w * (s + 1.0) / f32(params.grid.z));
}

@compute @workgroup_size(8, 8, 1)
fn integrate(@builtin(global_invocation_id) id: vec3<u32>) {
    let g = params.grid;
    if (id.x >= g.x || id.y >= g.y) {
        return;
    }
    // metres along the ray per metre of view depth, through this column's centre
    let pixel = (vec2<f32>(id.xy) + 0.5) * params.slab.x;
    let ndc = vec2(pixel.x / params.viewport.z * 2.0 - 1.0, 1.0 - pixel.y / params.viewport.w * 2.0);
    let far_point = params.view_from_clip * vec4(ndc, 1.0, 1.0);
    let ray = far_point.xyz / far_point.w;
    let stretch = length(ray) / abs(ray.z);
    var light = vec3(0.0);
    var shows = 1.0;
    // the first slice runs from the camera
    var front = 0.0;
    for (var s = 0u; s < g.z; s += 1u) {
        let back = boundary(f32(s));
        let len = (back - front) * stretch;
        front = back;
        let cell = textureLoad(atlas, vec2(id.x + (s % g.w) * g.x, id.y + (s / g.w) * g.y), 0);
        let sigma_t = cell.a;
        let through = exp(-sigma_t * len);
        // Hillaire's step: the light scattered along the cell, dimmed by what of the cell lies in front of it
        if (sigma_t > 1e-7) {
            light += shows * cell.rgb * (1.0 - through) / sigma_t;
        } else {
            light += shows * cell.rgb * len;
        }
        shows *= through;
        textureStore(grid_out, vec3(id.xy, s), vec4(light, shows));
    }
}

// the grid at a point (normalized texel coordinates), cubic B-spline across the frame (four bilinear taps: Sigg and
// Hadwiger's, GPU Gems 2 ch. 20) and linear in depth (the hardware's)
fn sample_grid(uv: vec2<f32>, w: f32) -> vec4<f32> {
    let size = vec2<f32>(vec2(params.grid.x, params.grid.y));
    let t = uv * size - 0.5;
    let i = floor(t);
    let f = t - i;
    let f2 = f * f;
    let f3 = f2 * f;
    // the B-spline's weights and the offsets of two linear taps that make them
    let w0 = (1.0 / 6.0) * (-f3 + 3.0 * f2 - 3.0 * f + 1.0);
    let w1 = (1.0 / 6.0) * (3.0 * f3 - 6.0 * f2 + 4.0);
    let w2 = (1.0 / 6.0) * (-3.0 * f3 + 3.0 * f2 + 3.0 * f + 1.0);
    let w3 = (1.0 / 6.0) * f3;
    let g0 = w0 + w1;
    let g1 = w2 + w3;
    let h0 = (i - 0.5 + w1 / g0) / size;
    let h1 = (i + 1.5 + w3 / g1) / size;
    let a = textureSampleLevel(grid, grid_sampler, vec3(h0.x, h0.y, w), 0.0);
    let b = textureSampleLevel(grid, grid_sampler, vec3(h1.x, h0.y, w), 0.0);
    let c = textureSampleLevel(grid, grid_sampler, vec3(h0.x, h1.y, w), 0.0);
    let d = textureSampleLevel(grid, grid_sampler, vec3(h1.x, h1.y, w), 0.0);
    return g0.y * (g0.x * a + g1.x * b) + g1.y * (g0.x * c + g1.x * d);
}

@fragment
fn apply(in: FullscreenVertexOutput) -> @location(0) vec4<f32> {
    let px = in.position.xy;
    let ndc_depth = textureLoad(depth_texture, vec2<i32>(px), 0);
    // the pixel's view depth (nothing drawn there: past the grid)
    var depth = params.slab.z * 2.0;
    if (ndc_depth > 0.0) {
        let p = params.view_from_clip * vec4(0.0, 0.0, ndc_depth, 1.0);
        depth = -p.z / p.w;
    }
    let uv = (px - params.viewport.xy) / (params.slab.x * vec2<f32>(vec2(params.grid.x, params.grid.y)));
    let first = boundary(0.0);
    if (depth < first) {
        // inside the first slice: from nothing at the camera to the first texel
        return mix(vec4(0.0, 0.0, 0.0, 1.0), sample_grid(uv, 0.5 / f32(params.grid.z)), max(depth, 0.0) / first);
    }
    // texel s holds the far side of slice s, at log depth s + 1
    let c = f32(params.grid.z) * log(depth / params.slab.y) / params.slab.w - 1.0;
    return sample_grid(uv, (c + 0.5) / f32(params.grid.z));
}
