// The froxel fog's hero lights (froxel.rs): the volumetric spot and directional lights whose shafts the fog shows,
// integrated exactly instead of in the grid's cells, so a shaft's edges (a blind's bars, a doorway) are as crisp as
// its shadow map and have no steps or noise.
//
// Where: Chen et al. 2011's epipolar sampling. Every shaft in the frame runs along a line through the light's point
// on the screen, so the light is integrated at samples along such lines (from that point to the border of the
// light's rectangle of the frame, `epi.x` lines `line gap` pixels apart round its border, a sample every `epi_tol.w`
// pixels along each), not at every pixel. Four passes off the frame, a texel a sample: `coarse` integrates every K-th
// sample and each line's last; `mark` copies those, interpolates the samples between two of them, or, where the
// depth bends between them (1 / depth's second difference) or their light differs by more than interpolating gets
// right, marks the sample; `fine` integrates the marked ones, packed together by the depth test. Then `epipolar`,
// over the frame with the grid's fog, reads each pixel's light from the samples either side of it on the two lines
// round it, those at its depth (`exact` integrates every pixel instead: RAEA_FOG_HERO_EXACT, the integrator's own
// ground truth).
//
// Along the ray the light scattered toward the eye is the integral of the medium's scattering x the light reaching
// each point x what of it still shows at the camera (the grid's transmittance, smooth). The ray is cut where anything
// in that integrand jumps: where it enters and leaves each fog volume, a spot's cones (its inner and outer edge),
// each cascade's depth range. Each piece is then walked across the shadow map texel by texel (the ray projects to a
// line there, along which the depth it's compared at is linear: Chen et al. 2011's observation, walked over a min-max
// pyramid of the map, froxel_minmax.wgsl, taking whole blocks as lit or shadowed), and the hardware 2x2 comparison filter the reference march samples is integrated exactly: between the
// depths where each of a texel cell's four corners turns from shadowed to lit, the filter is a fixed blend of the
// four, so each such part is integrated by Gauss-Legendre. Runs of fully lit cells are merged and integrated as one,
// for a spot or point light with the equiangular change of variable (the angle seen from the light, which takes the
// inverse square out: Sun et al. 2005's closed form for a homogeneous medium, Kulla and Fajardo 2012's measure), so
// a ray passing close to the light is integrated as exactly as one passing far from it.
//
// The model is the grid's and Bevy's march's: per volume its light, tint and attenuation (froxel_light.wgsl's
// `scatter`), its falloff, Henyey-Greenstein for directional lights and no phase for spots, the view's exposure.

#import bevy_core_pipeline::fullscreen_vertex_shader::FullscreenVertexOutput
#import bevy_pbr::mesh_view_bindings::{lights, view, clustered_lights}
#import bevy_pbr::mesh_view_bindings as view_bindings
#import bevy_pbr::mesh_view_types::{
    DIRECTIONAL_LIGHT_FLAGS_VOLUMETRIC_BIT,
    POINT_LIGHT_FLAGS_SHADOWS_ENABLED_BIT,
    POINT_LIGHT_FLAGS_SPOT_LIGHT_Y_NEGATIVE,
}
#import bevy_pbr::view_transformations::{frag_coord_to_ndc, position_ndc_to_view}
#import bevy_render::maths::orthonormalize

struct Params {
    view_from_clip: mat4x4<f32>,
    grid: vec4<u32>,
    slab: vec4<f32>,
    viewport: vec4<f32>,
    ambient: vec4<f32>,
    counts: vec4<u32>,
    heroes: vec4<u32>,
    // epipolar sampling: slices, samples along each, K (every K-th integrated), hero lights on
    epi: vec4<u32>,
    epi_tol: vec4<f32>,
    // each hero light's rectangle of the frame (x0, y0, x1, y1), its lines and samples along each at most
    hero_rect: array<vec4<f32>, 4>,
    hero_lines: vec4<u32>,
    hero_along: vec4<u32>,
}

struct Volume {
    local_from_world: mat4x4<f32>,
    scatter: vec4<f32>,
    ambient: vec4<f32>,
    shape: vec4<f32>,
}

@group(1) @binding(0) var<uniform> params: Params;
@group(1) @binding(1) var<storage, read> volumes: array<Volume>;
@group(1) @binding(2) var grid: texture_3d<f32>;
@group(1) @binding(3) var grid_sampler: sampler;
#ifdef MULTISAMPLED
@group(1) @binding(4) var depth_texture: texture_depth_multisampled_2d;
#else
@group(1) @binding(4) var depth_texture: texture_depth_2d;
#endif
// the shadow maps' blocks' nearest and farthest depth, 4, 16, 64 and 256 texels a side (froxel_minmax.wgsl)
@group(1) @binding(5) var blocks4: texture_2d_array<f32>;
@group(1) @binding(6) var blocks16: texture_2d_array<f32>;
@group(1) @binding(7) var blocks64: texture_2d_array<f32>;
@group(1) @binding(8) var blocks256: texture_2d_array<f32>;
@group(1) @binding(9) var nearest: sampler;

const FRAC_4_PI: f32 = 0.07957747154594767;
const NONE: u32 = 0xffffffffu;
// fog volumes whose sides cut the ray (more are still lit, their sides just aren't cut at)
const CUT_VOLUMES: u32 = 16u;
const CUTS: u32 = 40u;
// the pyramid's levels: texel cells (0), then blocks of 4, 16, 64 and 256 texels a side (4); and the steps a piece of
// ray takes at most, down and up it (a guard: far more than a ray across a 4096 map takes)
const TOP: u32 = 4u;
const MAX_STEPS: u32 = 4096u;
// past this many steps a piece of ray stops going down to texels: a block of 4 its depth crosses is taken as lit in the
// proportion its depth is between the block's nearest and farthest (a pass waits for its longest walk, and a ray
// running along a shadow's edge would step texel by texel the whole way)
const EXACT_STEPS: u32 = 16u;

// Gauss-Legendre on [0, 1]: 4 nodes for runs (exact to degree 7: a run lies between cuts at volume sides and cone
// edges, and a spot's inverse square is taken out by integrating in angle, so what is left is smooth; 8 measured the
// same), 2 for the parts of a partly shadowed cell
const GL4_X = array<f32, 4>(0.06943184420297371, 0.33000947820757187, 0.6699905217924281, 0.9305681557970263);
const GL4_W = array<f32, 4>(0.17392742256872692, 0.32607257743127307, 0.32607257743127307, 0.17392742256872692);
const GL2_X = array<f32, 2>(0.21132486540518713, 0.7886751345948129);

