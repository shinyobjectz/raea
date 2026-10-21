// Volumetric fog at 1/FOG_FACTOR resolution (fog_half.rs): the depth downsample before the march and the
// depth-guided upsample after it. Pure functions of the frame's depth and fog: no history.
#import bevy_core_pipeline::fullscreen_vertex_shader::FullscreenVertexOutput

// the march's output: in-scattered light (rgb) and what of the scene behind still shows (a), one texel a block
@group(0) @binding(0) var fog_tex: texture_2d<f32>;
// the depth (NDC, reverse Z) each texel's march stopped at
@group(0) @binding(1) var half_depth: texture_2d<f32>;
#ifdef MULTISAMPLED
@group(0) @binding(2) var full_depth: texture_depth_multisampled_2d;
#else
@group(0) @binding(2) var full_depth: texture_depth_2d;
#endif

const K: i32 = i32(#{FOG_FACTOR});

fn depth_at(c: vec2<i32>) -> f32 {
    let s = vec2<i32>(textureDimensions(full_depth));
    return textureLoad(full_depth, clamp(c, vec2<i32>(0), s - vec2<i32>(1)), 0);
}

// distance from the camera, up to a factor (depth is near / distance; nothing drawn is 0, far away)
fn dist(d: f32) -> f32 {
    return 1.0 / max(d, 1e-7);
}

// Each texel's depth from its K x K block of pixels, the nearest on even texels of a checkerboard and the farthest
// on odd ones, so that along a silhouette both sides have texels of their own depth to upsample from.
@fragment
fn downsample(in: FullscreenVertexOutput) -> @location(0) vec4<f32> {
    let t = vec2<i32>(floor(in.position.xy));
    let far_side = ((t.x + t.y) & 1) == 1;
    var d = depth_at(t * K);
    for (var y = 0; y < K; y += 1) {
        for (var x = 0; x < K; x += 1) {
            let e = depth_at(t * K + vec2(x, y));
            d = select(max(d, e), min(d, e), far_side);
        }
    }
    return vec4(d, 0.0, 0.0, 0.0);
}

// how alike two distances are: 1 at the same, falling off by 2% apart, so a texel of the wall behind a column
// doesn't lend the column its fog
fn alike(a: f32, b: f32) -> f32 {
    let rel = abs(a - b) / min(a, b);
    return exp(-rel * 50.0);
}

// The fog over the full-resolution frame: each pixel the bilinear mix of its four nearest texels, each weighted by
// how alike its depth is to the pixel's; where none is alike (a sliver thinner than a block), the texel nearest in
// depth of the 3 x 3 around it. Blended as Bevy's fog is: scene * a + rgb.
@fragment
fn composite(in: FullscreenVertexOutput) -> @location(0) vec4<f32> {
    let p = vec2<i32>(floor(in.position.xy));
    let hs = vec2<i32>(textureDimensions(fog_tex)) - vec2<i32>(1);
    let u = in.position.xy / f32(K) - 0.5;
    let i0 = vec2<i32>(floor(u));
    let f = u - floor(u);
    let z = dist(depth_at(p));

    var sum = vec4(0.0);
    var wsum = 0.0;
    for (var j = 0; j < 2; j += 1) {
        for (var i = 0; i < 2; i += 1) {
            let c = clamp(i0 + vec2(i, j), vec2<i32>(0), hs);
            let bw = select(1.0 - f.x, f.x, i == 1) * select(1.0 - f.y, f.y, j == 1);
            let w = bw * alike(z, dist(textureLoad(half_depth, c, 0).r));
            sum += w * textureLoad(fog_tex, c, 0);
            wsum += w;
        }
    }
    if (wsum > 1e-3) {
        return sum / wsum;
    }

    let n = vec2<i32>(round(u));
    var best = clamp(n, vec2<i32>(0), hs);
    var best_rel = 1e30;
    for (var j = -1; j <= 1; j += 1) {
        for (var i = -1; i <= 1; i += 1) {
            let c = clamp(n + vec2(i, j), vec2<i32>(0), hs);
            let zt = dist(textureLoad(half_depth, c, 0).r);
            let rel = abs(z - zt) / min(z, zt);
            if (rel < best_rel) {
                best_rel = rel;
                best = c;
            }
        }
    }
    return textureLoad(fog_tex, best, 0);
}
