// raea's lens (Fisheye and LensDistortion in @raea/motion/3d; engine/rdb/src/engine/lens.rs): a fullscreen pass after tone
// mapping that remaps the camera's rectilinear picture radially about its centre (a lens's vignette and colour fringes
// are <Grade>'s). Radii are in half-heights (the frame's centre to its top edge is 1); `edge` is the
// half-diagonal, so the corners sit at radius `edge`.
//
//   fisheye (equidistant, mode 1; stereographic, mode 2): the angle off the axis grows evenly with the radius
//     (equidistant) or as 2 atan (stereographic), scaled so the corners still show the picture's corners; the source
//     radius is tan(angle) / tan(vertical fov / 2), mixed with the plain radius by `strength`
//   Brown-Conrady (mode 3): r_src = r (1 + k1 r^2 + k2 r^4), r a share of the half-diagonal; k1 > 0 barrel, < 0
//     pincushion
// Both are divided by `zoom` (1 by default for a fisheye, enough for a barrel's corners to stay inside the picture).
// A sample outside the picture takes the nearest edge pixel (clamped), or black when `black` is set.

#import bevy_core_pipeline::fullscreen_vertex_shader::FullscreenVertexOutput

@group(0) @binding(0) var screen: texture_2d<f32>;
@group(0) @binding(1) var samp: sampler;

struct Lens {
    // mode, strength, k1, k2
    a: vec4<f32>,
    // zoom, tan(vertical fov / 2) of the render, aspect (w / h), black outside (1) or clamp (0)
    b: vec4<f32>,
}
@group(0) @binding(2) var<uniform> lens: Lens;

const HALF_PI: f32 = 1.5707963;

// the source radius for an output radius (half-heights)
fn source_radius(rho: f32, edge: f32) -> f32 {
    let mode = u32(lens.a.x + 0.5);
    let strength = lens.a.y;
    var s = rho;
    if mode == 1u || mode == 2u {
        let tanv = lens.b.y;
        let phi_max = atan(edge * tanv);
        var phi: f32;
        if mode == 1u {
            phi = rho / edge * phi_max;
        } else {
            phi = 2.0 * atan(rho / edge * tan(phi_max * 0.5));
        }
        let rs = tan(min(phi, HALF_PI - 0.01)) / tanv;
        s = mix(rho, rs, strength);
    } else if mode == 3u {
        let r = rho / edge;
        let r2 = r * r;
        s = rho * (1.0 + lens.a.z * r2 + lens.a.w * r2 * r2);
    }
    return s / lens.b.x;
}

fn sample_at(uv: vec2<f32>) -> vec4<f32> {
    if lens.b.w > 0.5 && (any(uv < vec2<f32>(0.0)) || any(uv > vec2<f32>(1.0))) {
        return vec4<f32>(0.0, 0.0, 0.0, 1.0);
    }
    return textureSampleLevel(screen, samp, clamp(uv, vec2<f32>(0.0), vec2<f32>(1.0)), 0.0);
}

@fragment
fn fragment(in: FullscreenVertexOutput) -> @location(0) vec4<f32> {
    let aspect = lens.b.z;
    let to_p = vec2<f32>(2.0 * aspect, 2.0);
    let p = (in.uv - 0.5) * to_p;
    let edge = sqrt(aspect * aspect + 1.0);
    let rho = length(p);
    var scale = 1.0 / lens.b.x;
    if rho > 1e-5 {
        scale = source_radius(rho, edge) / rho;
    } else {
        // the centre: the mapping's slope at 0
        scale = source_radius(1e-3, edge) / 1e-3;
    }
    let q = p * scale;
    return sample_at(q / to_p + 0.5);
}
