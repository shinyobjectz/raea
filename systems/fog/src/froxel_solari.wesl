// The froxel grid's light from Solari (docs/raytracing.feature RT19, froxel/solari.rs): at each cell of the atlas, the
// light Solari's own lights scatter toward the eye, found as Solari finds a surface's: its emissive meshes (CANDIDATES
// drawn, one kept by its share of the cell's light, resampled importance sampling, and one shadow ray to it through
// Solari's scene), and the sky through whatever opens onto it (SKY rays in directions drawn from the medium's phase,
// so the phase cancels). Bevy's lights (the sun's shafts through its shadow maps, the point and spot lights) stay the
// light pass's; Solari's directional lights are left out here, as they'd count the sun twice.
// `solari`: each frame's estimate of the light arriving (phase-weighted, before the medium's scattering) goes into the
// cell's history, found where the cell's centre was in the last frame's view, an average of its last `frames` frames.
// The sky through a small opening (photo 5's skylight) is found by few of the rays drawn from the phase, so each cell
// keeps a guide: a running sum of the directions that reached the sky from it, each weighted by the light it brought
// over its pdf (so the guide learns where the sky's light comes from, not where the rays were drawn), its own and half
// a neighbour's carried from the last frame. Half its rays are then drawn from a cone round their mean (as wide as
// their spread), half from the phase, each weighted by the two's mixed pdf: whatever the guide holds, the estimate
// stays unbiased (the phase covers every direction), and once a cell has found the opening most rays go through it.
// `filter`: the history blurred over the cell's 3 x 3 x 3 neighbours (a tent), then scattered by the medium at the
// cell's centre, which is what the integrate pass adds. The light is blurred, not the light the medium scatters, so a
// dense edge (the steam's) stays sharp while its light settles.

import bevy_render::view::View;
import bevy_render::maths::{PI_2, orthonormalize};
import bevy_pbr::render::utils::{rand_f, rand_vec2f};
import bevy_solari::scene::bindings::{trace_ray, light_sources, RAY_T_MIN, RAY_T_MAX};
import bevy_solari::scene::sampling::{generate_random_light_sample, trace_visibility, sample_environment_map_light, ResolvedLightSample};
import raea_sys_fog::froxel_volume::{Volume, volume_weight, volume_depth, henyey_greenstein, sample_phase};

enable wgpu_ray_query;

// the grid's (views.rs Params: what this pass reads of it, a prefix of the uniform)
struct Params {
    view_from_clip: mat4x4<f32>,
    grid: vec4<u32>,
    slab: vec4<f32>,
    viewport: vec4<f32>,
    ambient: vec4<f32>,
    counts: vec4<u32>,
    heroes: vec4<u32>,
}

// froxel/solari.rs SolariFogParams
struct SolariFog {
    // the last frame's view (unjittered) and its grid's slab, for the history
    prev_clip_from_world: mat4x4<f32>,
    prev_slab: vec4<f32>,
    // x: the frame's number; y: 1 to start the history again; z: candidates a cell; w: frames the history averages
    frame: vec4<u32>,
    // x: what lights it (1 the emissive meshes, 2 the sky); y: sky rays a cell; z: 1 to filter; w: what the sky's
    // guide is made of (1 the cell's own last, 2 half a neighbour's; 0 no guide, the rays drawn from the phase)
    more: vec4<u32>,
}

@group(1) @binding(0) var<uniform> params: Params;
@group(1) @binding(1) var<storage, read> volumes: array<Volume>;
@group(1) @binding(2) var<uniform> view: View;
@group(1) @binding(3) var<uniform> fog: SolariFog;
// solari: the last frame's history; filter: this frame's
@group(1) @binding(4) var history: texture_2d<f32>;
// solari: this frame's history; filter: what the integrate pass adds
@group(1) @binding(5) var out: texture_storage_2d<rgba16float, write>;
// the sky's guides, the last frame's and this one's: the sum of the directions that reached the sky (xyz) and of their
// weights (w), each weighted by its light over its pdf, fading (the filter binds them and leaves them be)
@group(1) @binding(6) var guides_was: texture_2d<f32>;
@group(1) @binding(7) var guides: texture_storage_2d<rgba32float, write>;

