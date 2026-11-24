// A fog volume's shape as the froxel passes read it (froxel_light, froxel_hero, froxel_solari): how much of its density
// a point in its box has. froxel.rs packs `shape`: x the thinning per metre going up (`falloff`), y the box's height in
// metres, z the feathered share of the box's half-size at every face (`soft`), w how much narrower its floor is than
// its top (`taper`). The reference path tracer walks the same volumes (bevy_solari pathtracer/media.wesl).

import bevy_render::maths::{PI_2, orthonormalize};

// froxel.rs GpuVolume
struct Volume {
    local_from_world: mat4x4<f32>,
    // rgb: the light it scatters toward the eye per metre from Bevy's lights; a: extinction per metre
    scatter: vec4<f32>,
    // rgb: the scattering coefficient per metre (fog colour x scattering x density); a: anisotropy
    ambient: vec4<f32>,
    shape: vec4<f32>,
    // x: the extinction per metre that dims light on its way to a point (froxel.rs, RAEA_FOG_SHADOW's rule)
    shadow: vec4<f32>,
}

// the optical depth light crosses in one volume on its way from p along dir (unit, world) for up to t_max metres: its
// `shadow` extinction x its shaped density along the part inside its box: closed for a plain box, else by 4
// Gauss-Legendre nodes (smooth, so the grid's cells and the hero pass's quadrature see no noise from it)
fn volume_depth(vol: Volume, p: vec3<f32>, dir: vec3<f32>, t_max: f32) -> f32 {
    if (vol.shadow.x <= 0.0) {
        return 0.0;
    }
    let o = (vol.local_from_world * vec4(p, 1.0)).xyz;
    let d = (vol.local_from_world * vec4(dir, 0.0)).xyz;
    // the box -0.5..0.5, by slabs (an axis the ray runs along gives +-inf, which min and max handle)
    let inv = 1.0 / d;
    let a = (vec3(-0.5) - o) * inv;
    let b = (vec3(0.5) - o) * inv;
    let near = max(max(min(a.x, b.x), min(a.y, b.y)), max(min(a.z, b.z), 0.0));
    let far = min(min(max(a.x, b.x), max(a.y, b.y)), min(max(a.z, b.z), t_max));
    if (near >= far) {
        return 0.0;
    }
    // a plain box (no soft faces, no taper: a hall's haze) thins as exp(-falloff x metres up), whose integral along
    // the ray is closed: exp(-k (up at near)) (1 - exp(-k dy (far - near))) / (k dy), k = falloff x height
    if (vol.shape.z <= 0.0 && vol.shape.w <= 0.0) {
        let k = vol.shape.x * vol.shape.y;
        let at_near = exp(-k * (o.y + 0.5 + d.y * near));
        let x = k * d.y * (far - near);
        var span = far - near;
        if (abs(x) > 1e-4) {
            span *= (1.0 - exp(-x)) / x;
        }
        return vol.shadow.x * at_near * span;
    }
    // unrolled, the nodes as literals
    let sum = 0.17392742256872692 * (volume_weight(vol.shape, o + d * mix(near, far, 0.06943184420297371)) + volume_weight(vol.shape, o + d * mix(near, far, 0.9305681557970263)))
        + 0.32607257743127307 * (volume_weight(vol.shape, o + d * mix(near, far, 0.33000947820757187)) + volume_weight(vol.shape, o + d * mix(near, far, 0.6699905217924281)));
    return vol.shadow.x * (far - near) * sum;
}

const FRAC_4_PI: f32 = 0.07957747154594767;

// the Henyey-Greenstein phase, per steradian, for the cosine between the direction light travels on and the one it
// came in on reversed: with V the eye ray's direction and L toward the light, cos_theta = dot(L, V), and g > 0 puts
// the glow round a light ahead of the eye
fn henyey_greenstein(g: f32, cos_theta: f32) -> f32 {
    let denom = 1.0 + g * g - 2.0 * g * cos_theta;
    return FRAC_4_PI * (1.0 - g * g) / (denom * sqrt(denom));
}

// a direction toward the light drawn from the phase about the view's direction V (pdf: the phase itself)
fn sample_phase(g: f32, V: vec3<f32>, u: vec2<f32>) -> vec3<f32> {
    var cos_theta = 1.0 - 2.0 * u.x;
    if (abs(g) > 1e-3) {
        let s = (1.0 - g * g) / (1.0 - g + 2.0 * g * u.x);
        cos_theta = clamp((1.0 + g * g - s * s) / (2.0 * g), -1.0, 1.0);
    }
    let sin_theta = sqrt(max(0.0, 1.0 - cos_theta * cos_theta));
    let phi = u.y * PI_2;
    return orthonormalize(V) * vec3(cos(phi) * sin_theta, sin(phi) * sin_theta, cos_theta);
}

// The density at `local` (the box's own coordinates, -0.5 to 0.5) over the volume's density: 0 outside its shape.
// Thinning is exp(-falloff x metres above the floor). Taper narrows the box's width and depth linearly toward the floor,
// to 1 - taper of the top's (a plume rising off a tank). Soft ramps the density in from each face (the tapered sides
// too) over that share of the half-size with a smoothstep, so the box's edges don't show (0: a hard box).
fn volume_weight(shape: vec4<f32>, local: vec3<f32>) -> f32 {
    let up = local.y + 0.5;
    let wide = max(1.0 - shape.w * (1.0 - up), 1e-3);
    let across = abs(local.xz) / wide;
    if (any(across > vec2(0.5)) || abs(local.y) > 0.5) {
        return 0.0;
    }
    var w = exp(-shape.x * up * shape.y);
    if (shape.z > 0.0) {
        let inside = (vec3(0.5) - vec3(across.x, abs(local.y), across.y)) * 2.0 / shape.z;
        let e = clamp(inside, vec3(0.0), vec3(1.0));
        let s = e * e * (3.0 - 2.0 * e);
        w *= s.x * s.y * s.z;
    }
    return w;
}
