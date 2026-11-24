// raea's diffuse BSDF (docs/bake.feature BK3), a module any ray-traced pass may include as it is (the light bake's
// trace today; systems/rt and LT5's path tracer can take the same file): Lambert, sampled cosine-weighted, so a
// path's weight is the albedo and the estimate of the light reaching a point is the mean of what its rays see.
// Written here until tools/cycles-port's diffuse and energy-conserving Oren-Nayar are on main.

// A direction about the unit normal n, cosine-weighted, from u in the unit square.
fn raea_diffuse_sample(n: vec3<f32>, u: vec2<f32>) -> vec3<f32> {
    let a = select(vec3<f32>(0.0, 1.0, 0.0), vec3<f32>(1.0, 0.0, 0.0), abs(n.x) < 0.9);
    let t = normalize(cross(n, a));
    let b = cross(n, t);
    let r = sqrt(u.x);
    let phi = 6.283185307 * u.y;
    return normalize(t * (r * cos(phi)) + b * (r * sin(phi)) + n * sqrt(max(0.0, 1.0 - u.x)));
}

// A hash of one u32 (PCG's output step): the paths' random numbers, fixed by texel, pass and sample.
fn raea_hash(v: u32) -> u32 {
    let s = v * 747796405u + 2891336453u;
    let w = ((s >> ((s >> 28u) + 4u)) ^ s) * 277803737u;
    return (w >> 22u) ^ w;
}

fn raea_rand(state: ptr<function, u32>) -> f32 {
    *state = raea_hash(*state);
    return f32(*state >> 8u) / 16777216.0;
}
