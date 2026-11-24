// raea: the participating media the reference path tracer walks: the scene's fog volumes, as raea_sys_fog's froxel
// grid reads them (froxel_volume.wesl: each a box with a density shaped by its falloff, soft faces and taper), so the
// tracer is the truth the froxel passes are measured against. Extinction is grey (the volume's density x (absorption +
// scattering)), scattering coloured (fog colour x scattering x density), the phase Henyey-Greenstein.
//
// A path's segment finds its next collision by delta tracking against a majorant, the sum of the extinctions of the
// volumes the segment crosses (a volume's shape only thins it); a collision scatters the path, a null one passes.
// A shadow ray's transmittance is ratio tracked against the same majorant. Both are unbiased.
//
// This imports raea_sys_fog's shader library, which raea always installs beside Solari.

import bevy_pbr::render::utils::rand_f;
import package::scene::bindings::RAY_T_MAX;
import raea_sys_fog::froxel_volume::{Volume, volume_weight};

// pathtracer/node.rs: PathtracerMedia's buffer (a single empty volume when the scene has none)
@group(1) @binding(3) var<storage, read> volumes: array<Volume>;

// enough null collisions for a ray through the densest box the hall's steam makes, many times over
const MAX_STEPS: u32 = 512u;

struct MediaSpan {
    t0: f32,
    t1: f32,
    majorant: f32,
}

struct MediumCollision {
    // where along the ray, or -1 when it reaches t_max
    t: f32,
    // the scattering albedo (scattering over extinction, per channel)
    albedo: vec3<f32>,
    // the scattering volume's anisotropy
    g: f32,
}

// the part of [t_min, t_max] inside any volume, and the sum of the extinctions of the volumes it crosses
fn media_span(origin: vec3<f32>, direction: vec3<f32>, t_min: f32, t_max: f32) -> MediaSpan {
    var span = MediaSpan(t_max, t_min, 0.0);
    for (var v = 0u; v < arrayLength(&volumes); v++) {
        let extinction = volumes[v].scatter.a;
        if extinction <= 0.0 { continue; }
        let m = volumes[v].local_from_world;
        let o = (m * vec4(origin, 1.0)).xyz;
        let d = (m * vec4(direction, 0.0)).xyz;
        // the box -0.5..0.5, by slabs (an axis the ray runs along gives +-inf, which min and max handle)
        let inv = 1.0 / d;
        let a = (vec3(-0.5) - o) * inv;
        let b = (vec3(0.5) - o) * inv;
        let near = max(max(min(a.x, b.x), min(a.y, b.y)), max(min(a.z, b.z), t_min));
        let far = min(min(max(a.x, b.x), max(a.y, b.y)), min(max(a.z, b.z), t_max));
        if near < far {
            span.t0 = min(span.t0, near);
            span.t1 = max(span.t1, far);
            span.majorant += extinction;
        }
    }
    return span;
}

// the extinction at p summed over the volumes, and the volume that `pick` (0 to that sum) falls in: x its index
// (or none), y the sum
fn extinction_at(p: vec3<f32>, pick: f32) -> vec2<f32> {
    var sum = 0.0;
    var picked = -1.0;
    for (var v = 0u; v < arrayLength(&volumes); v++) {
        let extinction = volumes[v].scatter.a;
        if extinction <= 0.0 { continue; }
        let local = (volumes[v].local_from_world * vec4(p, 1.0)).xyz;
        sum += extinction * volume_weight(volumes[v].shape, local);
        if picked < 0.0 && pick < sum { picked = f32(v); }
    }
    return vec2(picked, sum);
}

// the ray's first real collision with the media between t_min and t_max, by delta tracking
fn media_collision(origin: vec3<f32>, direction: vec3<f32>, t_min: f32, t_max: f32, rng: ptr<function, u32>) -> MediumCollision {
    let missed = MediumCollision(-1.0, vec3(1.0), 0.0);
    let span = media_span(origin, direction, t_min, t_max);
    if span.majorant <= 0.0 || span.t0 >= span.t1 { return missed; }
    var t = span.t0;
    for (var i = 0u; i < MAX_STEPS; i++) {
        t -= log(1.0 - rand_f(rng)) / span.majorant;
        if t >= span.t1 { return missed; }
        let hit = extinction_at(origin + direction * t, rand_f(rng) * span.majorant);
        if hit.x >= 0.0 {
            let vol = volumes[u32(hit.x)];
            return MediumCollision(t, vol.ambient.rgb / vol.scatter.a, vol.ambient.a);
        }
    }
    return missed;
}

// how much of the light gets through the media between t_min and t_max, by ratio tracking (a light at infinity:
// t_max RAY_T_MAX, which the volumes' boxes cut short)
fn media_transmittance(origin: vec3<f32>, direction: vec3<f32>, t_min: f32, t_max: f32, rng: ptr<function, u32>) -> f32 {
    let span = media_span(origin, direction, t_min, min(t_max, RAY_T_MAX));
    if span.majorant <= 0.0 || span.t0 >= span.t1 { return 1.0; }
    var t = span.t0;
    var through = 1.0;
    for (var i = 0u; i < MAX_STEPS; i++) {
        t -= log(1.0 - rand_f(rng)) / span.majorant;
        if t >= span.t1 { break; }
        through *= 1.0 - extinction_at(origin + direction * t, 0.0).y / span.majorant;
        // nearly none left: roulette it out, keeping the estimate unbiased
        if through < 0.05 {
            if rand_f(rng) >= through * 10.0 { return 0.0; }
            through = 0.1;
        }
    }
    return through;
}
