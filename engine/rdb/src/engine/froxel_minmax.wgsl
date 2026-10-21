// The hero lights' shadow maps as a min-max pyramid (froxel.rs, read by froxel_hero.wgsl's walk): for each 4 x 4,
// 16 x 16, 64 x 64 and 256 x 256 block of texels of each layer, the nearest and farthest depth stored over it and its
// one-texel border (the hardware 2x2 filter at a cell of the block reads its corner texels, one past its last), so the
// walk can take a whole block as lit or as shadowed when the depth it compares at is in front of or behind all of
// it, and goes down to smaller blocks, and to texels, only where a shadow's edge crosses it. Chen et al.'s min-max
// tree (2011), as a 2D pyramid over the map rather than a 1D tree per epipolar line: one build serves every line.

@group(0) @binding(0) var shadows: texture_depth_2d_array;
@group(0) @binding(1) var nearest: sampler;
@group(0) @binding(2) var blocks4: texture_storage_2d_array<rg32float, write>;
@group(0) @binding(5) var blocks16: texture_storage_2d_array<rg32float, write>;

// a workgroup's 32 x 32 texels and the row and column past them, as 17 x 17 gathers of 2 x 2 (each texel read once)
var<workgroup> quads: array<vec2<f32>, 289>;
// its 8 x 8 blocks of 4
var<workgroup> fours: array<vec2<f32>, 64>;

// level4 and level16 at once: a block of 4 is texels [4i, 4i + 4] each way (5 x 5, read as 3 x 3 gathers: a texel
// more, which only widens the range, so a block taken as lit or shadowed still is); a block of 16, the 4 x 4 blocks of
// 4 inside it, whose borders reach its own
@compute @workgroup_size(16, 16, 1)
fn level4(@builtin(workgroup_id) group: vec3<u32>, @builtin(local_invocation_id) local: vec3<u32>) {
    let size = vec2<f32>(textureDimensions(shadows));
    let layer = i32(group.z);
    let origin = vec2<f32>(group.xy) * 32.0;
    // the 289 gathers, 256 threads: the first 33 take a second
    for (var i = local.y * 16u + local.x; i < 289u; i += 256u) {
        let q = vec2<f32>(f32(i % 17u), f32(i / 17u));
        let g = textureGather(shadows, nearest, (origin + q * 2.0 + 1.0) / size, layer);
        quads[i] = vec2(min(min(g.x, g.y), min(g.z, g.w)), max(max(g.x, g.y), max(g.z, g.w)));
    }
    workgroupBarrier();
    if (local.x < 8u && local.y < 8u) {
        var lo = 1e30;
        var hi = -1e30;
        for (var y = 0u; y < 3u; y += 1u) {
            for (var x = 0u; x < 3u; x += 1u) {
                let m = quads[(local.y * 2u + y) * 17u + local.x * 2u + x];
                lo = min(lo, m.x);
                hi = max(hi, m.y);
            }
        }
        fours[local.y * 8u + local.x] = vec2(lo, hi);
        let cell = vec2<i32>(group.xy * 8u + local.xy);
        if (all(cell < vec2<i32>(textureDimensions(blocks4)))) {
            textureStore(blocks4, cell, layer, vec4(lo, hi, 0.0, 0.0));
        }
    }
    workgroupBarrier();
    if (local.x < 2u && local.y < 2u) {
        var lo = 1e30;
        var hi = -1e30;
        for (var y = 0u; y < 4u; y += 1u) {
            for (var x = 0u; x < 4u; x += 1u) {
                let m = fours[(local.y * 4u + y) * 8u + local.x * 4u + x];
                lo = min(lo, m.x);
                hi = max(hi, m.y);
            }
        }
        let cell = vec2<i32>(group.xy * 2u + local.xy);
        if (all(cell < vec2<i32>(textureDimensions(blocks16)))) {
            textureStore(blocks16, cell, layer, vec4(lo, hi, 0.0, 0.0));
        }
    }
}

// (its own bindings: one module, two layouts)
@group(0) @binding(3) var fine: texture_2d_array<f32>;
@group(0) @binding(4) var coarse: texture_storage_2d_array<rg32float, write>;

// reduce: a level's blocks from the four by four of the level below inside each, whose borders reach its own
@compute @workgroup_size(8, 8, 1)
fn reduce(@builtin(global_invocation_id) id: vec3<u32>) {
    let out = textureDimensions(coarse);
    if (id.x >= out.x || id.y >= out.y) {
        return;
    }
    let size = vec2<i32>(textureDimensions(fine));
    let layer = i32(id.z);
    let first = vec2<i32>(id.xy) * 4;
    var lo = 1e30;
    var hi = -1e30;
    for (var y = 0; y < 4; y += 1) {
        for (var x = 0; x < 4; x += 1) {
            let m = textureLoad(fine, min(first + vec2(x, y), size - 1), layer, 0).xy;
            lo = min(lo, m.x);
            hi = max(hi, m.y);
        }
    }
    textureStore(coarse, vec2<i32>(id.xy), layer, vec4(lo, hi, 0.0, 0.0));
}