// the ray, and the light being integrated
struct Ray {
    origin: vec3<f32>,
    dir: vec3<f32>,
    // view depth per metre along the ray
    depth_per_t: f32,
    uv: vec2<f32>,
}

struct Hero {
    // 0: spot, 1: directional
    kind: u32,
    index: u32,
    color: vec3<f32>,
    position: vec3<f32>,
    // spot: the way it points; directional: toward the light
    axis: vec3<f32>,
    // spot: its cone's scale and offset (Bevy's light_custom_data.zw), its tangent; its range's inverse square
    cone: vec4<f32>,
    // directional: Henyey-Greenstein's cosine (fixed along the ray)
    cos_view: f32,
    shadows: bool,
    // the equiangular frame: the ray's nearest approach to the light and its distance there
    t_closest: f32,
    h: f32,
    // the texel walk: the shadow map's layer and size, and the light's transform (directional: the cascade's)
    layer: i32,
    size: vec2<f32>,
    clip_from_world: mat4x4<f32>,
    rot: mat3x3<f32>,
    bias: f32,
}

// ---- the grid's transmittance, as froxel_grid.wgsl's apply reads it ----

fn boundary(s: f32) -> f32 {
    return params.slab.y * exp(params.slab.w * (s + 1.0) / f32(params.grid.z));
}

// the grid at a point of the frame and a slice, tricubic (froxel_grid.wgsl's sample_grid)
fn grid_rgba(uv: vec2<f32>, w: f32) -> vec4<f32> {
    let size = vec2<f32>(vec2(params.grid.x, params.grid.y));
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
    let a = textureSampleLevel(grid, grid_sampler, vec3(h0.x, h0.y, w), 0.0);
    let b = textureSampleLevel(grid, grid_sampler, vec3(h1.x, h0.y, w), 0.0);
    let c = textureSampleLevel(grid, grid_sampler, vec3(h0.x, h1.y, w), 0.0);
    let d = textureSampleLevel(grid, grid_sampler, vec3(h1.x, h1.y, w), 0.0);
    return g0.y * (g0.x * a + g1.x * b) + g1.y * (g0.x * c + g1.x * d);
}

fn grid_alpha(uv: vec2<f32>, w: f32) -> f32 {
    return grid_rgba(uv, w).a;
}

// the grid's fog at a pixel, as froxel_grid.wgsl's apply reads it: the light gathered in front of the surface (rgb)
// and what of the surface still shows (a)
fn grid_fog(px: vec2<f32>) -> vec4<f32> {
    let ndc_depth = textureLoad(depth_texture, vec2<i32>(px), 0);
    var depth = params.slab.z * 2.0;
    if (ndc_depth > 0.0) {
        let p = params.view_from_clip * vec4(0.0, 0.0, ndc_depth, 1.0);
        depth = -p.z / p.w;
    }
    let uv = (px - params.viewport.xy) / (params.slab.x * vec2<f32>(vec2(params.grid.x, params.grid.y)));
    let first = boundary(0.0);
    if (depth < first) {
        return mix(vec4(0.0, 0.0, 0.0, 1.0), grid_rgba(uv, 0.5 / f32(params.grid.z)), max(depth, 0.0) / first);
    }
    let c = f32(params.grid.z) * log(depth / params.slab.y) / params.slab.w - 1.0;
    return grid_rgba(uv, (c + 0.5) / f32(params.grid.z));
}

// the grid's transmittance at a point of the frame, trilinear (TRICUBIC_TRANSMITTANCE: tricubic, as the apply pass
// reads the light)
fn grid_t(uv: vec2<f32>, w: f32) -> f32 {
#ifdef TRICUBIC_TRANSMITTANCE
    return grid_alpha(uv, w);
#else
    return textureSampleLevel(grid, grid_sampler, vec3(uv, w), 0.0).a;
#endif
}

// what of a point at this view depth still shows at the camera
fn transmittance(ray: Ray, depth: f32) -> f32 {
    let first = boundary(0.0);
    if (depth < first) {
        return mix(1.0, grid_t(ray.uv, 0.5 / f32(params.grid.z)), max(depth, 0.0) / first);
    }
    let c = f32(params.grid.z) * log(depth / params.slab.y) / params.slab.w - 1.0;
    return grid_t(ray.uv, (c + 0.5) / f32(params.grid.z));
}

// ---- the integrand ----

fn henyey_greenstein(g: f32, c: f32) -> f32 {
    let denom = 1.0 + g * g - 2.0 * g * c;
    return FRAC_4_PI * (1.0 - g * g) / (denom * sqrt(denom));
}

// the light scattered toward the eye per metre at a point (per unit of the light's colour, before the light's own
// falloff), summed over the volumes holding it
fn medium(hero: Hero, p: vec3<f32>) -> vec3<f32> {
    var k = vec3(0.0);
    for (var v = 0u; v < params.counts.x; v += 1u) {
        let vol = &volumes[v];
        let local = ((*vol).local_from_world * vec4(p, 1.0)).xyz;
        if (any(abs(local) > vec3(0.5))) {
            continue;
        }
        let w = exp(-(*vol).shape.x * (local.y + 0.5) * (*vol).shape.y);
        var phase = 1.0;
        if (hero.kind == 1u) {
            phase = henyey_greenstein((*vol).ambient.a, hero.cos_view);
        }
        k += (*vol).scatter.rgb * w * phase;
    }
    return k;
}

// a spot's cone at a point (Bevy's: the cosine off its axis, scaled and offset, squared)
fn cone(hero: Hero, p: vec3<f32>) -> f32 {
    let cd = dot(hero.axis, normalize(p - hero.position));
    let s = saturate(cd * hero.cone.x + hero.cone.y);
    return s * s;
}

// Bevy's range window (getDistanceAttenuation without its inverse square)
fn window(hero: Hero, d2: f32) -> f32 {
    let factor = d2 * hero.cone.w;
    let smooth_factor = saturate(1.0 - factor * factor);
    return smooth_factor * smooth_factor;
}

// the integrand at t, unshadowed: medium x light x transmittance
fn radiance(ray: Ray, hero: Hero, t: f32) -> vec3<f32> {
    let p = ray.origin + ray.dir * t;
    var light = hero.color;
    if (hero.kind == 0u) {
        let d = p - hero.position;
        let d2 = dot(d, d);
        light *= window(hero, d2) / max(d2, 1e-4) * cone(hero, p);
    }
    return medium(hero, p) * light * transmittance(ray, t * ray.depth_per_t);
}

