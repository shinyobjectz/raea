// raea GI's probes (gi.rs; docs/realism.feature LT2, LT3): a grid of probes around the camera, each refreshed by 64
// rays through the scene's TLAS. A ray that misses sees the sky (the camera's environment); one that hits sees its
// instance's record (records.rs, LT4's first tier: averaged albedo and emission; the surface faces the ray), lit by what the world cache has
// learned there (cache.wesl: the sun and every bounce, the sky shadowed by what's around), or, where its cell is new,
// by the sun through one shadow ray and the sky's diffuse light, unshadowed. Each probe keeps a running mean of its
// ambient cube (the light arriving along +-x, +-y, +-z) and writes it into Bevy's irradiance volume, which every
// standard material samples in place of its environment's diffuse light. `learn` and `blend` are the cache's update.
import raea_sys_rt::cache::{read, rand, hash, radiance, cells, deltas, life, live, count, learned, sunlit, upkeep, LIFE};

enable wgpu_ray_query;

struct Params {
    /// the grid's centre (world), the spacing between probes (m)
    grid: vec4<f32>,
    /// probes along x, y, z; probes in all
    res: vec4<u32>,
    /// the first probe this dispatch, rounds each probe traces, the frame's salt, rounds a probe new to its slot traces
    run: vec4<u32>,
    /// toward the sun; 1 when there's a sky to see
    sun_dir: vec4<f32>,
    /// the sun's illuminance times its colour (lux); the sky's intensity
    sun: vec4<f32>,
    /// the turn into the sky cubemap's frame (a quaternion)
    sky_turn: vec4<f32>,
}

@group(0) @binding(0) var<uniform> p: Params;
@group(0) @binding(1) var tlas: acceleration_structure;
@group(0) @binding(2) var sky: texture_cube<f32>;
@group(0) @binding(3) var sky_diffuse: texture_cube<f32>;
@group(0) @binding(4) var sky_sampler: sampler;
// each probe's six faces (rgb and how many rounds its mean holds), kept by its place in the world (`slot`), so the grid
// can move a probe at a time and keep what the probes it still covers have learned
@group(0) @binding(5) var<storage, read_write> cubes: array<vec4<f32>>;
@group(0) @binding(6) var voxels: texture_storage_3d<rgba16float, write>;
// which world probe each slot holds (xyz) and whether it holds one (w), so a slot taken over starts afresh
@group(0) @binding(7) var<storage, read_write> tags: array<vec4<i32>>;
// each TLAS instance's record at its custom index (records.rs): its averaged albedo, and its emission (the radiance
// raster draws it with)
struct Record {
    albedo: vec4<f32>,
    emission: vec4<f32>,
}
@group(0) @binding(8) var<storage, read> records: array<Record>;

const PI = 3.14159265;
const RAYS = 64u;
// the most frames a probe's mean is kept over: after that each frame's rays count a 32nd
const HISTORY = 32.0;
const CASTS = 1u;
const ALL = 2u;

var<workgroup> light: array<vec3<f32>, 64>;
var<workgroup> dirs: array<vec3<f32>, 64>;
var<workgroup> fresh: u32;

// The probe at a place in the grid: its world position, and its slot (its place on a world lattice, wrapped into the
// grid: the same probe keeps its slot while the grid moves around it).
fn probe_at(ijk: vec3<u32>) -> vec3<f32> {
    return p.grid.xyz + ((vec3<f32>(ijk) + 0.5) - vec3<f32>(p.res.xyz) * 0.5) * p.grid.w;
}
// its cell on the world lattice, in integers (the grid's centre is a whole number of spacings: follow() rounds it)
fn cell_of(ijk: vec3<u32>) -> vec3<i32> {
    return vec3<i32>(round(p.grid.xyz / p.grid.w)) - vec3<i32>(p.res.xyz / 2u) + vec3<i32>(ijk);
}
fn slot_of(l: vec3<i32>) -> u32 {
    let r = vec3<i32>(p.res.xyz);
    let s = ((l % r) + r) % r;
    return u32(s.x) + u32(s.y) * p.res.x + u32(s.z) * p.res.x * p.res.y;
}
fn grid_ijk(i: u32) -> vec3<u32> {
    let r = p.res.xyz;
    return vec3<u32>(i % r.x, (i / r.x) % r.y, i / (r.x * r.y));
}

