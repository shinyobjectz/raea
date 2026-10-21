// Gaussian splats (splat.rs): each splat a 3D gaussian (a centre, a covariance from its scale and rotation, an
// opacity and a view-dependent colour in spherical harmonics up to degree 3) drawn as a screen-space ellipse: the
// covariance projected through the camera (EWA: J W Σ Wᵀ Jᵀ, with a 0.3 px² low-pass), a quad over the ellipse's
// extent, each pixel exp(-½ d²) of its opacity, blended back to front in the order splat.rs sorted them for this
// frame's camera (premultiplied "over").
//
// The quad's vertex `position` is (corner x, corner y, slot): the slot is the splat's place in this frame's order,
// order[slot] the splat. Bevy packs meshes into shared buffers, so @builtin(vertex_index) isn't the mesh's own.
//
// Every effect (morph, twist, wave, explode, disperse/assemble, reveal, pulse, grade) is a closed-form function of
// the splat's index, its seed and the row's values: no history, no clock, so a frame drawn alone equals the same
// frame in play. splat.rs mirrors the moving ones (anim_center) to sort by where the splats are drawn.

#import bevy_pbr::mesh_view_bindings::view
#import bevy_pbr::mesh_functions

struct Gauss {
    // centre (the splat's aligned space), opacity 0..1
    p: vec4<f32>,
    // scale (linear, the gaussian's standard deviations), unused
    s: vec4<f32>,
    // rotation (x y z w)
    q: vec4<f32>,
    // degree-0 colour (sRGB, 0.5 + C0 dc), unused
    c: vec4<f32>,
};

struct SplatParams {
    // aligned space -> the file's (for the spherical harmonics' view direction)
    sh_rot: mat3x3<f32>,
    center: vec3<f32>,
    radius: f32,
    count: u32,
    sh_deg: u32,
    sh_words: u32,
    morph: f32,
    seed: f32,
    twist: f32,
    wave_amp: f32,
    wave_len: f32,
    wave_phase: f32,
    explode: f32,
    disperse: f32,
    disperse_mode: u32,
    wind: vec3<f32>,
    curl: f32,
    reveal: f32,
    reveal_mode: u32,
    reveal_soft: f32,
    reveal_glow: f32,
    reveal_origin: vec3<f32>,
    crop: f32,
    reveal_axis: vec3<f32>,
    bands: f32,
    pulse: f32,
    pulse_scale: f32,
    pulse_glow: f32,
    splat_scale: f32,
    opacity: f32,
    brightness: f32,
    saturation: f32,
    contrast: f32,
    tint: vec4<f32>,
    glow_color: vec4<f32>,
    // painted (0..1): flat bands, splats grown into strokes, their edges hard
    paint: f32,
};

