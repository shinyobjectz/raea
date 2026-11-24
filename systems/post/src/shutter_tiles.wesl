// raea's motion blur, its tiles (shutter.rs): the longest motion in each tile of the frame (across each tile's rows,
// then down them: two passes of 20 reads, where one of 400 kept most of the GPU waiting, 1.1 ms of 1.9 at 1080p), then
// in each tile's 3x3 neighbourhood, so a pixel the motion of something near it can reach knows it's there, however
// still it is itself.

struct Params {
    /// the share of a frame the shutter is open
    open: f32,
    /// taps along the motion, per pixel
    taps: u32,
    /// a tile's side in pixels: also the farthest a pixel's light is carried each way
    tile: u32,
    /// the frame, the taps' noise
    frame: u32,
}

// the motion prepass (tile_rows: multisampled under MSAA, its first sample read), or the pass before's tiles
@if(MULTISAMPLED) {
@group(0) @binding(0) var input_tex: texture_multisampled_2d<f32>;
}
@else {
@group(0) @binding(0) var input_tex: texture_2d<f32>;
}
@group(0) @binding(1) var<uniform> p: Params;

// a pixel's motion while the shutter is open, in pixels: the motion prepass's (where it was a frame before to where it
// is, in uv) times the share the shutter is open, no longer than two tiles (a tile each way)
fn opened(mv: vec2<f32>, size: vec2<f32>) -> vec2<f32> {
    let v = mv * size * p.open;
    let l = length(v);
    let most = 2.0 * f32(p.tile);
    return select(v, v * (most / l), l > most);
}

// the longest motion along each row of each tile: one texel a tile across, a row of the frame down
@fragment
fn tile_rows(@builtin(position) at: vec4<f32>) -> @location(0) vec4<f32> {
    let size = vec2<i32>(textureDimensions(input_tex));
    let k = i32(p.tile);
    let row = vec2<i32>(at.xy);
    var best = vec2(0.0);
    for (var x = 0; x < k; x++) {
        let q = vec2(row.x * k + x, row.y);
        if q.x >= size.x { break; }
        let v = opened(textureLoad(input_tex, q, 0).xy, vec2<f32>(size));
        if dot(v, v) > dot(best, best) { best = v; }
    }
    return vec4(best, 0.0, 1.0);
}

// each tile's longest motion: the longest of its rows'
@fragment
fn tile_max(@builtin(position) at: vec4<f32>) -> @location(0) vec4<f32> {
    let size = vec2<i32>(textureDimensions(input_tex));
    let k = i32(p.tile);
    let tile = vec2<i32>(at.xy);
    var best = vec2(0.0);
    for (var y = 0; y < k; y++) {
        let q = vec2(tile.x, tile.y * k + y);
        if q.y >= size.y { break; }
        let v = textureLoad(input_tex, q, 0).xy;
        if dot(v, v) > dot(best, best) { best = v; }
    }
    return vec4(best, 0.0, 1.0);
}

// each tile's longest motion among its own and its eight neighbours'
@fragment
fn neighbour_max(@builtin(position) at: vec4<f32>) -> @location(0) vec4<f32> {
    let tiles = vec2<i32>(textureDimensions(input_tex));
    let c = vec2<i32>(at.xy);
    var best = vec2(0.0);
    for (var y = -1; y <= 1; y++) {
        for (var x = -1; x <= 1; x++) {
            let q = c + vec2(x, y);
            if any(q < vec2(0)) || any(q >= tiles) { continue; }
            let v = textureLoad(input_tex, q, 0).xy;
            if dot(v, v) > dot(best, best) { best = v; }
        }
    }
    return vec4(best, 0.0, 1.0);
}