const LUMA = vec3(0.2126, 0.7152, 0.0722);
// how much of a guide is kept from one frame to the next, so it follows a camera, an opening closed or the sun moved
const GUIDE_KEEP: f32 = 0.9;
// the guide's cone: no wider than 60 degrees, no narrower than 2
const COS_WIDEST: f32 = 0.5;
const COS_NARROWEST: f32 = 0.99939;

// what of a light's light gets through the fog from p along dir (unit) for up to t_max metres (froxel_volume.wesl)
fn light_through(p: vec3<f32>, dir: vec3<f32>, t_max: f32) -> f32 {
    var depth = 0.0;
    for (var v = 0u; v < params.counts.x; v += 1u) {
        depth += volume_depth(volumes[v], p, dir, t_max);
    }
    return exp(-depth);
}

// what a light sample sends to point P toward the eye, before its pdf: emitted radiance x its cosine / distance^2 x
// the phase
fn toward_eye(s: ResolvedLightSample, P: vec3<f32>, V: vec3<f32>, g: f32) -> vec3<f32> {
    let ray = s.world_position.xyz - P;
    let d2 = max(dot(ray, ray), 1e-6);
    let wi = ray * inverseSqrt(d2);
    let cos_light = saturate(dot(-wi, s.world_normal));
    return s.radiance * (cos_light / d2) * henyey_greenstein(g, dot(wi, V));
}

// a point at view depth `depth` through a frame pixel
fn view_point(pixel: vec2<f32>, depth: f32) -> vec3<f32> {
    let ndc = vec2((pixel.x - params.viewport.x) / params.viewport.z * 2.0 - 1.0, 1.0 - (pixel.y - params.viewport.y) / params.viewport.w * 2.0);
    let q4 = params.view_from_clip * vec4(ndc, 1.0, 1.0);
    let q = q4.xyz / q4.w;
    return q * (depth / -q.z);
}

// the world point at a place in the cell (0-1 each way: across, down, deep)
fn cell_point(cell: vec2<u32>, slice: u32, at: vec3<f32>) -> vec3<f32> {
    let tile = params.slab.x;
    let last = params.viewport.xy + params.viewport.zw - 0.5;
    let pixel = min(params.viewport.xy + (vec2<f32>(cell) + at.xy) * tile, last);
    let depth = params.slab.y * exp(params.slab.w * (f32(slice) + at.z) / f32(params.grid.z));
    return (view.world_from_view * vec4(view_point(pixel, depth), 1.0)).xyz;
}

// the medium at P: its scattering per metre (each volume's colour x scattering x density, thinned going up) in rgb,
// and its anisotropy weighted by each volume's share in a
fn medium(P: vec3<f32>) -> vec4<f32> {
    var sigma_s = vec3(0.0);
    var g = 0.0;
    var share = 0.0;
    for (var v = 0u; v < params.counts.x; v += 1u) {
        let vol = &volumes[v];
        let local = ((*vol).local_from_world * vec4(P, 1.0)).xyz;
        let w = volume_weight((*vol).shape, local);
        if (w <= 0.0) {
            continue;
        }
        let s = (*vol).ambient.rgb * w;
        sigma_s += s;
        g += (*vol).ambient.a * dot(s, LUMA);
        share += dot(s, LUMA);
    }
    return vec4(sigma_s, select(0.0, g / share, share > 0.0));
}

// an atlas texel's cell and slice, or slice >= the grid's depth where the texel holds none
fn texel_cell(t: vec2<u32>) -> vec3<u32> {
    let slice = (t.y / params.grid.y) * params.grid.w + t.x / params.grid.x;
    return vec3(t.x % params.grid.x, t.y % params.grid.y, slice);
}