// a fully lit run [a, b]: 4 Gauss-Legendre nodes, for a spot in the angle seen from the light
fn run(ray: Ray, hero: Hero, a: f32, b: f32) -> vec3<f32> {
    if (b <= a) {
        return vec3(0.0);
    }
    var sum = vec3(0.0);
    if (hero.kind == 0u) {
        // t = t_closest + h tan(theta): dt / d^2 = dtheta / h, so the inverse square goes and what's left is smooth
        let th_a = atan2(a - hero.t_closest, hero.h);
        let th_b = atan2(b - hero.t_closest, hero.h);
        for (var i = 0u; i < 4u; i += 1u) {
            let th = mix(th_a, th_b, GL4_X[i]);
            let t = hero.t_closest + hero.h * tan(th);
            let p = ray.origin + ray.dir * t;
            let d = p - hero.position;
            let d2 = dot(d, d);
            // radiance x d^2 / h, with the inverse square's floor kept
            let inv_sq = d2 / max(d2, 1e-4);
            sum += GL4_W[i] * medium(hero, p) * hero.color * window(hero, d2) * inv_sq * cone(hero, p) * transmittance(ray, t * ray.depth_per_t) / hero.h;
        }
        return sum * (th_b - th_a);
    }
    for (var i = 0u; i < 4u; i += 1u) {
        sum += GL4_W[i] * radiance(ray, hero, mix(a, b, GL4_X[i]));
    }
    return sum * (b - a);
}

// ---- the shadow map walk ----

// a point in the light's shadow map: texel coordinates (xy, the hardware filter's: uv x size - 0.5), the depth it's
// compared at (z) and, for a spot, 1 / its distance along the light's axis (w; 1 for a directional light)
fn to_shadow(hero: Hero, p: vec3<f32>) -> vec4<f32> {
    if (hero.kind == 0u) {
        let to_light = hero.position - p;
        let offset = -to_light + hero.bias * normalize(to_light);
        let q = offset * hero.rot;
        let z = max(-q.z, 1e-5);
        let ndc = q.xy / (hero.cone.z * z);
        let uv = ndc * vec2(0.5, -0.5) + 0.5;
        return vec4(uv * hero.size - 0.5, 0.1 / z, 1.0 / z);
    }
    let clip = hero.clip_from_world * vec4(p + hero.bias * hero.axis, 1.0);
    let ndc = clip.xyz / clip.w;
    let uv = ndc.xy * vec2(0.5, -0.5) + 0.5;
    return vec4(uv * hero.size - 0.5, ndc.z, 1.0);
}

// a piece of ray in the light's shadow map. Along the walk (lambda in [0, 1]) the texel coordinates, the compared
// depth and 1 / z are all linear; t isn't for a spot, and comes back perspective-correct (t / z is linear too)
struct Walk {
    sa: vec4<f32>,
    ds: vec2<f32>,
    dr: f32,
    ta: f32,
    tb: f32,
    wb: f32,
    // a fully lit run open since lit_from
    lit: bool,
    lit_from: f32,
    sum: vec3<f32>,
}

fn t_at(w: Walk, lam: f32) -> f32 {
    return mix(w.ta, w.tb, lam) / mix(w.sa.w, w.wb, lam);
}

fn lit_span(w: ptr<function, Walk>, lam0: f32) {
    if (!(*w).lit) {
        (*w).lit_from = t_at(*w, lam0);
        (*w).lit = true;
    }
}

fn dark_span(ray: Ray, hero: Hero, w: ptr<function, Walk>, lam0: f32) {
    if ((*w).lit) {
        (*w).sum += run(ray, hero, (*w).lit_from, t_at(*w, lam0));
        (*w).lit = false;
    }
}

// a block's nearest and farthest depth: level 1 to 4 the 4, 16, 64 and 256 texel blocks (froxel_minmax.wgsl)
fn block(level: u32, cell: vec2<i32>, layer: i32) -> vec2<f32> {
    switch level {
        case 1u: {
            return textureLoad(blocks4, clamp(cell, vec2(0), vec2<i32>(textureDimensions(blocks4)) - 1), layer, 0).xy;
        }
        case 2u: {
            return textureLoad(blocks16, clamp(cell, vec2(0), vec2<i32>(textureDimensions(blocks16)) - 1), layer, 0).xy;
        }
        case 3u: {
            return textureLoad(blocks64, clamp(cell, vec2(0), vec2<i32>(textureDimensions(blocks64)) - 1), layer, 0).xy;
        }
        default: {
            return textureLoad(blocks256, clamp(cell, vec2(0), vec2<i32>(textureDimensions(blocks256)) - 1), layer, 0).xy;
        }
    }
}

// the piece [a, b] of ray in the light, down its shadow map's min-max pyramid (froxel_minmax.wgsl): from each point
// on, a block the ray's depth is wholly in front of (lit) or behind (shadowed) is one span, and the walk moves past it
// and up a level once it leaves the block's parent; a block the depth crosses is looked at a level down, to the texel
// cells a shadow's edge crosses, integrated exactly (partial). Fully lit runs merged.
fn walk(ray: Ray, hero: Hero, a: f32, b: f32) -> vec3<f32> {
    var w: Walk;
    w.sa = to_shadow(hero, ray.origin + ray.dir * a);
    let sb = to_shadow(hero, ray.origin + ray.dir * b);
    w.ds = sb.xy - w.sa.xy;
    w.dr = sb.z - w.sa.z;
    w.ta = a * w.sa.w;
    w.tb = b * sb.w;
    w.wb = sb.w;
    w.lit = false;
    w.sum = vec3(0.0);
    let step = select(vec2(0.0), vec2(1.0), w.ds > vec2(0.0));
    // a thousandth of a texel along the walk, in lambda: where a point at a block's side is taken to be in the next
    let nudge = 1e-3 / max(length(w.ds), 1e-6);
    var lam = 0.0;
    var level = TOP;
    for (var n = 0u; n < MAX_STEPS && lam < 1.0; n += 1u) {
        let size = f32(1u << (2u * level));
        let probe = w.sa.xy + w.ds * min(lam + nudge, 1.0);
        let cell = floor(probe / size);
        let exits = select(vec2(1e30), ((cell + step) * size - w.sa.xy) / w.ds, abs(w.ds) > vec2(1e-9));
        let lam1 = clamp(min(exits.x, exits.y), lam + nudge, 1.0);
        let r0 = w.sa.z + w.dr * lam;
        let r1 = w.sa.z + w.dr * lam1;
        var d = vec4(0.0);
        var mm: vec2<f32>;
        if (level == 0u) {
            // the cell's corners, (0, 0) (1, 0) (0, 1) (1, 1), in one gather (its order: (0, 1) (1, 1) (1, 0) (0, 0))
            d = textureGather(view_bindings::directional_shadow_textures, nearest, (cell + 1.0) / hero.size, hero.layer).wzxy;
            mm = vec2(min(min(d.x, d.y), min(d.z, d.w)), max(max(d.x, d.y), max(d.z, d.w)));
        } else {
            mm = block(level, vec2<i32>(cell), hero.layer);
        }
        let lit = min(r0, r1) >= mm.y;
        let dark = max(r0, r1) < mm.x;
        if (!lit && !dark && level == 1u && n >= EXACT_STEPS) {
            dark_span(ray, hero, &w, lam);
            let a = t_at(w, lam);
            let b = t_at(w, lam1);
            let vis = saturate((0.5 * (r0 + r1) - mm.x) / max(mm.y - mm.x, 1e-9));
            w.sum += radiance(ray, hero, 0.5 * (a + b)) * (b - a) * vis;
            lam = lam1;
            if (lam1 < 1.0) {
                let next = floor((w.sa.xy + w.ds * min(lam1 + nudge, 1.0)) / (size * 4.0));
                if (any(next != floor(cell / 4.0))) {
                    level += 1u;
                }
            }
            continue;
        }
        if (!lit && !dark && level > 0u) {
            level -= 1u;
            continue;
        }
        if (lit) {
            lit_span(&w, lam);
        } else {
            dark_span(ray, hero, &w, lam);
            if (!dark) {
                w.sum += partial(ray, hero, w, cell, d, lam, lam1);
            }
        }
        // up a level once the walk leaves this block's parent
        if (level < TOP && lam1 < 1.0) {
            let next = floor((w.sa.xy + w.ds * min(lam1 + nudge, 1.0)) / (size * 4.0));
            if (any(next != floor(cell / 4.0))) {
                level += 1u;
            }
        }
        lam = lam1;
    }
    if (w.lit) {
        w.sum += run(ray, hero, w.lit_from, b);
    }
    return w.sum;
}