fn unit(h: u32) -> f32 {
    return f32(h >> 8u) / 16777216.0;
}

fn turn(q: vec4<f32>, v: vec3<f32>) -> vec3<f32> {
    return v + 2.0 * cross(q.xyz, cross(q.xyz, v) + q.w * v);
}

// a uniformly random rotation (Shoemake), so each round's 64 directions lie differently and the mean converges
fn random_turn(s: u32) -> vec4<f32> {
    let u1 = unit(hash(s));
    let u2 = unit(hash(s ^ 0x9e3779b9u));
    let u3 = unit(hash(s ^ 0x85ebca6bu));
    let a = sqrt(1.0 - u1);
    let b = sqrt(u1);
    return vec4(a * sin(2.0 * PI * u2), a * cos(2.0 * PI * u2), b * sin(2.0 * PI * u3), b * cos(2.0 * PI * u3));
}

// the i-th of n directions spread evenly over the sphere (spherical Fibonacci)
fn fib(i: u32, n: u32) -> vec3<f32> {
    let z = 1.0 - (2.0 * f32(i) + 1.0) / f32(n);
    let r = sqrt(max(1.0 - z * z, 0.0));
    let a = f32(i) * 2.39996323;
    return vec3(r * cos(a), r * sin(a), z);
}

// The queries live in functions of their own, a fresh one each call (traced_ao.wesl: Metal can't reassign one).
// the first surface along a ray: how far (-1 where the ray reaches the sky), and its instance's record
struct Hit {
    t: f32,
    rec: u32,
}
fn first_hit(o: vec3<f32>, d: vec3<f32>) -> Hit {
    var rq: ray_query;
    rayQueryInitialize(&rq, tlas, RayDesc(0u, ALL, 0.01, 10000.0, o, d));
    rayQueryProceed(&rq);
    let h = rayQueryGetCommittedIntersection(&rq);
    if (h.kind == RAY_QUERY_INTERSECTION_NONE) {
        return Hit(-1.0, 0u);
    }
    return Hit(h.t, h.instance_custom_data);
}
// whether a point sees the sun: only what casts shadows stops it (lib.rs's CASTS), as the shadow maps have it
fn sees_sun(o: vec3<f32>, d: vec3<f32>) -> bool {
    var rq: ray_query;
    rayQueryInitialize(&rq, tlas, RayDesc(RAY_FLAG_TERMINATE_ON_FIRST_HIT, CASTS, 0.01, 10000.0, o, d));
    rayQueryProceed(&rq);
    return rayQueryGetCommittedIntersection(&rq).kind == RAY_QUERY_INTERSECTION_NONE;
}

// a world direction in the sky cubemap's frame, as Bevy's environment_map.wesl turns it (cubemaps are left-handed)
fn sky_dir(d: vec3<f32>) -> vec3<f32> {
    var s = turn(p.sky_turn, d);
    s.z = -s.z;
    return s;
}
fn sky_radiance(d: vec3<f32>) -> vec3<f32> {
    if (p.sun_dir.w < 0.5) {
        return vec3(0.0);
    }
    return textureSampleLevel(sky, sky_sampler, sky_dir(d), 0.0).rgb * p.sun.w;
}
// the sky's irradiance over pi on a surface facing n, unshadowed (Bevy's diffuse map: what an albedo multiplies)
fn sky_irradiance(n: vec3<f32>) -> vec3<f32> {
    if (p.sun_dir.w < 0.5) {
        return vec3(0.0);
    }
    return textureSampleLevel(sky_diffuse, sky_sampler, sky_dir(n), 0.0).rgb * p.sun.w;
}