@group(#{MATERIAL_BIND_GROUP}) @binding(0) var<uniform> p: SplatParams;
@group(#{MATERIAL_BIND_GROUP}) @binding(1) var<storage, read> splats: array<Gauss>;
@group(#{MATERIAL_BIND_GROUP}) @binding(2) var<storage, read> sh: array<u32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(3) var<storage, read> target_splats: array<Gauss>;
@group(#{MATERIAL_BIND_GROUP}) @binding(4) var<storage, read> order: array<u32>;

struct VIn {
    @builtin(instance_index) instance_index: u32,
    @location(0) position: vec3<f32>,
};

struct VOut {
    @builtin(position) clip: vec4<f32>,
    // the pixel's offset from the centre in standard deviations along the ellipse's axes
    @location(0) d: vec2<f32>,
    @location(1) color: vec4<f32>,
};

// ---- per-splat randoms: pcg (the same integer steps as splat.rs's `hash`)

fn pcg(v: u32) -> u32 {
    let s = v * 747796405u + 2891336453u;
    let w = ((s >> ((s >> 28u) + 4u)) ^ s) * 277803737u;
    return (w >> 22u) ^ w;
}

fn hash(i: u32, k: u32) -> f32 {
    return f32(pcg(i ^ pcg(k + bitcast<u32>(p.seed))) >> 8u) / 16777216.0;
}

// a divergence-free wind: the curl of a vector potential of sines (two octaves), as splat.rs's `curl`
fn potential(x: vec3<f32>) -> vec3<f32> {
    return vec3<f32>(
        sin(x.y * 1.7 + 1.3) + cos(x.z * 2.3 + 0.4) + 0.5 * sin(x.y * 3.9 + x.z * 1.1 + 2.0),
        sin(x.z * 1.9 + 2.9) + cos(x.x * 2.1 + 1.7) + 0.5 * sin(x.z * 4.1 + x.x * 1.3 + 0.7),
        sin(x.x * 1.5 + 0.2) + cos(x.y * 2.7 + 2.2) + 0.5 * sin(x.x * 3.7 + x.y * 1.2 + 1.5),
    );
}

fn curl(x: vec3<f32>) -> vec3<f32> {
    let e = 0.05;
    let dx = vec3<f32>(e, 0.0, 0.0);
    let dy = vec3<f32>(0.0, e, 0.0);
    let dz = vec3<f32>(0.0, 0.0, e);
    let px0 = potential(x - dx); let px1 = potential(x + dx);
    let py0 = potential(x - dy); let py1 = potential(x + dy);
    let pz0 = potential(x - dz); let pz1 = potential(x + dz);
    return vec3<f32>(
        (py1.z - py0.z) - (pz1.y - pz0.y),
        (pz1.x - pz0.x) - (px1.z - px0.z),
        (px1.y - px0.y) - (py1.x - py0.x),
    ) / (2.0 * e);
}

fn quat_mul(a: vec4<f32>, b: vec4<f32>) -> vec4<f32> {
    return vec4<f32>(
        a.w * b.xyz + b.w * a.xyz + cross(a.xyz, b.xyz),
        a.w * b.w - dot(a.xyz, b.xyz),
    );
}

fn quat_mat(q: vec4<f32>) -> mat3x3<f32> {
    let x = q.x; let y = q.y; let z = q.z; let w = q.w;
    return mat3x3<f32>(
        vec3<f32>(1.0 - 2.0 * (y * y + z * z), 2.0 * (x * y + w * z), 2.0 * (x * z - w * y)),
        vec3<f32>(2.0 * (x * y - w * z), 1.0 - 2.0 * (x * x + z * z), 2.0 * (y * z + w * x)),
        vec3<f32>(2.0 * (x * z + w * y), 2.0 * (y * z - w * x), 1.0 - 2.0 * (x * x + y * y)),
    );
}

fn smooth01(x: f32) -> f32 {
    let t = clamp(x, 0.0, 1.0);
    return t * t * (3.0 - 2.0 * t);
}

// ---- the effects

struct Anim {
    pos: vec3<f32>,
    scale: vec3<f32>,
    q: vec4<f32>,
    color: vec3<f32>,
    alpha: f32,
    glow: f32,
};

// The splat where the row's effects put it (splat.rs's anim_center follows the same steps for the centre).
fn animate(i: u32) -> Anim {
    let g = splats[i];
    var a: Anim;
    a.pos = g.p.xyz;
    a.scale = g.s.xyz;
    a.q = g.q;
    a.color = g.c.rgb;
    a.alpha = g.p.w;
    a.glow = 0.0;
    let r = max(p.radius, 1e-4);
    // morph: towards the matched splat of the target, lifted off the straight path mid-way
    if p.morph > 0.0 {
        let t = target_splats[i];
        let m = smooth01(p.morph);
        let lift = sin(3.14159265 * m) * 0.35 * r;
        a.pos = mix(a.pos, t.p.xyz, m) + lift * (vec3<f32>(hash(i, 11u), hash(i, 12u), hash(i, 13u)) - 0.5) * 2.0;
        a.scale = mix(a.scale, t.s.xyz, m) * (1.0 - 0.45 * sin(3.14159265 * m));
        var tq = t.q;
        if dot(a.q, tq) < 0.0 { tq = -tq; }
        a.q = normalize(mix(a.q, tq, m));
        a.color = mix(a.color, t.c.rgb, m);
        a.alpha = mix(a.alpha, t.p.w, m);
    }
    // twist: turned about the vertical through the centre, more the higher (radians a metre)
    if p.twist != 0.0 {
        let ang = p.twist * (a.pos.y - p.center.y);
        let c = cos(ang); let s = sin(ang);
        let d = a.pos - p.center;
        a.pos = p.center + vec3<f32>(c * d.x + s * d.z, d.y, -s * d.x + c * d.z);
        a.q = quat_mul(vec4<f32>(0.0, sin(ang * 0.5), 0.0, cos(ang * 0.5)), a.q);
    }
    // wave: a ripple running out from the centre across the ground plane
    if p.wave_amp != 0.0 {
        let d = length((a.pos - p.center).xz);
        a.pos.y += p.wave_amp * sin(6.2831853 * d / max(p.wave_len, 1e-3) - p.wave_phase);
    }
    // explode: thrown out from the centre, each its own speed, falling, fading at the end
    if p.explode > 0.0 {
        let e = p.explode;
        let jit = vec3<f32>(hash(i, 21u), hash(i, 22u), hash(i, 23u)) - 0.5;
        let dir = normalize(a.pos - p.center + jit * 0.6 * r + vec3<f32>(0.0, 1e-4, 0.0));
        a.pos += dir * e * r * 2.0 * (0.4 + hash(i, 24u));
        a.pos.y -= 1.2 * r * e * e;
        a.scale *= 1.0 - 0.5 * clamp(e, 0.0, 1.0);
        a.alpha *= 1.0 - smooth01((e - 0.7) / 0.3);
    }
    // disperse (0 whole, 1 gone): wind, blown off on a curl-noise wind from the upwind side on; dust, scattered into
    // a cloud round the scene (assemble is this run backwards)
    if p.disperse > 0.0 {
        let o = disperse_order(i, a.pos);
        if p.disperse_mode == 0u {
            let local = clamp((p.disperse - 0.55 * o) / 0.45, 0.0, 1.0);
            let w = normalize(p.wind + vec3<f32>(0.0, 0.0, 1e-6));
            a.pos += w * local * local * r * 3.0 + curl(a.pos * (1.4 / r) + vec3<f32>(p.seed)) * (p.curl * r * 0.25 * local);
            a.pos.y += local * local * r * 0.4;
            a.scale *= 1.0 - 0.75 * local;
            a.alpha *= 1.0 - smooth01((local - 0.7) / 0.3);
        } else {
            let local = smooth01((p.disperse - 0.5 * o) / 0.5);
            let u = hash(i, 31u);
            let v = hash(i, 32u);
            let rr = pow(hash(i, 33u), 0.333) * r * 1.6;
            let th = 6.2831853 * u;
            let ph = acos(2.0 * v - 1.0);
            let q = p.center + rr * vec3<f32>(sin(ph) * cos(th), cos(ph), sin(ph) * sin(th));
            let swirl = curl(q * (1.4 / r) + vec3<f32>(p.seed)) * (p.curl * r * 0.12);
            a.pos = mix(a.pos, q + swirl, local);
            a.scale *= mix(1.0, 0.3, local);
            a.alpha *= mix(1.0, 0.7, local);
        }
    }
    // reveal (0 nothing, 1 all): each splat scales in as the front (a sphere from the origin, or a plane along the
    // axis) passes it, the front glowing
    if p.reveal_mode != 0u {
        var t: f32;
        if p.reveal_mode == 1u {
            t = length(a.pos - p.reveal_origin) / (2.0 * r);
        } else {
            t = (dot(a.pos - p.center, normalize(p.reveal_axis)) / r + 1.0) * 0.5;
        }
        t += (hash(i, 41u) - 0.5) * 0.04;
        let soft = max(p.reveal_soft, 1e-3);
        let local = clamp((p.reveal * (1.0 + soft) - t) / soft, 0.0, 1.0);
        a.scale *= smooth01(local);
        a.alpha *= smooth01(local * 3.0);
        a.glow += p.reveal_glow * 4.0 * local * (1.0 - local);
    }
    // pulse: bigger and brighter on a value the composition passes (a beat)
    a.scale *= (1.0 + p.pulse_scale * p.pulse) * p.splat_scale * (1.0 + 0.7 * p.paint);
    a.glow += p.pulse_glow * p.pulse;
    // crop: nothing past this distance from the centre
    if p.crop > 0.0 && length(g.p.xyz - p.center) > p.crop {
        a.alpha = 0.0;
    }
    return a;
}

// a splat's turn to go: along the wind (upwind first) or out from the centre, with some grain
fn disperse_order(i: u32, home: vec3<f32>) -> f32 {
    let r = max(p.radius, 1e-4);
    var o: f32;
    if p.disperse_mode == 0u {
        o = (dot(home - p.center, normalize(p.wind + vec3<f32>(0.0, 0.0, 1e-6))) / r + 1.0) * 0.5;
    } else {
        o = length(home - p.center) / (1.5 * r);
    }
    return clamp(o * 0.8 + hash(i, 51u) * 0.2, 0.0, 1.0);
}

// ---- spherical harmonics (the 3DGS reference's constants)

const SH_C1: f32 = 0.4886025119029199;
const SH_C2 = array<f32, 5>(1.0925484305920792, -1.0925484305920792, 0.31539156525252005, -1.0925484305920792, 0.5462742152960396);
const SH_C3 = array<f32, 7>(-0.5900435899266435, 2.890611442640554, -0.4570457994644658, 0.3731763325901154, -0.4570457994644658, 1.445305721320277, -0.5900435899266435);

fn coef(i: u32, k: u32) -> vec3<f32> {
    let base = i * p.sh_words;
    let h = k * 3u;
    var out: vec3<f32>;
    for (var c = 0u; c < 3u; c++) {
        let hh = h + c;
        let w = unpack2x16float(sh[base + hh / 2u]);
        out[c] = select(w.x, w.y, (hh & 1u) == 1u);
    }
    return out;
}

fn sh_color(i: u32, dir: vec3<f32>, dc: vec3<f32>) -> vec3<f32> {
    var c = dc;
    if p.sh_deg == 0u {
        return c;
    }
    let x = dir.x; let y = dir.y; let z = dir.z;
    c += SH_C1 * (-y * coef(i, 0u) + z * coef(i, 1u) - x * coef(i, 2u));
    if p.sh_deg == 1u {
        return c;
    }
    let xx = x * x; let yy = y * y; let zz = z * z;
    let xy = x * y; let yz = y * z; let xz = x * z;
    c += SH_C2[0] * xy * coef(i, 3u) + SH_C2[1] * yz * coef(i, 4u) + SH_C2[2] * (2.0 * zz - xx - yy) * coef(i, 5u)
        + SH_C2[3] * xz * coef(i, 6u) + SH_C2[4] * (xx - yy) * coef(i, 7u);
    if p.sh_deg == 2u {
        return c;
    }
    c += SH_C3[0] * y * (3.0 * xx - yy) * coef(i, 8u) + SH_C3[1] * xy * z * coef(i, 9u)
        + SH_C3[2] * y * (4.0 * zz - xx - yy) * coef(i, 10u) + SH_C3[3] * z * (2.0 * zz - 3.0 * xx - 3.0 * yy) * coef(i, 11u)
        + SH_C3[4] * x * (4.0 * zz - xx - yy) * coef(i, 12u) + SH_C3[5] * z * (xx - yy) * coef(i, 13u)
        + SH_C3[6] * x * (xx - 3.0 * yy) * coef(i, 14u);
    return c;
}

fn srgb_to_linear(c: vec3<f32>) -> vec3<f32> {
    let lo = c / 12.92;
    let hi = pow((c + 0.055) / 1.055, vec3<f32>(2.4));
    return select(hi, lo, c <= vec3<f32>(0.04045));
}

fn grade(c0: vec3<f32>, glow: f32) -> vec3<f32> {
    var c = c0 * p.tint.rgb * p.brightness;
    let l = dot(c, vec3<f32>(0.2126, 0.7152, 0.0722));
    c = mix(vec3<f32>(l), c, p.saturation);
    c = (c - 0.5) * p.contrast + 0.5;
    // the paint look: the colour's light in flat bands, its hue kept (painted: four bands unless given, a little
    // more colour)
    var bands = p.bands;
    if p.paint > 0.0 {
        if bands <= 0.0 { bands = 4.0; }
        let l0 = dot(c, vec3<f32>(0.2126, 0.7152, 0.0722));
        c = mix(c, mix(vec3<f32>(l0), c, 1.35), p.paint);
    }
    if bands > 0.0 {
        let l2 = max(dot(c, vec3<f32>(0.2126, 0.7152, 0.0722)), 1e-4);
        let q = (floor(l2 * bands) + 0.5) / bands;
        // the hue kept where there's light to carry it; the darkest go to their band's grey (a dark blue's
        // luminance is tiny, and scaling it up to the band blew it out)
        c = mix(vec3<f32>(q), c * min(q / l2, 3.0), smooth01(l2 * 6.0));
    }
    c += p.glow_color.rgb * glow;
    return max(c, vec3<f32>(0.0));
}

fn offscreen() -> VOut {
    var o: VOut;
    o.clip = vec4<f32>(0.0, 0.0, 2.0, 1.0);
    o.d = vec2<f32>(0.0);
    o.color = vec4<f32>(0.0);
    return o;
}

@vertex
fn vertex(v: VIn) -> VOut {
    let slot = u32(v.position.z + 0.5);
    if slot >= p.count {
        return offscreen();
    }
    let i = order[slot];
    let a = animate(i);
    let alpha = clamp(a.alpha * p.opacity, 0.0, 1.0);
    if alpha < 1.0 / 255.0 || max(a.scale.x, max(a.scale.y, a.scale.z)) <= 0.0 {
        return offscreen();
    }
    let world_from_local = mesh_functions::get_world_from_local(v.instance_index);
    let m3 = mat3x3<f32>(world_from_local[0].xyz, world_from_local[1].xyz, world_from_local[2].xyz);
    let world = world_from_local * vec4<f32>(a.pos, 1.0);
    let vp = view.view_from_world * world;
    let depth = -vp.z;
    if depth < 0.02 {
        return offscreen();
    }
    // the 3D covariance in view space: V (M R S)(M R S)ᵀ Vᵀ
    let rs = quat_mat(normalize(a.q)) * mat3x3<f32>(vec3<f32>(a.scale.x, 0.0, 0.0), vec3<f32>(0.0, a.scale.y, 0.0), vec3<f32>(0.0, 0.0, a.scale.z));
    let w3 = mat3x3<f32>(view.view_from_world[0].xyz, view.view_from_world[1].xyz, view.view_from_world[2].xyz);
    let t = w3 * m3 * rs;
    let cov = t * transpose(t);
    // the camera's focal lengths in pixels
    let fx = view.clip_from_view[0][0] * view.viewport.z * 0.5;
    let fy = view.clip_from_view[1][1] * view.viewport.w * 0.5;
    // clamp the centre's slope to a little past the frame (the 3DGS reference's), so splats at the edge don't smear
    let lim_x = 1.3 * view.viewport.z * 0.5 / fx;
    let lim_y = 1.3 * view.viewport.w * 0.5 / fy;
    let tx = clamp(vp.x / depth, -lim_x, lim_x) * depth;
    let ty = clamp(vp.y / depth, -lim_y, lim_y) * depth;
    // the projection's Jacobian (view space looks down -z; screen y up), rows (fx/d, 0, fx x/d²), (0, fy/d, fy y/d²)
    let j0 = vec3<f32>(fx / depth, 0.0, fx * tx / (depth * depth));
    let j1 = vec3<f32>(0.0, fy / depth, fy * ty / (depth * depth));
    let cj0 = cov * j0;
    let cj1 = cov * j1;
    var ca = dot(j0, cj0) + 0.3;
    let cb = dot(j0, cj1);
    var cc = dot(j1, cj1) + 0.3;
    // the ellipse's axes
    let mid = 0.5 * (ca + cc);
    let rad = sqrt(max(0.25 * (ca - cc) * (ca - cc) + cb * cb, 0.0));
    let l1 = mid + rad;
    let l2 = max(mid - rad, 0.1);
    var e1 = vec2<f32>(1.0, 0.0);
    if abs(cb) > 1e-9 {
        e1 = normalize(vec2<f32>(cb, l1 - ca));
    } else if cc > ca {
        e1 = vec2<f32>(0.0, 1.0);
    }
    let e2 = vec2<f32>(-e1.y, e1.x);
    // out to where the gaussian falls below 1/255 of this splat's opacity (at most 3 sigma), and at most 1024 px
    let k = min(sqrt(max(2.0 * log(255.0 * alpha), 0.0)), 3.0);
    let s1 = min(sqrt(l1), 1024.0 / max(k, 1e-3));
    let s2 = min(sqrt(l2), 1024.0 / max(k, 1e-3));
    let corner = v.position.xy * k;
    let off_px = corner.x * s1 * e1 + corner.y * s2 * e2;
    var clip = view.clip_from_world * world;
    // the corner's depth: where the gaussian is densest along the corner's own ray (t = μᵀΣ⁻¹d / dᵀΣ⁻¹d, Σ⁻¹ = A⁻ᵀA⁻¹),
    // so a flat splat (a trained surface) has its plane's depth at every pixel, not its centre's, and meets a mesh
    // or a model where the surfaces cross
    let pc = vec2<f32>(fx * vp.x / depth, fy * vp.y / depth) + off_px;
    let ray = vec3<f32>(pc.x / fx, pc.y / fy, -1.0);
    let ai = inverse3(t);
    let am = ai * vp.xyz;
    let ad = ai * ray;
    let tt = dot(am, ad) / max(dot(ad, ad), 1e-20);
    var z = clip.z / clip.w;
    if tt > 0.02 && tt < depth * 4.0 {
        let pclip = view.clip_from_view * vec4<f32>(ray * tt, 1.0);
        z = pclip.z / pclip.w;
    }
    clip = vec4<f32>(clip.xy + off_px * 2.0 / view.viewport.zw * clip.w, z * clip.w, clip.w);
    // the colour seen from the camera: the view direction in the file's own space
    let cam_local = (inverse_affine(world_from_local) * vec4<f32>(view.world_position, 1.0)).xyz;
    let dir = normalize(p.sh_rot * (a.pos - cam_local));
    var col = a.color;
    if p.morph <= 0.0 {
        col = sh_color(i, dir, a.color);
    } else {
        // the morph's colours are both degree 0, the view-dependent part fading out
        col = mix(sh_color(i, dir, splats[i].c.rgb), a.color, smooth01(p.morph));
    }
    col = grade(max(col, vec3<f32>(0.0)), a.glow);
    var o: VOut;
    o.clip = clip;
    o.d = corner;
    o.color = vec4<f32>(srgb_to_linear(col), alpha);
    return o;
}

fn inverse3(a: mat3x3<f32>) -> mat3x3<f32> {
    let det = dot(a[0], cross(a[1], a[2]));
    return transpose(mat3x3<f32>(cross(a[1], a[2]), cross(a[2], a[0]), cross(a[0], a[1]))) * (1.0 / det);
}

fn inverse_affine(m: mat4x4<f32>) -> mat4x4<f32> {
    let a = mat3x3<f32>(m[0].xyz, m[1].xyz, m[2].xyz);
    let det = dot(a[0], cross(a[1], a[2]));
    let inv = transpose(mat3x3<f32>(cross(a[1], a[2]), cross(a[2], a[0]), cross(a[0], a[1]))) * (1.0 / det);
    let t = -(inv * m[3].xyz);
    return mat4x4<f32>(vec4<f32>(inv[0], 0.0), vec4<f32>(inv[1], 0.0), vec4<f32>(inv[2], 0.0), vec4<f32>(t, 1.0));
}

@fragment
fn fragment(i: VOut) -> @location(0) vec4<f32> {
    let power = -0.5 * dot(i.d, i.d);
    var a = min(0.99, i.color.a * exp(power));
    // painted: a dab with a hard edge where the gaussian was half its peak
    if p.paint > 0.0 {
        // (the faint, wide splats stay soft: hard, they'd show as discs)
        a = mix(a, select(0.0, min(0.99, i.color.a * 1.2), power > -0.69), p.paint * smooth01((i.color.a - 0.25) / 0.4));
    }
    if a < 1.0 / 255.0 {
        discard;
    }
#ifdef SPLAT_MASK
    // a looked model's cells erased where the splat is in front of it (blend: dst * (1 - a))
    return vec4<f32>(0.0, 0.0, 0.0, a);
#else
    // premultiplied
    return vec4<f32>(i.color.rgb * a, a);
#endif
}