// a partly shadowed cell [lam0, lam1]: cut where each corner turns, the filter a fixed blend of the four between, each
// part integrated by 2 Gauss-Legendre nodes; the light's integrand is taken at the cell's lit centroid (it changes
// slowly across one texel), times the lit length
fn partial(ray: Ray, hero: Hero, w: Walk, cell: vec2<f32>, d: vec4<f32>, lam0: f32, lam1: f32) -> vec3<f32> {
    // where each corner's comparison flips
    var cuts = array<f32, 6>(lam0, lam1, lam1, lam1, lam1, lam1);
    var count = 1u;
    if (abs(w.dr) > 1e-12) {
        for (var c = 0u; c < 4u; c += 1u) {
            let l = (d[c] - w.sa.z) / w.dr;
            if (l > lam0 && l < lam1) {
                cuts[count] = l;
                count += 1u;
            }
        }
    }
    cuts[count] = lam1;
    count += 1u;
    for (var i = 1u; i < count; i += 1u) {
        let x = cuts[i];
        var j = i;
        while (j > 0u && cuts[j - 1u] > x) {
            cuts[j] = cuts[j - 1u];
            j -= 1u;
        }
        cuts[j] = x;
    }
    var mass = 0.0;
    var moment = 0.0;
    for (var i = 0u; i + 1u < count; i += 1u) {
        let l0 = cuts[i];
        let l1 = cuts[i + 1u];
        if (l1 <= l0) {
            continue;
        }
        // the corners lit in this part
        let rm = w.sa.z + w.dr * 0.5 * (l0 + l1);
        let bits = vec4<f32>(vec4(rm) >= d);
        let len = t_at(w, l1) - t_at(w, l0);
        for (var k = 0u; k < 2u; k += 1u) {
            let lam = mix(l0, l1, GL2_X[k]);
            let f = w.sa.xy + w.ds * lam - cell;
            let vis = mix(mix(bits.x, bits.y, f.x), mix(bits.z, bits.w, f.x), f.y);
            // the part's length in t, split evenly between its two nodes (t is near linear across a texel)
            let m = 0.5 * len * vis;
            mass += m;
            moment += m * t_at(w, lam);
        }
    }
    if (mass <= 0.0) {
        return vec3(0.0);
    }
    return radiance(ray, hero, moment / mass) * mass;
}

// ---- cutting the ray ----

fn add_cut(cuts: ptr<function, array<f32, CUTS>>, n: ptr<function, u32>, t: f32, end: f32) {
    if (t > 0.0 && t < end && *n < CUTS) {
        (*cuts)[*n] = t;
        *n += 1u;
    }
}

// where the ray crosses a cone around the spot's axis at cosine c
fn cone_cuts(cuts: ptr<function, array<f32, CUTS>>, n: ptr<function, u32>, ray: Ray, hero: Hero, c: f32, end: f32) {
    let w = ray.origin - hero.position;
    let ad = dot(hero.axis, ray.dir);
    let aw = dot(hero.axis, w);
    let c2 = c * c;
    let qa = ad * ad - c2;
    let qb = 2.0 * (aw * ad - c2 * dot(w, ray.dir));
    let qc = aw * aw - c2 * dot(w, w);
    if (abs(qa) < 1e-9) {
        if (abs(qb) > 1e-9) {
            add_cut(cuts, n, -qc / qb, end);
        }
        return;
    }
    let disc = qb * qb - 4.0 * qa * qc;
    if (disc < 0.0) {
        return;
    }
    let sq = sqrt(disc);
    add_cut(cuts, n, (-qb - sq) / (2.0 * qa), end);
    add_cut(cuts, n, (-qb + sq) / (2.0 * qa), end);
}

// the medium's cuts: where the ray enters and leaves each volume
fn volume_cuts(cuts: ptr<function, array<f32, CUTS>>, n: ptr<function, u32>, ray: Ray, end: f32) {
    for (var v = 0u; v < min(params.counts.x, CUT_VOLUMES); v += 1u) {
        let m = volumes[v].local_from_world;
        let o = (m * vec4(ray.origin, 1.0)).xyz;
        let d = (m * vec4(ray.dir, 0.0)).xyz;
        let inv = 1.0 / select(d, vec3(1e-12), abs(d) < vec3(1e-12));
        let t0 = (vec3(-0.5) - o) * inv;
        let t1 = (vec3(0.5) - o) * inv;
        let lo = max(max(min(t0.x, t1.x), min(t0.y, t1.y)), min(t0.z, t1.z));
        let hi = min(min(max(t0.x, t1.x), max(t0.y, t1.y)), max(t0.z, t1.z));
        if (hi > max(lo, 0.0)) {
            add_cut(cuts, n, lo, end);
            add_cut(cuts, n, hi, end);
        }
    }
}

