// raea's GPU paint (quads/mod.rs): every primitive of a page is one instanced quad, shaded by kind. Coverage is
// analytic (signed distance to a rounded rect, in the primitive's own space), so rotation and scale cost nothing;
// glyphs, gradient ramps and anything the kinds don't cover are rasterized by vello_cpu itself and sampled here, so
// they match the CPU path. Output is premultiplied, blended One / OneMinusSrcAlpha in sRGB-encoded space, as
// vello_cpu blends.

struct Inst {
  xf0: vec4<f32>,    // local -> device: a b c d
  xf1: vec4<f32>,    // e f, device pixels per local unit, 0
  xi0: vec4<f32>,    // device -> local: a b c d
  xi1: vec4<f32>,    // e f
  quad: vec4<f32>,   // the local rect to cover: x0 y0 x1 y1 (device pixels for glyphs and textures)
  shape: vec4<f32>,  // the shape's local rect (a stroke's outer edge)
  radii: vec4<f32>,  // corner radii tl tr br bl
  color: vec4<f32>,  // solid colour, straight alpha; for gradients, images and textures only .a (opacity) counts
  p: vec4<f32>,      // kind, stroke width (0: fill), paint (0 solid, 1 linear, 2 radial, 3 image), image filter (0 bilinear, 1 nearest, 2 bicubic)
  g: vec4<f32>,      // linear p0 p1 | radial centre radius r0 | shadow w h r1
  a: vec4<f32>,      // texel origin x y (glyph, texture) | shadow exponent scale 1/sigma min_edge | ramp row, extend | image extend x y, w h
  m0: vec4<f32>,     // local -> paint (gradient or image space): a b c d
  m1: vec4<f32>,     // e f
  clip: vec4<f32>,   // the device clip rect
  clipr: vec4<f32>,  // its corner radii
  pad: vec4<f32>,
}

@group(0) @binding(0) var<storage, read> insts: array<Inst>;
@group(0) @binding(1) var<uniform> view: vec4<f32>;
@group(0) @binding(2) var lin: sampler;
@group(0) @binding(3) var near: sampler;
@group(0) @binding(4) var atlas: texture_2d<f32>;
@group(0) @binding(5) var tex: texture_2d<f32>;
@group(0) @binding(6) var ramp: texture_2d<f32>;

struct VOut {
  @builtin(position) pos: vec4<f32>,
  @location(0) @interpolate(flat) idx: u32,
}

@vertex
fn vs(@builtin(vertex_index) vi: u32, @builtin(instance_index) ii: u32) -> VOut {
  let I = insts[ii];
  let corner = vec2<f32>(f32(vi & 1u), f32((vi >> 1u) & 1u));
  let l = mix(I.quad.xy, I.quad.zw, corner);
  let d = vec2<f32>(I.xf0.x * l.x + I.xf0.z * l.y + I.xf1.x, I.xf0.y * l.x + I.xf0.w * l.y + I.xf1.y);
  var o: VOut;
  o.pos = vec4<f32>(d.x / view.x * 2.0 - 1.0, 1.0 - d.y / view.y * 2.0, 0.0, 1.0);
  o.idx = ii;
  return o;
}

fn sd_rrect(p: vec2<f32>, r: vec4<f32>, radii: vec4<f32>) -> f32 {
  let c = (r.xy + r.zw) * 0.5;
  let half = (r.zw - r.xy) * 0.5;
  let q = p - c;
  var rr: f32;
  if (q.x > 0.0) { rr = select(radii.y, radii.z, q.y > 0.0); } else { rr = select(radii.x, radii.w, q.y > 0.0); }
  rr = min(rr, min(half.x, half.y));
  let d = abs(q) - half + rr;
  return min(max(d.x, d.y), 0.0) + length(max(d, vec2<f32>(0.0))) - rr;
}

// The rounded rect's signed distance and its outward normal, in its own space.
fn sdg_rrect(p: vec2<f32>, r: vec4<f32>, radii: vec4<f32>) -> vec3<f32> {
  let c = (r.xy + r.zw) * 0.5;
  let half = (r.zw - r.xy) * 0.5;
  let q = p - c;
  var rr: f32;
  if (q.x > 0.0) { rr = select(radii.y, radii.z, q.y > 0.0); } else { rr = select(radii.x, radii.w, q.y > 0.0); }
  rr = min(rr, min(half.x, half.y));
  let d = abs(q) - half + rr;
  let sq = select(vec2<f32>(-1.0), vec2<f32>(1.0), q >= vec2<f32>(0.0));
  var g: vec2<f32>;
  if (d.x > 0.0 && d.y > 0.0) { g = normalize(d) * sq; } else if (d.x > d.y) { g = vec2<f32>(sq.x, 0.0); } else { g = vec2<f32>(0.0, sq.y); }
  let dist = min(max(d.x, d.y), 0.0) + length(max(d, vec2<f32>(0.0))) - rr;
  return vec3<f32>(dist, g);
}