fn cell_texel(cell: vec2<u32>, slice: u32) -> vec2<u32> {
    let cols = params.grid.w;
    return vec2(cell.x + (slice % cols) * params.grid.x, cell.y + (slice / cols) * params.grid.y);
}

fn inside_grid(c: vec3<i32>) -> bool {
    return all(c >= vec3(0)) && c.x < i32(params.grid.x) && c.y < i32(params.grid.y) && c.z < i32(params.grid.z);
}

// where world point C was in the last frame's grid: its cell and slice (x < 0 when it was outside)
fn last_cell(C: vec3<f32>) -> vec3<i32> {
    let was = fog.prev_clip_from_world * vec4(C, 1.0);
    if (was.w <= 0.0) {
        return vec3(-1);
    }
    let ndc = was.xy / was.w;
    let px = vec2((ndc.x + 1.0) * 0.5 * params.viewport.z, (1.0 - ndc.y) * 0.5 * params.viewport.w);
    let c = vec3(vec2<i32>(floor(px / params.slab.x)), i32(floor(f32(params.grid.z) * log(was.w / fog.prev_slab.y) / fog.prev_slab.w)));
    return select(vec3(-1), c, inside_grid(c));
}

// the n-th point of the R3 sequence (Roberts' generalised golden ratio) shifted by `offset`: over the frames its points
// spread through the unit cube far more evenly than random ones, so an average of a cell's last frames (its history)
// covers the cell sooner. n wraps at 4096, where the float's step is still 1/4000
fn stratified(n: u32, offset: vec3<f32>) -> vec3<f32> {
    return fract(offset + f32(n % 4096u) * vec3(0.8191725134, 0.6710436067, 0.5497004779));
}

// a direction drawn evenly in the cone round `axis` out to cos_max (pdf 1 / (2 pi (1 - cos_max)))
fn sample_cone(axis: vec3<f32>, cos_max: f32, u: vec2<f32>) -> vec3<f32> {
    let cos_theta = 1.0 - u.x * (1.0 - cos_max);
    let sin_theta = sqrt(max(0.0, 1.0 - cos_theta * cos_theta));
    let phi = u.y * PI_2;
    return orthonormalize(axis) * vec3(cos(phi) * sin_theta, sin(phi) * sin_theta, cos_theta);
}

fn guide_at(c: vec3<i32>) -> vec4<f32> {
    if (!inside_grid(c)) {
        return vec4(0.0);
    }
    return textureLoad(guides_was, cell_texel(vec2<u32>(c.xy), u32(c.z)), 0);
}