fn sort_cuts(cuts: ptr<function, array<f32, CUTS>>, n: u32) {
    for (var i = 1u; i < n; i += 1u) {
        let x = (*cuts)[i];
        var j = i;
        while (j > 0u && (*cuts)[j - 1u] > x) {
            (*cuts)[j] = (*cuts)[j - 1u];
            j -= 1u;
        }
        (*cuts)[j] = x;
    }
}

// is any volume at this point
fn in_medium(p: vec3<f32>) -> bool {
    for (var v = 0u; v < params.counts.x; v += 1u) {
        let local = (volumes[v].local_from_world * vec4(p, 1.0)).xyz;
        if (all(abs(local) <= vec3(0.5))) {
            return true;
        }
    }
    return false;
}

// a piece [a, b] of the ray inside the medium, for one light
fn piece(ray: Ray, hero: Hero, a: f32, b: f32) -> vec3<f32> {
    let mid = ray.origin + ray.dir * (0.5 * (a + b));
    if (hero.kind == 0u && cone(hero, mid) <= 0.0) {
        return vec3(0.0);
    }
    if (!hero.shadows) {
        return run(ray, hero, a, b);
    }
    return walk(ray, hero, a, b);
}

// ---- the lights ----

fn spot_hero(id: u32, ray: Ray) -> Hero {
    let light = &clustered_lights.data[id];
    var h: Hero;
    h.kind = 0u;
    h.index = id;
    h.color = (*light).color_inverse_square_range.rgb;
    h.position = (*light).position_radius.xyz;
    var spot_dir = vec3<f32>((*light).light_custom_data.x, 0.0, (*light).light_custom_data.y);
    spot_dir.y = sqrt(max(0.0, 1.0 - spot_dir.x * spot_dir.x - spot_dir.z * spot_dir.z));
    if (((*light).flags & POINT_LIGHT_FLAGS_SPOT_LIGHT_Y_NEGATIVE) != 0u) {
        spot_dir.y = -spot_dir.y;
    }
    h.axis = spot_dir;
    h.cone = vec4((*light).light_custom_data.zw, (*light).spot_light_tan_angle, (*light).color_inverse_square_range.w);
    h.shadows = ((*light).flags & POINT_LIGHT_FLAGS_SHADOWS_ENABLED_BIT) != 0u;
    h.t_closest = dot(h.position - ray.origin, ray.dir);
    h.h = max(length(ray.origin + ray.dir * h.t_closest - h.position), 1e-3);
    h.layer = i32(id) + lights.spot_light_shadowmap_offset;
    h.size = vec2<f32>(textureDimensions(view_bindings::directional_shadow_textures).xy);
    h.rot = orthonormalize(-spot_dir);
    h.bias = (*light).shadow_depth_bias;
    return h;
}

// the k-th volumetric directional light (prepare_lights sorts them after the others)
fn volumetric_sun(k: u32) -> u32 {
    var seen = 0u;
    for (var i = 0u; i < lights.n_directional_lights; i += 1u) {
        if ((lights.directional_lights[i].flags & DIRECTIONAL_LIGHT_FLAGS_VOLUMETRIC_BIT) != 0u) {
            if (seen == k) {
                return i;
            }
            seen += 1u;
        }
    }
    return 0u;
}

fn directional_hero(k: u32, ray: Ray) -> Hero {
    let li = volumetric_sun(k);
    let light = &lights.directional_lights[li];
    var h: Hero;
    h.kind = 1u;
    h.index = li;
    h.color = (*light).color.rgb;
    h.axis = (*light).direction_to_light.xyz;
    h.cos_view = dot(normalize(h.axis), ray.dir);
    // always walked: Bevy's march samples a directional light's shadow map whether it casts shadows or not
    h.shadows = true;
    h.h = 1.0;
    h.size = vec2<f32>(textureDimensions(view_bindings::directional_shadow_textures).xy);
    h.bias = (*light).shadow_depth_bias;
    return h;
}

// a directional light along [a, b]: each cascade over its depth range, clipped to its shadow map's box (outside it
// Bevy's march has no light), as world_to_directional_light_local
fn directional(ray: Ray, hero_in: Hero, a: f32, b: f32) -> vec3<f32> {
    var hero = hero_in;
    let light = &lights.directional_lights[hero.index];
    var sum = vec3(0.0);
    var near_depth = 0.0;
    for (var c = 0u; c < (*light).num_cascades; c += 1u) {
        let far_depth = (*light).cascades[c].far_bound;
        let ca = max(a, near_depth / ray.depth_per_t);
        let cb = min(b, far_depth / ray.depth_per_t);
        near_depth = far_depth;
        if (cb <= ca) {
            continue;
        }
        hero.clip_from_world = (*light).cascades[c].clip_from_world;
        hero.layer = i32((*light).depth_texture_base_index + c);
        // the box: ndc x, y in [-1, 1], z in [0, 1], all linear along the ray
        let p0 = hero.clip_from_world * vec4(ray.origin + hero.bias * hero.axis, 1.0);
        let pd = hero.clip_from_world * vec4(ray.dir, 0.0);
        var lo = ca;
        var hi = cb;
        for (var k = 0u; k < 3u; k += 1u) {
            let low = select(-1.0, 0.0, k == 2u);
            if (abs(pd[k]) < 1e-12) {
                if (p0[k] < low || p0[k] > 1.0) {
                    hi = lo;
                }
                continue;
            }
            let u0 = (low - p0[k]) / pd[k];
            let u1 = (1.0 - p0[k]) / pd[k];
            lo = max(lo, min(u0, u1));
            hi = min(hi, max(u0, u1));
        }
        if (hi > lo) {
            sum += piece(ray, hero, lo, hi);
        }
    }
    return sum;
}

// ---- one pixel's ray, for one light or all ----

// the ray through a point of the frame (pixels) to the surface there (its depth buffer's value), or past the grid
fn ray_at(px: vec2<f32>) -> Ray {
    let uv = (px - view.viewport.xy) / view.viewport.zw;
    let q = position_ndc_to_view(vec3(uv * vec2(2.0, -2.0) + vec2(-1.0, 1.0), 1.0));
    var ray: Ray;
    ray.origin = view.world_position;
    let dir_view = normalize(q);
    ray.dir = normalize((view.world_from_view * vec4(dir_view, 0.0)).xyz);
    ray.depth_per_t = -dir_view.z;
    ray.uv = (px - params.viewport.xy) / (params.slab.x * vec2<f32>(vec2(params.grid.x, params.grid.y)));
    return ray;
}

// the view depth (metres) of the surface at a pixel, or the grid's far depth past everything
fn depth_at(pixel: vec2<i32>) -> f32 {
    let ndc_depth = textureLoad(depth_texture, pixel, 0);
    var depth = params.slab.z;
    if (ndc_depth > 0.0) {
        let p = params.view_from_clip * vec4(0.0, 0.0, ndc_depth, 1.0);
        depth = min(-p.z / p.w, depth);
    }
    return depth;
}

