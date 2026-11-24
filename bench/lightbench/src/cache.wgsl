// LightBench's world radiance cache (LB7, joined after light.wgsl and trace.wgsl): a hash table of cells over the
// scene's surfaces, each the size of its distance from the camera (base size at 1 m, doubling with each doubling of
// distance) and one of six normal directions, holding the irradiance there. A pixel's bounce ray reads the cell it
// hits instead of tracing on, and adds a fresh sample to it: that point's direct light plus one more ray, which brings
// the next surface's cell; so each frame the cache carries light one bounce further, and a pixel gets every bounce
// for the price of two rays. After the pixels, `resolve` folds each cell's samples into its light and frees cells no
// ray has touched for a while. The idea of NVIDIA's SHaRC and Bevy Solari's world cache.

// a cell is six words: its key (0 empty), the last frame a ray touched it, then this frame's summed light (r, g, b, in
// 1/FIX steps) and how many samples
@group(0) @binding(14) var<storage, read_write> cells: array<atomic<u32>>;
// each cell's light: irradiance (rgb) and how many samples it holds (w)
@group(0) @binding(15) var<storage, read_write> cell_light: array<vec4<f32>>;

const CELLS: u32 = 262144u;
const WORDS: u32 = 6u;
const FIX: f32 = 256.0;
// a sample brighter than this is clipped as it goes in (a light right against a surface), so a sum can't overflow
const CELL_MAX: f32 = 64.0;
// frames a cell lives without a ray touching it
const CELL_LIFE: u32 = 120u;

// the cell at a point: (where its probe starts, its key; never 0)
fn cell_key(pos: vec3<f32>, n: vec3<f32>) -> vec2<u32> {
    let level = u32(clamp(ceil(log2(max(length(pos - p.eye.xyz), 1.0))), 0.0, 15.0));
    let q = bitcast<vec3<u32>>(vec3<i32>(floor(pos / (p.cache.x * exp2(f32(level))))));
    let a = abs(n);
    let axis = select(select(2u, 1u, a.y >= a.z), 0u, a.x >= a.y && a.x >= a.z);
    let dir = axis * 2u + select(0u, 1u, n[axis] < 0.0);
    let at = pcg(q.x ^ pcg(q.y ^ pcg(q.z ^ pcg(level * 8u + dir))));
    let key = pcg(level * 8u + dir + pcg(q.z + pcg(q.y + pcg(q.x + 0x2545f491u)))) | 1u;
    return vec2(at, key);
}

// the cell's slot, made if `make` and there's room along its probe; CELLS if it has none
fn cell(pos: vec3<f32>, n: vec3<f32>, make: bool) -> u32 {
    let h = cell_key(pos, n);
    for (var k = 0u; k < 8u; k++) {
        let i = (h.x + k) % CELLS;
        var key = atomicLoad(&cells[i * WORDS]);
        if key == 0u {
            if !make {
                return CELLS;
            }
            let r = atomicCompareExchangeWeak(&cells[i * WORDS], 0u, h.y);
            // a weak exchange can fail with the slot still empty: try again next frame rather than probe past it
            key = select(r.old_value, h.y, r.exchanged);
            if key == 0u {
                return CELLS;
            }
        }
        if key == h.y {
            atomicMax(&cells[i * WORDS + 1u], p.size.w);
            return i;
        }
    }
    return CELLS;
}

fn cell_add(i: u32, e: vec3<f32>) {
    let v = vec3<u32>(min(e, vec3(CELL_MAX)) * FIX + 0.5);
    atomicAdd(&cells[i * WORDS + 2u], v.x);
    atomicAdd(&cells[i * WORDS + 3u], v.y);
    atomicAdd(&cells[i * WORDS + 4u], v.z);
    atomicAdd(&cells[i * WORDS + 5u], 1u);
}

// the nearest surface along d from o: its point, its normal facing the ray, its albedo; w of the first 0 on a miss
struct Surface { pos: vec4<f32>, n: vec3<f32>, albedo: vec3<f32> }

fn surface(o: vec3<f32>, d: vec3<f32>) -> Surface {
    let h = closest(o, d, 1e4);
    if h.i == 0xFFFFFFFFu {
        return Surface(vec4(0.0), vec3(0.0), vec3(0.0));
    }
    let v0 = tris[3u * h.i];
    let e1 = tris[3u * h.i + 1u];
    let e2 = tris[3u * h.i + 2u];
    var n = normalize(cross(e1.xyz, e2.xyz));
    n = select(n, -n, dot(n, d) > 0.0);
    return Surface(vec4(o + d * h.t, 1.0), n, vec3(v0.w, e1.w, e2.w));
}

// a pixel's bounced light by way of the cache: one ray to the nearest surface, whose cell's light it takes (or the
// fresh sample, until the cell holds any); the fresh sample goes into the cell
fn cached(n0: vec3<f32>, o0: vec3<f32>) -> vec3<f32> {
    let s1 = surface(o0, cosine_dir(n0));
    if s1.pos.w == 0.0 {
        return PI * p.sky.rgb;
    }
    let o1 = s1.pos.xyz + s1.n * (2e-3 + 5e-4 * length(s1.pos.xyz - p.eye.xyz));
    var e = lit_at(s1.pos.xyz, s1.n, o1, 1u);
    let s2 = surface(o1, cosine_dir(s1.n));
    if s2.pos.w == 0.0 {
        e += PI * p.sky.rgb;
    } else {
        let j = cell(s2.pos.xyz, s2.n, false);
        if j < CELLS {
            e += s2.albedo * cell_light[j].rgb;
        }
    }
    let i = cell(s1.pos.xyz, s1.n, true);
    if i == CELLS {
        return s1.albedo * e;
    }
    cell_add(i, e);
    let held = cell_light[i];
    return s1.albedo * select(e, held.rgb, held.w > 0.0);
}

// after the pixels: each cell's samples folded into its light (a running mean over at most p.cache.y samples), and a
// cell no ray has touched in CELL_LIFE frames freed
@compute @workgroup_size(64, 1, 1)
fn resolve(@builtin(global_invocation_id) id: vec3<u32>) {
    let i = id.x;
    if i >= CELLS || atomicLoad(&cells[i * WORDS]) == 0u {
        return;
    }
    let b = i * WORDS;
    let n = atomicLoad(&cells[b + 5u]);
    if p.size.w - atomicLoad(&cells[b + 1u]) > CELL_LIFE {
        atomicStore(&cells[b], 0u);
        cell_light[i] = vec4(0.0);
    } else if n > 0u {
        let mean = vec3(f32(atomicLoad(&cells[b + 2u])), f32(atomicLoad(&cells[b + 3u])), f32(atomicLoad(&cells[b + 4u]))) / (FIX * f32(n));
        let was = cell_light[i];
        let held = min(was.w, p.cache.y);
        cell_light[i] = vec4(mix(was.rgb, mean, f32(n) / (f32(n) + held)), held + f32(n));
    }
    for (var k = 2u; k < WORDS; k++) {
        atomicStore(&cells[b + k], 0u);
    }
}