// How much of a device pixel lies inside an edge at signed distance `d` (device pixels) with normal `n`: the exact
// area a box filter sees, as vello's coverage is, rather than a linear ramp (they differ on slanted edges).
fn box_cov(d: f32, n: vec2<f32>) -> f32 {
  let a = max(abs(n.x), abs(n.y));
  let b = min(abs(n.x), abs(n.y));
  let t = -d;
  if (b < 1e-3) { return clamp(0.5 + t, 0.0, 1.0); }
  let h0 = (a - b) * 0.5;
  let h1 = (a + b) * 0.5;
  let at = abs(t);
  if (at <= h0) { return 0.5 + t / a; }
  if (at >= h1) { return select(0.0, 1.0, t > 0.0); }
  let u = h1 - at;
  let area = u * u / (2.0 * a * b);
  return select(area, 1.0 - area, t > 0.0);
}

// A rounded rect's coverage of the pixel at local point `l` (xi: device -> local, so its transpose maps normals).
fn cov_rrect(l: vec2<f32>, r: vec4<f32>, radii: vec4<f32>, xi: vec4<f32>) -> f32 {
  let sg = sdg_rrect(l, r, radii);
  let m = vec2<f32>(xi.x * sg.y + xi.y * sg.z, xi.z * sg.y + xi.w * sg.z);
  let k = max(length(m), 1e-6);
  return box_cov(sg.x / k, m / k);
}

fn erf7(x0: f32) -> f32 {
  let x = clamp(x0, -10.0, 10.0) * 1.1283791671;
  let xx = x * x;
  let y = x + (0.24295 + (0.03395 + 0.0104 * xx) * xx) * (x * xx);
  return y / sqrt(1.0 + y * y);
}

fn pm(c: vec4<f32>) -> vec4<f32> { return vec4<f32>(c.rgb * c.a, c.a); }

// peniko's Extend: 0 pad, 1 repeat, 2 reflect
fn extend(t: f32, mode: f32) -> f32 {
  if (mode > 1.5) { let m = t - 2.0 * floor(t * 0.5); return select(m, 2.0 - m, m > 1.0); }
  if (mode > 0.5) { return fract(t); }
  return clamp(t, 0.0, 1.0);
}

// One texel of an image brush, its extend applied per axis as vello_cpu does (pad clamps, repeat wraps, reflect
// mirrors).
fn ext_i(v: i32, n: i32, mode: f32) -> i32 {
  if (mode > 1.5) {
    let period = 2 * n;
    let r = ((v % period) + period) % period;
    return select(r, period - 1 - r, r >= n);
  }
  if (mode > 0.5) { return ((v % n) + n) % n; }
  return clamp(v, 0, n - 1);
}

fn texel(I: Inst, x: i32, y: i32) -> vec4<f32> {
  let n = vec2<i32>(I.a.zw);
  return textureLoad(tex, vec2<i32>(ext_i(x, n.x, I.a.x), ext_i(y, n.y, I.a.y)), 0);
}

// Mitchell-Netravali (B = C = 1/3), vello_cpu's high-quality resampler
fn mitchell(t: f32) -> vec4<f32> {
  let b = 1.0 / 3.0;
  let c = 1.0 / 3.0;
  let w0 = (1.0 / 6.0) * b + t * (-(3.0 / 6.0) * b - c + t * ((3.0 / 6.0) * b + 2.0 * c + t * (-(1.0 / 6.0) * b - c)));
  let w1 = 1.0 - (2.0 / 6.0) * b + t * (0.0 + t * (-3.0 + (12.0 / 6.0) * b + c + t * (2.0 - (9.0 / 6.0) * b - c)));
  let w2 = (1.0 / 6.0) * b + t * ((3.0 / 6.0) * b + c + t * (3.0 - (15.0 / 6.0) * b - 2.0 * c + t * (-2.0 + (9.0 / 6.0) * b + c)));
  let w3 = t * t * (-c + t * ((1.0 / 6.0) * b + c));
  return vec4<f32>(w0, w1, w2, w3);
}