// the k-th hero light that's on: 0, 1 the spots, 2, 3 the directional lights
fn slot_light(k: u32) -> u32 {
    var seen = 0u;
    for (var hi = 0u; hi < 4u; hi += 1u) {
        var on = false;
        if (hi < 2u) {
            on = select(params.heroes.y, params.heroes.x, hi == 0u) != NONE;
        } else {
            on = hi - 2u < params.heroes.z;
        }
        if (on) {
            if (seen == k) {
                return hi;
            }
            seen += 1u;
        }
    }
    return NONE;
}

// one hero light's shaft along a ray to view depth `depth`
fn shaft(ray: Ray, depth: f32, hi: u32) -> vec3<f32> {
    let end = depth / ray.depth_per_t;
    var cuts: array<f32, CUTS>;
    var n = 0u;
    volume_cuts(&cuts, &n, ray, end);
    var hero: Hero;
    if (hi < 2u) {
        hero = spot_hero(select(params.heroes.y, params.heroes.x, hi == 0u), ray);
        // the cone's outer and inner edges
        cone_cuts(&cuts, &n, ray, hero, -hero.cone.y / hero.cone.x, end);
        cone_cuts(&cuts, &n, ray, hero, (1.0 - hero.cone.y) / hero.cone.x, end);
    } else {
        hero = directional_hero(hi - 2u, ray);
    }
    add_cut(&cuts, &n, 1e-6, end);
    add_cut(&cuts, &n, end * (1.0 - 1e-6), end);
    sort_cuts(&cuts, n);
    var sum = vec3(0.0);
    for (var i = 0u; i + 1u < n; i += 1u) {
        let a = cuts[i];
        let b = cuts[i + 1u];
        if (b <= a || !in_medium(ray.origin + ray.dir * (0.5 * (a + b)))) {
            continue;
        }
        if (hero.kind == 0u) {
            sum += piece(ray, hero, a, b);
        } else {
            sum += directional(ray, hero, a, b);
        }
    }
    return sum;
}

// every hero light at a pixel, exactly
fn exact(px: vec2<f32>) -> vec4<f32> {
    let ray = ray_at(px);
    let depth = depth_at(vec2<i32>(px));
    var sum = vec3(0.0);
    for (var k = 0u; k < 4u; k += 1u) {
        let hi = slot_light(k);
        if (hi == NONE) {
            break;
        }
        sum += shaft(ray, depth, hi);
    }
    return vec4(sum * view.exposure, 0.0);
}

// at each pixel (RAEA_FOG_HERO_EXACT: what the epipolar passes are checked against)
@fragment
fn hero(in: FullscreenVertexOutput) -> @location(0) vec4<f32> {
    return exact(in.position.xy);
}


// ---- epipolar sampling (Chen et al. 2011) ----
//
// Seen from the camera, a light's shafts run along lines through the light's place in the frame (its epipolar lines):
// along one, the light scattered toward the eye changes smoothly except where the depth behind it jumps, while across
// lines it changes as sharply as the shadows do. So each hero light's shafts are integrated only at samples along
// `slices` lines from it to each point of the frame's border (as many lines as border pixels / 1.5, so neighbours
// are at most a pixel and a half apart), `along` samples a line: exactly at every K-th sample, at every sample of a
// stretch where the depth jumps, and wherever the two ends of a stretch differ by more than interpolating between them
// would get right; the rest are interpolated. Each pixel then reads its light from the two lines either side of it,
// at its distance along, weighted to the samples at its own depth; a pixel with none at its depth (a thin edge) is
// integrated itself. Each light's samples are a band of rows `along` high, the k-th light's from row k x along.

// light k's part of the frame (pixels; froxel.rs's hero_rect: where its cone, and the fog, can be): its lines run to
// that rectangle's border, and outside it the light adds nothing
fn frame_lo(k: u32) -> vec2<f32> {
    return params.hero_rect[k].xy;
}

fn frame_hi(k: u32) -> vec2<f32> {
    return params.hero_rect[k].zw;
}

// light k's lines, and samples along each at most
fn lines(k: u32) -> u32 {
    return params.hero_lines[k];
}

fn along(k: u32) -> u32 {
    return params.hero_along[k];
}

// the light's place in the frame (pixels): a spot's position, a directional light's direction; behind the camera its
// projection is where the lines meet as well, and one in the frame's plane is put far off along its direction
fn light_px(hi: u32) -> vec2<f32> {
    var clip: vec4<f32>;
    if (hi < 2u) {
        clip = view.clip_from_world * vec4(clustered_lights.data[select(params.heroes.y, params.heroes.x, hi == 0u)].position_radius.xyz, 1.0);
    } else {
        clip = view.clip_from_world * vec4(lights.directional_lights[volumetric_sun(hi - 2u)].direction_to_light.xyz, 0.0);
    }
    let w = select(max(clip.w, 1e-6), min(clip.w, -1e-6), clip.w < 0.0);
    var ndc = clip.xy / w;
    let far = 1e4;
    if (abs(clip.w) < 1e-6 * length(clip.xy)) {
        ndc = normalize(clip.xy) * far;
    }
    ndc = clamp(ndc, vec2(-far), vec2(far));
    return view.viewport.xy + (ndc * vec2(0.5, -0.5) + 0.5) * view.viewport.zw;
}

// where the line o + t d is inside light k's rectangle: (t in, t out)
fn clip_frame(k: u32, o: vec2<f32>, d: vec2<f32>) -> vec2<f32> {
    let inv = 1.0 / select(d, vec2(1e-12), abs(d) < vec2(1e-12));
    let t0 = (frame_lo(k) - o) * inv;
    let t1 = (frame_hi(k) - o) * inv;
    return vec2(max(min(t0.x, t1.x), min(t0.y, t1.y)), min(max(t0.x, t1.x), max(t0.y, t1.y)));
}

// a point on the rectangle's border by how far round it is, in [0, 1): along the top, down the right, back along the
// bottom, up the left
fn border_point(k: u32, u: f32) -> vec2<f32> {
    let lo = frame_lo(k);
    let size = frame_hi(k) - lo;
    var s = fract(u) * 2.0 * (size.x + size.y);
    if (s < size.x) {
        return lo + vec2(s, 0.0);
    }
    s -= size.x;
    if (s < size.y) {
        return lo + vec2(size.x, s);
    }
    s -= size.y;
    if (s < size.x) {
        return lo + vec2(size.x - s, size.y);
    }
    s -= size.x;
    return lo + vec2(0.0, size.y - s);
}

