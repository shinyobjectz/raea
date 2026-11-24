// raea GI's world cache (cache.rs; docs/realism.feature LT3): a hash grid of cells on the surfaces rays hit, each
// holding the light arriving at it (irradiance, lux: an albedo over pi turns it into the light the surface sends
// back), learned over frames. A cell is found by its place, quantised by a size that doubles with distance from the
// eye, and by its side (the normal, quantised to an octant), so a wall's two faces and a corner's two walls don't
// share. A read that finds no cell claims an empty one, and a read keeps a cell alive: one unread for LIFE frames
// empties (compact.wesl). Each frame some of the live cells learn from one bounce more (cache_update.wesl), so the
// light settles to every bounce over frames. From vendor/bevy_solari's world_cache_query.wesl, made raea's.
//
// Until hits know their surface (LT4) a cell is keyed by the side a ray meets it from: its normal faces the ray.

/// where a cell was claimed, and the normal it was claimed with (w unused)
struct Cell {
    at: vec4<f32>,
    n: vec4<f32>,
}

struct Upkeep {
    /// the eye (xyz)
    eye: vec4<f32>,
    /// the smallest cell (m), the distance over which a cell's size doubles (m), the most frames a cell's mean holds
    size: vec4<f32>,
    /// the frame's salt, how many cells a frame aims to update, 1 to forget every mean (a cut), unused
    run: vec4<u32>,
}

@group(1) @binding(0) var<storage, read_write> checksums: array<atomic<u32>>;
@group(1) @binding(1) var<storage, read_write> life: array<atomic<u32>>;
// what the cell has learned (rgb) and over how many frames (a)
@group(1) @binding(2) var<storage, read_write> radiance: array<vec4<f32>>;
@group(1) @binding(3) var<storage, read_write> cells: array<Cell>;
// how fast a cell's light is changing (a running mean of its luminance step), so a changing cell forgets faster
@group(1) @binding(4) var<storage, read_write> deltas: array<f32>;
// the live cells' list: each cell's place among the live ones in its block (a), each block's first place (b), the
// list, its length and the update's indirect dispatch (compact.wesl writes them)
@group(1) @binding(5) var<storage, read_write> a: array<u32>;
@group(1) @binding(6) var<storage, read_write> b: array<u32>;
@group(1) @binding(7) var<storage, read_write> live: array<u32>;
@group(1) @binding(8) var<storage, read_write> count: u32;
@group(1) @binding(9) var<storage, read_write> dispatch: vec3<u32>;
// each listed cell's new light this frame, before the blend
@group(1) @binding(10) var<storage, read_write> learned: array<vec4<f32>>;
// how much of the time the cell sees the sun, a mean as its light's: the sun's share of its light, which a pixel
// whose raster draws the sun itself takes out (raea-sys-gi)
@group(1) @binding(11) var<storage, read_write> sunlit: array<f32>;
@group(1) @binding(12) var<uniform> upkeep: Upkeep;

/// cells in all (cache.rs CELLS): 1024 blocks of 1024
const CELLS: u32 = 1048576u;
/// the most frames a cell lives unread
const LIFE: u32 = 10u;
/// the most cells tried after a collision
const SEARCH: u32 = 3u;
const EMPTY: u32 = 0u;
/// no cell: one claimed new this frame, or none to be had
const NONE: u32 = 0xffffffffu;

fn hash(v: u32) -> u32 {
    let s = v * 747796405u + 2891336453u;
    let w = ((s >> ((s >> 28u) + 4u)) ^ s) * 277803737u;
    return (w >> 22u) ^ w;
}
// a second hash, independent of the first, for the checksum that tells two places sharing a key apart
fn hash2(v: u32) -> u32 {
    let n = (v << 13u) ^ v;
    return n * (n * n * 15731u + 789221u) + 1376312589u;
}
/// a number in [0, 1) from a running seed
fn rand(seed: ptr<function, u32>) -> f32 {
    *seed = hash(*seed);
    return f32(*seed >> 8u) / 16777216.0;
}

/// A cell's size at a place: the smallest near the eye, doubling every `size.y` metres or so (log2 of the distance),
/// the step to the next size taken at random near the boundary so the sizes blend rather than step.
fn cell_size(at: vec3<f32>, seed: ptr<function, u32>) -> f32 {
    let lod = log2(1.0 + distance(upkeep.eye.xyz, at) / upkeep.size.y);
    let f = fract(lod);
    return upkeep.size.x * exp2(floor(lod) + select(0.0, 1.0, rand(seed) < f * f * f));
}

/// A place and side's key into the cells (where to look first) and its checksum (whether a cell found is this one).
fn key_of(at: vec3<f32>, n: vec3<f32>, size: f32) -> vec2<u32> {
    let p = bitcast<vec3<u32>>(floor(at / size + 0.0001));
    let s = bitcast<vec3<u32>>(floor(n + 0.0001));
    var k = hash(p.x);
    var c = hash2(p.x);
    k = hash(k + p.y);
    c = hash2(c + p.y);
    k = hash(k + p.z);
    c = hash2(c + p.z);
    k = hash(k + s.x);
    c = hash2(c + s.x);
    k = hash(k + s.y);
    c = hash2(c + s.y);
    k = hash(k + s.z);
    c = hash2(c + s.z);
    return vec2(k & (CELLS - 1u), max(c, 1u));
}

/// The light a cell holds at a place and side (rgb) and over how many frames (a: 0 for a cell new this frame, or
/// none to be had), keeping it alive `keep` frames (see `find`).
fn read(at: vec3<f32>, n: vec3<f32>, keep: u32, seed: ptr<function, u32>) -> vec4<f32> {
    let k = find(at, n, keep, seed);
    if (k == NONE) {
        return vec4(0.0);
    }
    return radiance[k];
}

/// The cell at a place and side, kept alive `keep` frames: NONE for one claimed new this frame, or none to be had.
/// The place is jittered across the surface by up to half a cell, so the cache reads as a blur rather than a grid. A
/// cell not found is claimed, empty: some update learns it.
fn find(at_in: vec3<f32>, n: vec3<f32>, keep: u32, seed: ptr<function, u32>) -> u32 {
    var at = at_in;
    var size = cell_size(at, seed);
    let t = normalize(cross(n, select(vec3(1.0, 0.0, 0.0), vec3(0.0, 1.0, 0.0), abs(n.x) > 0.9)));
    let bt = cross(n, t);
    at += ((rand(seed) * 2.0 - 1.0) * t + (rand(seed) * 2.0 - 1.0) * bt) * size * 0.5;
    size = cell_size(at, seed);
    let kc = key_of(at, n, size);
    var key = kc.x;
    for (var i = 0u; i < SEARCH; i++) {
        // a plain load first: a screen's pixels read the same cell by the hundred, and a write each would queue them
        let now = atomicLoad(&checksums[key]);
        if (now == kc.y) {
            if (atomicLoad(&life[key]) < keep) {
                atomicMax(&life[key], keep);
            }
            return key;
        }
        if (now == EMPTY) {
            let cas = atomicCompareExchangeWeak(&checksums[key], EMPTY, kc.y);
            if (cas.exchanged) {
                atomicMax(&life[key], keep);
                cells[key] = Cell(vec4(at, 0.0), vec4(n, 0.0));
                return NONE;
            }
            if (cas.old_value == kc.y) {
                atomicMax(&life[key], keep);
                return key;
            }
            if (cas.old_value == EMPTY) {
                // the weak exchange failed with nothing there: tried again next frame
                return NONE;
            }
        }
        key = (key + 1u) & (CELLS - 1u);
    }
    return NONE;
}