// The light arriving at a stand-in surface (irradiance, lux): what the cache has learned there, or where its cell is
// new, the sun through one shadow ray and the sky's diffuse light, unshadowed (LT2's stand-in).
fn irradiance_at(hit: vec3<f32>, n: vec3<f32>, keep: u32, seed: ptr<function, u32>) -> vec3<f32> {
    let cached = read(hit, n, keep, seed);
    if (cached.a > 0.0) {
        return cached.rgb;
    }
    let ndl = dot(n, p.sun_dir.xyz);
    var e = PI * sky_irradiance(n);
    if (ndl > 0.0 && sees_sun(hit + n * 0.02, p.sun_dir.xyz)) {
        e += p.sun.xyz * ndl;
    }
    return e;
}

// a direction about n, by the cosine (Malley: a point on the disc lifted to the hemisphere)
fn cosine(n: vec3<f32>, seed: ptr<function, u32>) -> vec3<f32> {
    let r = sqrt(rand(seed));
    let a = 2.0 * PI * rand(seed);
    let t = normalize(cross(n, select(vec3(1.0, 0.0, 0.0), vec3(0.0, 1.0, 0.0), abs(n.x) > 0.9)));
    let b = cross(n, t);
    return normalize(t * (r * cos(a)) + b * (r * sin(a)) + n * sqrt(max(1.0 - r * r, 0.0)));
}

fn luma(c: vec3<f32>) -> f32 {
    return dot(c, vec3(0.2126, 0.7152, 0.0722));
}

// whether a listed cell learns this frame: about upkeep.run.y of the live cells do, the same ones in `learn` and
// `blend` (the same first draw from the cell's seed)
fn learns(cell: u32, seed: ptr<function, u32>) -> bool {
    *seed = hash(cell ^ upkeep.run.x);
    return rand(seed) < f32(upkeep.run.y) / f32(max(count, 1u));
}

// one workgroup a probe, one ray a thread each round
@compute @workgroup_size(64, 1, 1)
fn main(@builtin(workgroup_id) wg: vec3<u32>, @builtin(local_invocation_index) li: u32) {
    let ijk = grid_ijk((p.run.x + wg.x) % p.res.w);
    let at = probe_at(ijk);
    let cell = cell_of(ijk);
    let probe = slot_of(cell);
    // a slot that held another probe (the grid moved over it) or none starts its means again
    if (li == 0u) {
        let tag = tags[probe];
        fresh = select(0u, 1u, tag.w == 0 || any(tag.xyz != cell));
        tags[probe] = vec4(cell, 1);
    }
    workgroupBarrier();
    let restart = workgroupUniformLoad(&fresh) == 1u;
    // a probe new to its slot traces the warm rounds (run.w), so it is lit the frame it is first reached
    let rounds = select(p.run.y, max(p.run.y, p.run.w), restart);
    for (var round = 0u; round < rounds; round++) {
        let q = random_turn(hash(p.run.z * 7919u + round * 104729u + probe));
        let d = turn(q, fib(li, RAYS));
        var l = vec3(0.0);
        let h = first_hit(at, d);
        if (h.t < 0.0) {
            l = sky_radiance(d);
        } else {
            let hit = at + d * h.t;
            var seed = hash(p.run.z * 7919u + round * 104729u + probe * RAYS + li);
            let r = records[h.rec];
            l = r.albedo.rgb / PI * irradiance_at(hit, -d, LIFE, &seed) + r.emission.rgb;
        }
        light[li] = l;
        dirs[li] = d;
        workgroupBarrier();
        // six threads, one a face of the cube: irradiance over pi from uniform sphere samples, (4 / N) sum L cos
        if (li < 6u) {
            let axis = li / 2u;
            let neg = li & 1u;
            let sign = select(1.0, -1.0, neg == 1u);
            var e = vec3(0.0);
            for (var k = 0u; k < RAYS; k++) {
                e += light[k] * max(dirs[k][axis] * sign, 0.0);
            }
            e *= 4.0 / f32(RAYS);
            let slot = probe * 6u + li;
            var old = cubes[slot];
            if (restart && round == 0u) {
                old = vec4(0.0);
            }
            let count = min(old.w + 1.0, HISTORY);
            cubes[slot] = vec4(mix(old.rgb, e, 1.0 / count), count);
        }
        workgroupBarrier();
    }
}