// and back: how far round the border a point on it is
fn border_param(k: u32, p: vec2<f32>) -> f32 {
    let lo = frame_lo(k);
    let size = frame_hi(k) - lo;
    let q = clamp(p - lo, vec2(0.0), size);
    let per = 2.0 * (size.x + size.y);
    // the nearest side
    let dist = vec4(q.y, size.x - q.x, size.y - q.y, q.x);
    let m = min(min(dist.x, dist.y), min(dist.z, dist.w));
    if (m == dist.x) {
        return q.x / per;
    }
    if (m == dist.y) {
        return (size.x + q.y) / per;
    }
    if (m == dist.z) {
        return (2.0 * size.x + size.y - q.x) / per;
    }
    return fract((2.0 * size.x + 2.0 * size.y - q.y) / per);
}

// a line from the light: where it enters the rectangle (a) and leaves it (e, on the border at u), or none (its exit
// isn't where it leaves: the light is outside the rectangle and the border point faces it)
struct Line {
    a: vec2<f32>,
    e: vec2<f32>,
    ok: bool,
}

fn slice_line(k: u32, l: vec2<f32>, u: f32) -> Line {
    var line: Line;
    line.e = border_point(k, u);
    let d = line.e - l;
    let t = clip_frame(k, l, d);
    line.ok = t.y > t.x && t.y <= 1.0 + 1e-4 && dot(d, d) > 1e-8;
    line.a = l + d * clamp(t.x, 0.0, 1.0);
    return line;
}

// light k's line `slice`
fn line_of(k: u32, slice: u32) -> Line {
    return slice_line(k, light_px(slot_light(k)), (f32(slice) + 0.5) / f32(lines(k)));
}

// a pixel's place among light k's lines (from l): (slice, sample), continuous, in samples (spaced epi_tol.w pixels
// apart from where the line enters the rectangle)
fn epipolar_of(k: u32, l: vec2<f32>, px: vec2<f32>) -> vec2<f32> {
    let d = px - l;
    let t = clip_frame(k, l, d);
    let e = l + d * max(t.y, 1.0);
    let u = border_param(k, e);
    let line = slice_line(k, l, u);
    return vec2(u * f32(lines(k)) - 0.5, min(distance(line.a, px) / params.epi_tol.w, f32(along(k) - 1u)));
}

// the sample (slice, j) of light k: its point in the frame, or none (past the line's end: the last is at its end)
fn sample_px(k: u32, slice: u32, j: u32, ok: ptr<function, bool>) -> vec2<f32> {
    let line = line_of(k, slice);
    let len = distance(line.a, line.e);
    let at = f32(j) * params.epi_tol.w;
    *ok = slot_light(k) != NONE && slice < lines(k) && line.ok && at < len + params.epi_tol.w;
    return line.a + (line.e - line.a) * min(at / max(len, 1e-6), 1.0);
}

// the depth at a point of the frame
fn depth_of(px: vec2<f32>) -> f32 {
    let p = clamp(vec2<i32>(floor(px)), vec2<i32>(view.viewport.xy), vec2<i32>(view.viewport.xy + view.viewport.zw) - 1);
    return depth_at(p);
}

// the depth at light k's (slice, j) sample, 0 for none
fn sample_depth(k: u32, slice: u32, j: u32) -> f32 {
    var ok = false;
    let px = sample_px(k, slice, j, &ok);
    return select(0.0, depth_of(px), ok);
}

// the texel of light k's (slice, j) sample
fn texel(k: u32, slice: u32, j: u32) -> vec2<i32> {
    return vec2<i32>(i32(slice), i32(k * params.epi.y + j));
}

// does the depth jump between two samples (or is one of them none, depth 0)
fn breaks(a: f32, b: f32) -> bool {
    return min(a, b) <= 0.0 || abs(a - b) > params.epi_tol.z * min(a, b);
}

// do three samples in a row lie on one surface: 1 / depth goes linearly along a line across a plane, so its second
// difference is what bends (a floor seen edge-on changes depth fast, but evenly)
fn smooth3(a: f32, b: f32, c: f32) -> bool {
    if (min(a, min(b, c)) <= 0.0) {
        return false;
    }
    return abs(1.0 / a - 2.0 / b + 1.0 / c) <= params.epi_tol.z / b;
}

// a pass's light before it: the coarse samples', or every sample's (with its depth in a, 0 for none)
@group(2) @binding(0) var epi_light: texture_2d<f32>;

// the coarse samples: every K-th along a line and its last, each light's a band of `coarse_rows()` rows of their own
// texture, so the pass integrating them integrates at every texel
fn coarse_rows() -> u32 {
    return (params.epi.y - 1u + params.epi.z - 1u) / params.epi.z + 1u;
}

// the last sample on light k's line `slice` (at its end)
fn line_last(k: u32, slice: u32) -> u32 {
    let line = line_of(k, slice);
    return min(u32(ceil(distance(line.a, line.e) / params.epi_tol.w)), along(k) - 1u);
}

// the coarse sample r of a line whose last sample is `last`
fn coarse_j(r: u32, last: u32) -> u32 {
    return min(r * params.epi.z, last);
}

fn coarse_texel(k: u32, slice: u32, r: u32) -> vec2<i32> {
    return vec2<i32>(i32(slice), i32(k * coarse_rows() + r));
}

// a sample's light (exposed) and its depth
fn integrate_sample(k: u32, slice: u32, j: u32) -> vec4<f32> {
    var ok = false;
    let px = sample_px(k, slice, j, &ok);
    let depth = select(0.0, depth_of(px), ok);
    if (depth <= 0.0) {
        return vec4(0.0);
    }
    return vec4(shaft(ray_at(px), depth, slot_light(k)) * view.exposure, depth);
}

// pass 2: the coarse samples integrated
@fragment
fn coarse(in: FullscreenVertexOutput) -> @location(0) vec4<f32> {
    let t = vec2<u32>(in.position.xy);
    let rows = coarse_rows();
    let k = t.y / rows;
    let r = t.y % rows;
    if (t.x >= lines(k)) {
        return vec4(0.0);
    }
    let last = line_last(k, t.x);
    if (r * params.epi.z >= last + params.epi.z) {
        return vec4(0.0);
    }
    return integrate_sample(k, t.x, coarse_j(r, last));
}

struct Marked {
    @location(0) light: vec4<f32>,
    @builtin(frag_depth) depth: f32,
}