// An image brush at image-space point `pp` (pixels): nearest (p.w 1), bilinear (0) or bicubic (2), as vello_cpu
// samples for Low, Medium and High.
fn sample_image(I: Inst, pp: vec2<f32>) -> vec4<f32> {
  if (I.p.w > 0.5 && I.p.w < 1.5) {
    let i = vec2<i32>(floor(pp));
    return texel(I, i.x, i.y);
  }
  let q = pp - 0.5;
  let i = vec2<i32>(floor(q));
  let f = q - floor(q);
  if (I.p.w < 0.5) {
    let a = mix(texel(I, i.x, i.y), texel(I, i.x + 1, i.y), f.x);
    let b = mix(texel(I, i.x, i.y + 1), texel(I, i.x + 1, i.y + 1), f.x);
    return mix(a, b, f.y);
  }
  let wx = mitchell(f.x);
  let wy = mitchell(f.y);
  var acc = vec4<f32>(0.0);
  for (var r = 0; r < 4; r++) {
    var row = vec4<f32>(0.0);
    for (var k = 0; k < 4; k++) {
      row += texel(I, i.x - 1 + k, i.y - 1 + r) * wx[k];
    }
    acc += row * wy[r];
  }
  let a = clamp(acc.a, 0.0, 1.0);
  return vec4<f32>(clamp(acc.rgb, vec3<f32>(0.0), vec3<f32>(a)), a);
}

fn paint(I: Inst, l: vec2<f32>) -> vec4<f32> {
  let mode = u32(I.p.z);
  if (mode == 0u) { return pm(I.color); }
  let pp = vec2<f32>(I.m0.x * l.x + I.m0.z * l.y + I.m1.x, I.m0.y * l.x + I.m0.w * l.y + I.m1.y);
  if (mode == 3u) {
    return sample_image(I, pp) * I.color.a;
  }
  var t: f32;
  if (mode == 1u) {
    let d = I.g.zw - I.g.xy;
    t = dot(pp - I.g.xy, d) / dot(d, d);
  } else {
    t = (length(pp - I.g.xy) - I.g.w) / (I.g.z - I.g.w);
  }
  let u = (extend(t, I.a.y) * 255.0 + 0.5) / 256.0;
  return textureSampleLevel(ramp, lin, vec2<f32>(u, I.a.x), 0.0) * I.color.a;
}

// On the u8 grid, as vello_cpu's lowp pipeline holds every colour: the premultiplied source and its coverage are
// each rounded to bytes and multiplied with exact rounding (its div_255), so the blend unit's float over of an
// on-grid source rounds exactly as vello's does (off the grid, a 55% scrim left a fifth of channels a level apart).
fn q(v: vec4<f32>) -> vec4<f32> { return round(v * 255.0) / 255.0; }
fn qs(v: f32) -> f32 { return round(v * 255.0) / 255.0; }
fn over_cov(c: vec4<f32>, cov: f32) -> vec4<f32> { return q(q(c) * qs(cov)); }

@fragment
fn fs(v: VOut) -> @location(0) vec4<f32> {
  let I = insts[v.idx];
  let p = v.pos.xy;
  let clipped = clamp(0.5 - sd_rrect(p, I.clip, I.clipr), 0.0, 1.0);
  let kind = u32(I.p.x);
  if (kind == 2u) {
    let t = vec2<i32>(I.a.xy + floor(p - I.quad.xy));
    return over_cov(pm(I.color), textureLoad(atlas, t, 0).r * clipped);
  }
  if (kind == 3u) {
    let t = vec2<i32>(I.a.xy + floor(p - I.quad.xy));
    return over_cov(textureLoad(tex, t, 0), qs(I.color.a) * clipped);
  }
  let l = vec2<f32>(I.xi0.x * p.x + I.xi0.z * p.y + I.xi1.x, I.xi0.y * p.x + I.xi0.w * p.y + I.xi1.y);
  if (kind == 1u) {
    let lp = l - I.shape.xy;
    let wh = I.shape.zw - I.shape.xy;
    let e = I.a.x;
    let r1 = I.g.z;
    let y = lp.y - 0.5 * wh.y;
    let y0 = r1 - (I.g.y * 0.5 - abs(y));
    let y1 = max(y0, 0.0);
    let x = lp.x - 0.5 * wh.x;
    let x0 = r1 - (I.g.x * 0.5 - abs(x));
    let x1 = max(x0, 0.0);
    let px = select(0.0, pow(x1, e), x1 > 0.0);
    let py = select(0.0, pow(y1, e), y1 > 0.0);
    let sum = px + py;
    let dpos = select(0.0, pow(sum, 1.0 / e), sum > 0.0);
    let dneg = min(max(x0, y0), 0.0);
    let d = dpos + dneg - r1;
    let z = I.a.y * (erf7(I.a.z * (I.a.w + d)) - erf7(I.a.z * d));
    // vello paints the blur as a colour (premultiplied f32 times the blur), then covers it by the clip
    return over_cov(pm(I.color) * z, clipped);
  }
  var cov = cov_rrect(l, I.shape, I.radii, I.xi0);
  let w = I.p.y;
  if (w > 0.0) {
    let inner = I.shape + vec4<f32>(w, w, -w, -w);
    if (inner.z > inner.x && inner.w > inner.y) {
      cov = cov * (1.0 - cov_rrect(l, inner, max(I.radii - vec4<f32>(w), vec4<f32>(0.0)), I.xi0));
    }
  }
  return over_cov(paint(I, l), cov * clipped);
}
