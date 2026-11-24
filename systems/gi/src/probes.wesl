// raea GI's screen probes (lib.rs; docs/realism.feature LT8, D6): one every 16 pixels, on the surface its tile's middle
// pixel shows (or where that is sky, the first of its quarters' middles that isn't). Each traces 16 rays a frame by the
// cosine about its normal through the ray scene, as raea-sys-rt's gi.wesl does: a hit sees its instance's record, lit
// by what the world cache has learned there; a miss sees the sky. The light arriving (irradiance, lux; the sun on the
// probe itself left out, raster draws it) is kept as a running mean over up to HISTORY frames. Its history is the
// probe that stood where it stands last frame (its place through last frame's view), kept only when that one was on
// the same surface. `pool` then takes each probe with its neighbours on its own surface (probe space, D6), and the
// screen pass (screen.wesl) fills the pixels between by depth and normal.
import raea_sys_rt::cache::{hash, LIFE};
import raea_sys_rt::gi::{first_hit, irradiance_at, sky_radiance, cosine, records};
import raea_sys_gi::frame::{surface, view, now, then, soft, p, Probe, Surface, TILE};

enable wgpu_ray_query;

const PI: f32 = 3.14159265;
/// rays a probe a frame; a workgroup holds four probes
const RAYS: u32 = 16u;
const HISTORY: f32 = 24.0;

var<workgroup> light: array<vec3<f32>, 64>;
var<workgroup> spot: array<vec4<f32>, 4>;
var<workgroup> facing: array<vec3<f32>, 4>;

/// where in its tile a probe looks for a surface, in order
const SPOTS = array<vec2<i32>, 5>(vec2(8, 8), vec2(4, 4), vec2(12, 4), vec2(4, 12), vec2(12, 12));

/// how far a probe may move and still be the one it was (m), at a distance from the camera
fn near(dist: f32) -> f32 {
    return 0.05 + 0.03 * dist;
}

@compute @workgroup_size(64, 1, 1)
fn main(@builtin(workgroup_id) wg: vec3<u32>, @builtin(local_invocation_index) li: u32) {
    let local = li / RAYS;
    let probe = wg.x * 4u + local;
    let tiles = p.run.yz;
    let inside = probe < tiles.x * tiles.y;
    let tile = vec2<i32>(vec2<u32>(probe % tiles.x, probe / tiles.x) * TILE);
    if (li % RAYS == 0u) {
        var s = Surface(vec4(0.0), vec3(0.0, 1.0, 0.0));
        if (inside) {
            for (var k = 0u; k < 5u; k++) {
                s = surface(tile + SPOTS[k]);
                if (s.at.w > 0.0) {
                    break;
                }
            }
        }
        spot[local] = s.at;
        facing[local] = s.n;
    }
    workgroupBarrier();
    let at = spot[local];
    let n = facing[local];
    let dist = length(at.xyz - view.world_position);
    var l = vec3(0.0);
    if (at.w > 0.0) {
        var seed = hash(probe * 7919u + (li % RAYS) * 104729u + p.run.x);
        let o = at.xyz + n * (0.02 + 0.002 * dist);
        let d = cosine(n, &seed);
        let h = first_hit(o, d);
        if (h.t < 0.0) {
            l = sky_radiance(d);
        } else {
            let r = records[h.rec];
            l = r.albedo.rgb / PI * irradiance_at(o + d * h.t, -d, LIFE, &seed) + r.emission.rgb;
        }
    }
    light[li] = l;
    workgroupBarrier();
    if (li % RAYS != 0u || !inside) {
        return;
    }
    if (at.w <= 0.0) {
        now[probe] = Probe(vec4(0.0), vec4(0.0), vec4(0.0));
        return;
    }
    // cosine samples' irradiance: pi times the mean radiance they see
    var e = vec3(0.0);
    for (var k = 0u; k < RAYS; k++) {
        e += light[li + k];
    }
    e *= PI / f32(RAYS);
    var held = 0.0;
    var old = vec3(0.0);
    if (p.run.w == 1u) {
        let c = p.then_clip * vec4(at.xyz, 1.0);
        if (c.w > 0.0) {
            let uv = vec2(c.x / c.w * 0.5 + 0.5, 0.5 - c.y / c.w * 0.5);
            let t = vec2<i32>(floor((view.viewport.xy + uv * view.viewport.zw) / f32(TILE)));
            if (all(t >= vec2(0)) && all(t < vec2<i32>(tiles))) {
                let was = then[u32(t.x) + u32(t.y) * tiles.x];
                if (was.at.w > 0.0 && length(was.at.xyz - at.xyz) < near(dist) && dot(was.n.xyz, n) > 0.8) {
                    held = was.at.w;
                    old = was.e.rgb;
                }
            }
        }
    }
    let k = min(held + 1.0, HISTORY);
    now[probe] = Probe(vec4(at.xyz, k), vec4(n, 0.0), vec4(mix(old, e, 1.0 / k), 0.0));
}

// Each probe with the probes up to two tiles around it that stand on its surface (their planes and normals alike), the
// nearer and the longer held counting more: a probe's 16 rays a frame rarely find a small bright light (FLOOR3's
// ceiling lenses), so alone neighbouring probes disagree and show as tiles.
@compute @workgroup_size(64, 1, 1)
fn pool(@builtin(global_invocation_id) id: vec3<u32>) {
    let tiles = vec2<i32>(p.run.yz);
    if (id.x >= u32(tiles.x * tiles.y)) {
        return;
    }
    let c = now[id.x];
    if (c.at.w <= 0.0) {
        soft[id.x] = c;
        return;
    }
    let t = vec2<i32>(i32(id.x) % tiles.x, i32(id.x) / tiles.x);
    let reach = near(length(c.at.xyz - view.world_position));
    var sum = vec3(0.0);
    var weight = 0.0;
    for (var dy = -2; dy <= 2; dy++) {
        for (var dx = -2; dx <= 2; dx++) {
            let u = t + vec2(dx, dy);
            if (any(u < vec2(0)) || any(u >= tiles)) {
                continue;
            }
            let q = now[u32(u.x + u.y * tiles.x)];
            if (q.at.w <= 0.0) {
                continue;
            }
            let same = pow(max(dot(c.n.xyz, q.n.xyz), 0.0), 8.0) * exp(-abs(dot(c.n.xyz, q.at.xyz - c.at.xyz)) / reach);
            let w = same * exp(-f32(dx * dx + dy * dy) / 4.5) * q.at.w;
            sum += q.e.rgb * w;
            weight += w;
        }
    }
    soft[id.x] = Probe(c.at, c.n, vec4(sum / weight, 0.0));
}