// the sky's light toward the eye at P (phase-weighted, as the lamps'): `rays` directions, each from the guide's cone
// or the phase, weighted by their mixed pdf. The guide is the cell's last (where its centre C was) and half a
// neighbour's within 2 cells and a slice, unless the history starts again; this cell's new one is stored at texel t
fn sky_guided(P: vec3<f32>, C: vec3<f32>, V: vec3<f32>, g: f32, rays: u32, t: vec2<u32>, rng: ptr<function, u32>) -> vec3<f32> {
    var guide = vec4(0.0);
    var parts = 0.0;
    if (fog.frame.y == 0u) {
        let was = last_cell(C);
        if (was.x >= 0) {
            if ((fog.more.w & 1u) != 0u) {
                guide += guide_at(was);
                parts += 1.0;
            }
            if ((fog.more.w & 2u) != 0u) {
                let d = vec3(vec2<i32>(floor(rand_vec2f(rng) * 5.0)) - vec2(2), i32(floor(rand_f(rng) * 3.0)) - 1);
                guide += 0.5 * guide_at(was + d);
                parts += 0.5;
            }
        }
    }
    // the cone: round the escapes' mean, as wide as a uniform cone with their spread (mean length R: cos = 2R - 1)
    var axis = vec3(0.0, 0.0, 1.0);
    var cos_max = COS_WIDEST;
    var share = 0.0;
    if (guide.w > 0.0) {
        let mean = guide.xyz / guide.w;
        let R = length(mean);
        if (R > 1e-3) {
            axis = mean / R;
            cos_max = clamp(2.0 * R - 1.0, COS_WIDEST, COS_NARROWEST);
            share = 0.5;
        }
    }
    let cone_pdf = 1.0 / (PI_2 * (1.0 - cos_max));
    var sky = vec3(0.0);
    var found = vec4(0.0);
    for (var i = 0u; i < rays; i += 1u) {
        var L: vec3<f32>;
        // random, not stratified: the guide adapts to the last frames' rays, and a sequence would tie this frame's to
        // theirs (white fog under a uniform sky came out 2-3% too bright)
        if (rand_f(rng) < share) {
            L = sample_cone(axis, cos_max, rand_vec2f(rng));
        } else {
            L = sample_phase(g, V, rand_vec2f(rng));
        }
        let phase = henyey_greenstein(g, dot(L, V));
        let pdf = (1.0 - share) * phase + share * select(0.0, cone_pdf, dot(L, axis) >= cos_max);
        let hit = trace_ray(P, L, RAY_T_MIN, RAY_T_MAX, RAY_FLAG_TERMINATE_ON_FIRST_HIT);
        if (hit.kind == RAY_QUERY_INTERSECTION_NONE && pdf > 0.0) {
            let arrives = sample_environment_map_light(L) * (phase / pdf) * light_through(P, L, RAY_T_MAX);
            sky += arrives;
            // weighted by what it brought: a cone's own rays, crowded in it, would otherwise narrow it frame by frame
            found += vec4(L, 1.0) * dot(arrives, LUMA);
        }
    }
    // the guide carried on, scaled so a neighbour's half doesn't make it grow, with this frame's escapes added
    textureStore(guides, t, guide * (GUIDE_KEEP / max(parts, 1.0)) + found);
    return sky / f32(max(rays, 1u));
}