// pass 3: every sample's light: the coarse ones' copied, the rest interpolated between their stretch's ends; or, in a
// stretch whose depth jumps or whose ends differ by more than interpolating would get right, marked (depth 0) for pass
// 4 to integrate
@fragment
fn mark(in: FullscreenVertexOutput) -> Marked {
    let t = vec2<u32>(in.position.xy);
    let k = t.y / params.epi.y;
    let j = t.y % params.epi.y;
    var out: Marked;
    out.depth = 1.0;
    out.light = vec4(0.0);
    // the stretch's samples are on one line: its depths read straight from the frame's
    let line = line_of(k, t.x);
    let len = max(distance(line.a, line.e), 1e-6);
    let own = sample_depth(k, t.x, j);
    if (own <= 0.0) {
        return out;
    }
    let kk = params.epi.z;
    let r0 = j / kk;
    let last = line_last(k, t.x);
    let j0 = coarse_j(r0, last);
    let j1 = coarse_j(r0 + 1u, last);
    let a = textureLoad(epi_light, coarse_texel(k, t.x, r0), 0).rgb;
    let b = textureLoad(epi_light, coarse_texel(k, t.x, r0 + 1u), 0).rgb;
    if (j == j0 || j == j1) {
        out.light = vec4(select(a, b, j == j1), own);
        return out;
    }
    var jump = any(abs(a - b) > params.epi_tol.x * max(a, b) + params.epi_tol.y);
    // a bend in the surface anywhere from just before the stretch to just after it
    let lo = max(j0, 1u) - 1u;
    let hi = min(j1 + 1u, last);
    let step = (line.e - line.a) * (params.epi_tol.w / len);
    var d0 = depth_of(line.a + step * f32(lo));
    var d1 = depth_of(line.a + step * f32(lo + 1u));
    for (var i = lo + 2u; i <= hi && !jump; i += 1u) {
        // (the last at the line's end)
        let d2 = depth_of(select(line.a + step * f32(i), line.e, i == last));
        jump = !smooth3(d0, d1, d2);
        d0 = d1;
        d1 = d2;
    }
    if (jump) {
        out.depth = 0.0;
        return out;
    }
    out.light = vec4(mix(a, b, f32(j - j0) / f32(j1 - j0)), own);
    return out;
}

// pass 4's triangle over the samples at depth one half: drawn only where pass 3 marked (depth 0, the test greater),
// the depth test culling the rest before they're shaded, so the samples integrated are packed together
@vertex
fn at_half(@builtin(vertex_index) i: u32) -> @builtin(position) vec4<f32> {
    let uv = vec2<f32>(f32((i << 1u) & 2u), f32(i & 2u));
    return vec4(uv * 2.0 - 1.0, 0.5, 1.0);
}

// pass 4: the marked samples integrated
@fragment
fn fine(@builtin(position) position: vec4<f32>) -> @location(0) vec4<f32> {
    let t = vec2<u32>(position.xy);
    return integrate_sample(t.y / params.epi.y, t.x, t.y % params.epi.y);
}

// pass 5: each pixel's light from the samples either side of it, at its depth (exposed already); a pixel with none
// at its depth near it (a thin edge, 0.03% of curia's frame) takes the sample nearest its depth. Not integrated
// itself: the integrator in this pass would hold registers every pixel pays for, and in a pass of its own the few
// pixels' longest walk is what the pass takes (0.4-0.6 ms on curia for 629 pixels)
// with the grid's fog: one draw over the frame for both (blended as the grid's apply: frame x a + rgb)
@fragment
fn epipolar(in: FullscreenVertexOutput) -> @location(0) vec4<f32> {
    let px = in.position.xy;
    let fog = grid_fog(px);
    return vec4(fog.rgb + shafts_at(px), fog.a);
}

fn shafts_at(px: vec2<f32>) -> vec3<f32> {
    let depth = depth_at(vec2<i32>(px));
    var sum = vec3(0.0);
    for (var k = 0u; k < 4u; k += 1u) {
        let hi = slot_light(k);
        if (hi == NONE) {
            break;
        }
        if (any(px < frame_lo(k)) || any(px > frame_hi(k))) {
            continue;
        }
        let n = i32(lines(k));
        let c = epipolar_of(k, light_px(hi), px);
        let s0 = floor(c.x);
        let fs = c.x - s0;
        let j0 = min(u32(floor(c.y)), max(along(k), 2u) - 2u);
        let fj = c.y - f32(j0);
        var acc = vec3(0.0);
        var wsum = 0.0;
        for (var ds = 0u; ds < 2u; ds += 1u) {
            // round the border: the last slice's neighbour is the first
            let slice = u32((i32(s0) + i32(ds) + n) % n);
            let ws = select(1.0 - fs, fs, ds == 1u);
            let ta = texel(k, slice, j0);
            let tb = texel(k, slice, j0 + 1u);
            let la = textureLoad(epi_light, ta, 0);
            let lb = textureLoad(epi_light, tb, 0);
            let da = la.a;
            let db = lb.a;
            // both, where the pixel is on the surface between them (1 / depth interpolated); else each at its depth
            let on = min(da, db) > 0.0 && abs(1.0 / depth - mix(1.0 / da, 1.0 / db, fj)) <= params.epi_tol.z / depth;
            let wa = ws * (1.0 - fj) * f32(on || !breaks(da, depth));
            let wb = ws * fj * f32(on || !breaks(db, depth));
            acc += wa * la.rgb + wb * lb.rgb;
            wsum += wa + wb;
        }
        if (wsum <= 1e-3) {
            // none of the four at its depth (an edge): the nearest along the two lines that is, a few samples on, or
            // failing that the one nearest its depth
            var best = vec3(0.0);
            var best_off = 1e30;
            for (var ds = 0u; ds < 2u; ds += 1u) {
                let slice = u32((i32(s0) + i32(ds) + n) % n);
                let ws = select(1.0 - fs, fs, ds == 1u) + 1e-3;
                for (var dj = -3; dj <= 4; dj += 1) {
                    let jj = i32(j0) + dj;
                    if (jj < 0 || jj >= i32(along(k))) {
                        continue;
                    }
                    let tx = texel(k, slice, u32(jj));
                    let l4 = textureLoad(epi_light, tx, 0);
                    let d = l4.a;
                    if (d <= 0.0) {
                        continue;
                    }
                    let light = l4.rgb;
                    if (!breaks(d, depth)) {
                        let w = ws / f32(abs(dj - select(0, 1, dj > 0)) + 1);
                        acc += w * light;
                        wsum += w;
                    }
                    let off = abs(d - depth);
                    if (off < best_off) {
                        best_off = off;
                        best = light;
                    }
                }
            }
            if (wsum <= 1e-3) {
#ifdef SHOW_FALLBACK
                best = vec3(4.0, 0.0, 4.0);
#endif
                acc = best;
                wsum = 1.0;
            }
        }
        sum += acc / wsum;
    }
    return sum;
}