// Every probe's cube into Bevy's irradiance volume, each frame (the grid may have moved, so every texel may now be
// another probe's): (x, y, z) a probe, the negative face's half below in y, each axis a third of z. A slot not yet
// holding its probe writes nothing new.
@compute @workgroup_size(64, 1, 1)
fn resolve(@builtin(global_invocation_id) id: vec3<u32>) {
    if (id.x >= p.res.w) {
        return;
    }
    let ijk = grid_ijk(id.x);
    let cell = cell_of(ijk);
    let probe = slot_of(cell);
    let tag = tags[probe];
    let held = tag.w == 1 && all(tag.xyz == cell);
    let r = p.res.xyz;
    for (var face = 0u; face < 6u; face++) {
        let c = select(vec3(0.0), cubes[probe * 6u + face].rgb, held);
        textureStore(voxels, vec3<u32>(ijk.x, ijk.y + r.y * (face & 1u), ijk.z + r.z * (face / 2u)), vec4(c, 1.0));
    }
}

// The cache's update, a thread a listed cell (an indirect dispatch: compact.wesl's count): the light arriving at the
// cell this frame, from the sun through one shadow ray and one bounce by the cosine, which sees the cell it meets
// (last frame's light there, times the stand-in's albedo) or the sky. Over frames that is every bounce.
@compute @workgroup_size(64, 1, 1)
fn learn(@builtin(global_invocation_id) id: vec3<u32>) {
    if (id.x >= count) {
        return;
    }
    let cell = live[id.x];
    var seed = 0u;
    if (!learns(cell, &seed)) {
        return;
    }
    let c = cells[cell];
    let n = c.n.xyz;
    let o = c.at.xyz + n * 0.02;
    var e = vec3(0.0);
    var seen = 0.0;
    let ndl = dot(n, p.sun_dir.xyz);
    if (ndl > 0.0 && sees_sun(o, p.sun_dir.xyz)) {
        e = p.sun.xyz * ndl;
        seen = 1.0;
    }
    let d = cosine(n, &seed);
    let h = first_hit(o, d);
    if (h.t < 0.0) {
        // a cosine sample's irradiance: pi times the radiance it sees
        e += PI * sky_radiance(d);
    } else {
        // what the surface met sends back, its albedo of the light the cell met there has learned (that cell lives as
        // long as the one reading it), and its own light
        let r = records[h.rec];
        e += r.albedo.rgb * irradiance_at(o + d * h.t, -d, atomicLoad(&life[cell]), &seed) + PI * r.emission.rgb;
    }
    // the light (rgb) and whether the sun was seen (a)
    learned[id.x] = vec4(e, seen);
}

// Each cell that learned this frame takes its new light into its mean: up to upkeep.size.z frames, fewer while its
// light is changing (its luminance step against its light), so a door opening or a sky changing shows within frames.
@compute @workgroup_size(64, 1, 1)
fn blend(@builtin(global_invocation_id) id: vec3<u32>) {
    if (id.x >= count) {
        return;
    }
    let cell = live[id.x];
    var seed = 0u;
    if (!learns(cell, &seed)) {
        return;
    }
    let old = radiance[cell];
    let now = learned[id.x].rgb;
    let delta = deltas[cell];
    let n = min(old.a + 1.0, upkeep.size.z);
    let most = mix(upkeep.size.z, 1.0, pow(saturate(abs(delta) / max(luma(old.rgb), 0.001)), 1.0 / 8.0));
    let reset = upkeep.run.z == 1u;
    let w = select(1.0 / min(n, most), 1.0, reset);
    let mixed = mix(old.rgb, now, w);
    deltas[cell] = select(mix(delta, luma(mixed) - luma(old.rgb), 1.0 / 8.0), 0.0, reset);
    radiance[cell] = vec4(mixed, n);
    sunlit[cell] = mix(sunlit[cell], learned[id.x].a, w);
}