@compute @workgroup_size(8, 8, 1)
fn solari(@builtin(global_invocation_id) id: vec3<u32>) {
    let size = textureDimensions(out);
    if (id.x >= size.x || id.y >= size.y) {
        return;
    }
    let c = texel_cell(id.xy);
    if (c.z >= params.grid.z) {
        textureStore(out, id.xy, vec4(0.0));
        textureStore(guides, id.xy, vec4(0.0));
        return;
    }
    var rng = (id.x * 1973u + id.y * 9277u + fog.frame.x * 26699u) | 1u;
    let C = cell_point(c.xy, c.z, vec3(0.5));
    // the cell's own offsets for its sequences, the same every frame (Cranley-Patterson rotation)
    var cell_rng = (id.x * 7919u + id.y * 104729u) | 1u;
    let offset = vec3(rand_vec2f(&cell_rng), rand_f(&cell_rng));

    // this frame's point: anywhere in the cell, stratified over the frames, so the history averages the whole cell
    let P = cell_point(c.xy, c.z, stratified(fog.frame.x, offset));
    let V = normalize(P - view.world_position);
    let m = medium(P);
    let held = any(m.rgb > vec3(0.0));
    let g = m.a;

    var light = vec3(0.0);
    if (held) {
        // the emissive meshes: CANDIDATES drawn, one kept by its share of the light here, one shadow ray to it
        let count = select(0u, fog.frame.z, (fog.more.x & 1u) != 0u && arrayLength(&light_sources) > 0u);
        var kept: ResolvedLightSample;
        var kept_weight = 0.0;
        var w_sum = 0.0;
        for (var i = 0u; i < count; i += 1u) {
            let s = generate_random_light_sample(&rng).resolved_light_sample;
            // a directional light (w 0) is the light pass's, a sky portal's sky (w 2) the sky term's
            if (s.world_position.w != 1.0) {
                continue;
            }
            let p_hat = dot(toward_eye(s, P, V, g), LUMA);
            let w = p_hat * s.inverse_pdf;
            w_sum += w;
            if (w > 0.0 && rand_f(&rng) * w_sum < w) {
                kept = s;
                kept_weight = p_hat;
            }
        }
        if (kept_weight > 0.0) {
            let to = kept.world_position.xyz - P;
            let d = length(to);
            light += toward_eye(kept, P, V, g) * (w_sum / (f32(count) * kept_weight)) * trace_visibility(P, kept.world_position) * light_through(P, to / max(d, 1e-6), d);
        }
        // the sky, through what opens onto it: guided, or directions drawn from the phase (so the phase cancels)
        let rays = select(0u, fog.more.y, (fog.more.x & 2u) != 0u);
        if (rays > 0u && fog.more.w != 0u) {
            light += sky_guided(P, C, V, g, rays, id.xy, &rng);
        } else {
            var sky = vec3(0.0);
            for (var i = 0u; i < rays; i += 1u) {
                let L = sample_phase(g, V, rand_vec2f(&rng));
                let hit = trace_ray(P, L, RAY_T_MIN, RAY_T_MAX, RAY_FLAG_TERMINATE_ON_FIRST_HIT);
                if (hit.kind == RAY_QUERY_INTERSECTION_NONE) {
                    sky += sample_environment_map_light(L) * light_through(P, L, RAY_T_MAX);
                }
            }
            light += sky / f32(max(rays, 1u));
            textureStore(guides, id.xy, vec4(0.0));
        }
    } else {
        textureStore(guides, id.xy, vec4(0.0));
    }
    let now = light * view.exposure;

    // the history: where the cell's centre was in the last frame's view
    var n = 1.0;
    var result = now;
    if (fog.frame.y == 0u) {
        let was = last_cell(C);
        if (was.x >= 0) {
            let h = textureLoad(history, cell_texel(vec2<u32>(was.xy), u32(was.z)), 0);
            n = min(h.a + 1.0, f32(fog.frame.w));
            result = mix(h.rgb, now, 1.0 / n);
        }
    }
    // a cell outside every volume keeps no history (and the filter leaves it out)
    textureStore(out, id.xy, select(vec4(0.0), vec4(result, n), held));
}

@compute @workgroup_size(8, 8, 1)
fn filter_light(@builtin(global_invocation_id) id: vec3<u32>) {
    let size = textureDimensions(out);
    if (id.x >= size.x || id.y >= size.y) {
        return;
    }
    let c = texel_cell(id.xy);
    if (c.z >= params.grid.z) {
        textureStore(out, id.xy, vec4(0.0));
        return;
    }
    let sigma_s = medium(cell_point(c.xy, c.z, vec3(0.5))).rgb;
    if (all(sigma_s <= vec3(0.0))) {
        textureStore(out, id.xy, vec4(0.0));
        return;
    }
    var sum = vec3(0.0);
    var weight = 0.0;
    let reach = select(0, 1, fog.more.z != 0u);
    for (var dz = -reach; dz <= reach; dz += 1) {
        let s = i32(c.z) + dz;
        if (s < 0 || s >= i32(params.grid.z)) {
            continue;
        }
        for (var dy = -reach; dy <= reach; dy += 1) {
            let y = i32(c.y) + dy;
            if (y < 0 || y >= i32(params.grid.y)) {
                continue;
            }
            for (var dx = -reach; dx <= reach; dx += 1) {
                let x = i32(c.x) + dx;
                if (x < 0 || x >= i32(params.grid.x)) {
                    continue;
                }
                let h = textureLoad(history, cell_texel(vec2(u32(x), u32(y)), u32(s)), 0);
                // a tent (1 2 1 each way), and only cells with light found (a > 0)
                let w = f32((2 - abs(dx)) * (2 - abs(dy)) * (2 - abs(dz))) * select(0.0, 1.0, h.a > 0.0);
                sum += h.rgb * w;
                weight += w;
            }
        }
    }
    textureStore(out, id.xy, vec4(sigma_s * sum / max(weight, 1e-6), 1.0));
}
