
// raea's GPU paint (quads/mod.rs): every primitive of a page is an instance; a compute pass composites them per
// pixel, each 16x16 tile walking the instances binned to it, in paint order. Coverage is analytic (exact box-filter
// area of a rounded rect, in the primitive's own space), glyphs, gradient ramps and fallbacks come from vello_cpu
// rasters, and compositing is vello_cpu's own u8 arithmetic (raea's fork): Skia's truncating blend for solid
// interiors and popped layers, exact rounding for paints, the masked formula on its 4x4 strip tiles. So a page
// comes out as vello_cpu would paint it, to the level, where a fixed-function float blend is a level off under any
// translucent fill.

struct Inst {
  xf0: vec4<f32>,    // local -> device: a b c d
  xf1: vec4<f32>,    // e f, device pixels per local unit, 0
  xi0: vec4<f32>,    // device -> local: a b c d
  xi1: vec4<f32>,    // e f
  quad: vec4<f32>,   // the local rect it covers: x0 y0 x1 y1 (device pixels for glyphs and textures)
  shape: vec4<f32>,  // the shape's local rect (a stroke's outer edge)
  radii: vec4<f32>,  // corner radii tl tr br bl
  color: vec4<f32>,  // solid colour, straight alpha; for gradients, images and textures only .a (opacity) counts
  p: vec4<f32>,      // kind (0 shape, 1 shadow, 2 glyph, 3 texture), stroke width, paint (0 solid 1 linear 2 radial 3 image 4 video), image filter (0 bilinear 1 nearest 2 bicubic)
  g: vec4<f32>,      // linear p0 p1 | radial centre radius r0 | shadow w h r1
  a: vec4<f32>,      // texel origin x y (glyph, texture) | shadow exponent scale 1/sigma min_edge | ramp row, extend | image extend x y, w h
  m0: vec4<f32>,     // local -> paint (gradient or image space): a b c d
  m1: vec4<f32>,     // e f
  clip: vec4<f32>,   // the device clip rect
  clipr: vec4<f32>,  // its corner radii
  pad: vec4<f32>,    // texture slot, texture mode (0 or 1 a paint, 2 a popped layer, 3 solid coverage, 4 solid coverage as a Skia mask), image alpha straight (1)
  inner: vec4<f32>,  // a hole's local rect (a stroke's inner edge, a border's padding box); none when empty
  innerr: vec4<f32>, // its corner radii
  hole: vec4<f32>,   // the device rect cut out of the clip (a box shadow's outside); none when empty
  holer: vec4<f32>,  // its corner radii
}

struct Pass {
  size: vec2<u32>,
  tiles_x: u32,
  offsets: u32,      // where this pass's tile offsets start in `bins` (tiles + 1 of them)
  clear: vec4<f32>,
  origin: vec2<u32>, // the first tile dispatched (a group's pass covers only its picture's tiles, its target from there)
}

@group(0) @binding(0) var<storage, read> insts: array<Inst>;
@group(0) @binding(1) var<storage, read> bins: array<u32>;
@group(0) @binding(2) var<uniform> pass_: Pass;
@group(0) @binding(3) var lin: sampler;
@group(0) @binding(4) var atlas: texture_2d<f32>;
@group(0) @binding(5) var ramp: texture_2d<f32>;
@group(0) @binding(6) var out: texture_storage_2d<rgba8unorm, write>;
// the images, rasters and group pictures draws sample, in a group of their own (Metal keeps binding arrays apart)
@group(1) @binding(0) var texs: binding_array<texture_2d<f32>>;

fn tex_of(I: Inst) -> u32 { return u32(I.pad.x); }

// Built with Skia (the engine's default), vello_cpu draws text as Skia glyph masks: an image paint of the colour
// premultiplied per pixel, which composites with rounding, where plain vello fills glyph outlines as solid paths.
override SKIA_TEXT: bool = false;

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
  let t = textureLoad(texs[tex_of(I)], vec2<i32>(ext_i(x, n.x, I.a.x), ext_i(y, n.y, I.a.y)), 0);
  if (I.pad.z < 0.5) { return t; }
  // straight alpha, premultiplied on the u8 grid as vello's pixmap premultiplies (exact rounding)
  let c = vec4<u32>(round(t * 255.0));
  return vec4<f32>(f32(div255(c.r * c.a)), f32(div255(c.g * c.a)), f32(div255(c.b * c.a)), f32(c.a)) / 255.0;
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

// One plane of a video frame (texture slot `t`, `n` texels) at point `q` (texels), filtered: nearest (f 1), bilinear
// (0) or bicubic (2), the edges repeated.
fn plane(t: u32, q0: vec2<f32>, n: vec2<i32>, f: f32) -> vec4<f32> {
  let hi = n - vec2<i32>(1);
  if (f > 0.5 && f < 1.5) {
    return textureLoad(texs[t], clamp(vec2<i32>(floor(q0)), vec2<i32>(0), hi), 0);
  }
  let q = q0 - 0.5;
  let i = vec2<i32>(floor(q));
  let fr = q - floor(q);
  if (f < 0.5) {
    let a = mix(textureLoad(texs[t], clamp(i, vec2<i32>(0), hi), 0), textureLoad(texs[t], clamp(i + vec2<i32>(1, 0), vec2<i32>(0), hi), 0), fr.x);
    let b = mix(textureLoad(texs[t], clamp(i + vec2<i32>(0, 1), vec2<i32>(0), hi), 0), textureLoad(texs[t], clamp(i + vec2<i32>(1, 1), vec2<i32>(0), hi), 0), fr.x);
    return mix(a, b, fr.y);
  }
  let wx = mitchell(fr.x);
  let wy = mitchell(fr.y);
  var acc = vec4<f32>(0.0);
  for (var r = 0; r < 4; r++) {
    var row = vec4<f32>(0.0);
    for (var k = 0; k < 4; k++) {
      row += textureLoad(texs[t], clamp(i + vec2<i32>(k - 1, r - 1), vec2<i32>(0), hi), 0) * wx[k];
    }
    acc += row * wy[r];
  }
  return clamp(acc, vec4<f32>(0.0), vec4<f32>(1.0));
}

fn paint(I: Inst, l: vec2<f32>) -> vec4<f32> {
  let mode = u32(I.p.z);
  if (mode == 0u) { return pm(I.color); }
  let pp = vec2<f32>(I.m0.x * l.x + I.m0.z * l.y + I.m1.x, I.m0.y * l.x + I.m0.w * l.y + I.m1.y);
  if (mode == 3u) {
    return sample_image(I, pp) * I.color.a;
  }
  if (mode == 4u) {
    // a video frame: NV12 planes (luma, then chroma in the next slot), BT.709 limited range, as VideoToolbox decodes HD.
    // Each plane filtered as an image brush is (nearest, bilinear, bicubic: the brush's quality), luma at its own
    // texels and chroma at half size where its samples sit (left-sited: on the even luma columns, between the rows),
    // then converted: what VideoToolbox's BGRA, filtered the same way, gives, without converting the whole frame
    let n = vec2<i32>(I.a.zw);
    let y = (plane(tex_of(I), pp, n, I.p.w).r * 255.0 - 16.0) / 219.0;
    let cn = (n + vec2<i32>(1)) / 2;
    // chroma always bilinear, whatever the luma's filter: it's upsampled from half size, as VideoToolbox's own conversion
    // does (nearest or bicubic chroma missed by up to 34 levels on colour edges; bilinear 11)
    let c = (plane(tex_of(I) + 1u, vec2<f32>(pp.x * 0.5 + 0.25, pp.y * 0.5), cn, 0.0).rg * 255.0 - 128.0) / 224.0;
    let rgb = vec3<f32>(y + 1.5748 * c.y, y - 0.1873 * c.x - 0.4681 * c.y, y + 1.8556 * c.x);
    return vec4<f32>(clamp(rgb, vec3<f32>(0.0), vec3<f32>(1.0)), 1.0) * I.color.a;
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


// ---- vello_cpu's u8 arithmetic

fn div255(x: u32) -> u32 { return (x + ((x + 128u) >> 8u) + 128u) >> 8u; }
fn to8(v: vec4<f32>) -> vec4<u32> { return vec4<u32>(clamp(round(v * 255.0), vec4<f32>(0.0), vec4<f32>(255.0))); }
fn cov8(c: f32) -> u32 { return u32(clamp(round(c * 255.0), 0.0, 255.0)); }

// What an instance puts on a pixel: its premultiplied colour, its coverage, and how vello composites it.
struct Src {
  c: vec4<u32>,
  m: u32,
  // 0: a solid fill (Skia's truncating blend on interior spans); 1: a paint (rounding); 2: a popped layer (truncating
  // everywhere)
  mode: u32,
  interior: bool,
}

fn compose(dst: vec4<u32>, s: Src) -> vec4<u32> {
  if (s.m == 0u) { return dst; }
  let sa = s.c.a;
  if ((s.mode == 2u && s.m == 255u) || (s.interior && s.m == 255u && s.mode == 0u)) {
    // SkPMSrcOver: dst * (256 - srcA) >> 8, plus the source
    return min(((dst * (256u - sa)) >> vec4<u32>(8u)) + s.c, vec4<u32>(255u));
  }
  if (s.interior && s.m == 255u) {
    return min(vec4<u32>(div255(dst.r * (255u - sa)), div255(dst.g * (255u - sa)), div255(dst.b * (255u - sa)), div255(dst.a * (255u - sa))) + s.c, vec4<u32>(255u));
  }
  let inv = 255u - div255(sa * s.m);
  let x = dst * inv + s.c * s.m;
  return min(vec4<u32>(div255(x.r), div255(x.g), div255(x.b), div255(x.a)), vec4<u32>(255u));
}

fn to_local(I: Inst, p: vec2<f32>) -> vec2<f32> {
  return vec2<f32>(I.xi0.x * p.x + I.xi0.z * p.y + I.xi1.x, I.xi0.y * p.x + I.xi0.w * p.y + I.xi1.y);
}

fn holed(I: Inst) -> bool { return I.hole.z > I.hole.x && I.hole.w > I.hole.y; }

fn clip_cov(I: Inst, p: vec2<f32>) -> f32 {
  var c = clamp(0.5 - sd_rrect(p, I.clip, I.clipr), 0.0, 1.0);
  if (holed(I)) { c = c * clamp(0.5 + sd_rrect(p, I.hole, I.holer), 0.0, 1.0); }
  return c;
}

// Whether the pixel's 4x4 tile (vello's strip tile) is clear of every edge of this instance and its clip: vello fills
// such tiles as spans, the rest as strips.
fn tile_inside(I: Inst, p: vec2<f32>) -> bool {
  // vello's strips come from edges crossing scanlines: a vertical edge on the tile's border makes a strip, a
  // horizontal one there doesn't; so the corners sit just outside the tile in x and just inside it in y
  let t0 = floor(p / 4.0) * 4.0;
  let e = 1e-3;
  let cs = array<vec2<f32>, 4>(t0 + vec2<f32>(-e, e), t0 + vec2<f32>(4.0 + e, e), t0 + vec2<f32>(-e, 4.0 - e), t0 + vec2<f32>(4.0 + e, 4.0 - e));
  for (var k = 0; k < 4; k++) {
    let c = cs[k];
    if (sd_rrect(c, I.clip, I.clipr) > 0.0) { return false; }
    let l = to_local(I, c);
    if (sd_rrect(l, I.shape, I.radii) > 0.0) { return false; }
    if (I.inner.z > I.inner.x && I.inner.w > I.inner.y && sd_rrect(l, I.inner, I.innerr) <= 0.0) { return false; }
  }
  return true;
}

fn shade(I: Inst, p: vec2<f32>, inside: bool) -> Src {
  var s: Src;
  let kind = u32(I.p.x);
  let clipped = clip_cov(I, p);
  // glyphs and textures are device-pixel boxes: nothing outside them (their texels beyond belong to others)
  if (kind >= 2u && !(all(p >= I.quad.xy) && all(p < I.quad.zw))) {
    s.m = 0u;
    return s;
  }
  if (kind == 2u) {
    // a glyph: a solid fill whose coverage is vello's own raster; a span where its whole 4x4 tile is covered
    let o = I.a.xy - I.quad.xy;
    let t = vec2<i32>(o + floor(p));
    if (SKIA_TEXT) {
      let c8 = to8(I.color);
      let pre = vec4<u32>(div255(c8.r * c8.a), div255(c8.g * c8.a), div255(c8.b * c8.a), c8.a);
      let mk = cov8(textureLoad(atlas, t, 0).r);
      s.c = vec4<u32>(div255(pre.r * mk), div255(pre.g * mk), div255(pre.b * mk), div255(pre.a * mk));
      s.m = cov8(clipped);
      s.mode = 1u;
      s.interior = true;
      return s;
    }
    s.c = to8(pm(I.color));
    s.m = cov8(textureLoad(atlas, t, 0).r * clipped);
    s.mode = 0u;
    s.interior = inside;
    return s;
  }
  if (kind == 3u) {
    let o = I.a.xy - I.quad.xy;
    let t = vec2<i32>(o + floor(p));
    let mode = u32(I.pad.y + 0.5);
    if (mode == 3u || mode == 4u) {
      // a solid fill's coverage, rasterized by vello_cpu: shaded in its colour as vello composites the fill
      let mk = cov8(textureLoad(texs[tex_of(I)], t, 0).a);
      if (mode == 4u) {
        let c8 = to8(I.color);
        let pre = vec4<u32>(div255(c8.r * c8.a), div255(c8.g * c8.a), div255(c8.b * c8.a), c8.a);
        s.c = vec4<u32>(div255(pre.r * mk), div255(pre.g * mk), div255(pre.b * mk), div255(pre.a * mk));
        s.m = cov8(clipped);
        s.mode = 1u;
        s.interior = true;
        return s;
      }
      s.c = to8(pm(I.color));
      s.m = div255(mk * cov8(clipped));
      s.mode = 0u;
      s.interior = inside;
      return s;
    }
    var c = to8(textureLoad(texs[tex_of(I)], t, 0));
    let alpha = cov8(I.color.a);
    if (alpha < 255u) { c = vec4<u32>(div255(c.r * alpha), div255(c.g * alpha), div255(c.b * alpha), div255(c.a * alpha)); }
    s.c = c;
    s.m = cov8(clipped);
    // 2: a popped layer (the truncating blend); otherwise a paint
    s.mode = select(1u, 2u, mode == 2u);
    s.interior = true;
    return s;
  }
  let l = to_local(I, p);
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
    // vello paints the blur as a colour over its inflated rect: a paint, covered by the rect and the clip
    s.c = to8(pm(I.color) * z);
    s.m = select(0u, cov8(clipped), all(l >= I.quad.xy) && all(l <= I.quad.zw));
    s.mode = 1u;
    s.interior = inside;
    return s;
  }
  var cov = cov_rrect(l, I.shape, I.radii, I.xi0);
  if (I.inner.z > I.inner.x && I.inner.w > I.inner.y) {
    cov = cov * (1.0 - cov_rrect(l, I.inner, I.innerr, I.xi0));
  }
  s.m = cov8(cov * clipped);
  if (s.m == 0u) { return s; }
  s.c = to8(paint(I, l));
  s.mode = select(1u, 0u, u32(I.p.z) == 0u);
  s.interior = inside;
  return s;
}

// A shadow's (or any quad's) tile clear of its covering rect's edges and the clip.
fn tile_inside_rect(I: Inst, p: vec2<f32>) -> bool {
  // vello's strips come from edges crossing scanlines: a vertical edge on the tile's border makes a strip, a
  // horizontal one there doesn't; so the corners sit just outside the tile in x and just inside it in y
  let t0 = floor(p / 4.0) * 4.0;
  let e = 1e-3;
  let cs = array<vec2<f32>, 4>(t0 + vec2<f32>(-e, e), t0 + vec2<f32>(4.0 + e, e), t0 + vec2<f32>(-e, 4.0 - e), t0 + vec2<f32>(4.0 + e, 4.0 - e));
  for (var k = 0; k < 4; k++) {
    if (sd_rrect(cs[k], I.clip, I.clipr) > 0.0) { return false; }
    let l = to_local(I, cs[k]);
    if (!(all(l >= I.quad.xy) && all(l <= I.quad.zw))) { return false; }
  }
  return true;
}

fn pm(c: vec4<f32>) -> vec4<f32> { return vec4<f32>(c.rgb * c.a, c.a); }

// Which of a 16x16 tile's sixteen 4x4 strip tiles are spans for an instance (bit i: subtile i, row-major).
fn span_mask(I: Inst, tile0: vec2<f32>) -> u32 {
  var mask = 0u;
  let kind = u32(I.p.x);
  let mode = u32(I.pad.y + 0.5);
  for (var i = 0u; i < 16u; i++) {
    let t0 = tile0 + vec2<f32>(f32(i % 4u), f32(i / 4u)) * 4.0;
    let c = t0 + 2.0;
    var inside = false;
    if (kind == 0u) {
      inside = tile_inside(I, c);
    } else if (kind == 1u) {
      inside = tile_inside_rect(I, c);
    } else if (kind == 2u || (kind == 3u && mode == 3u)) {
      // coverage from a raster: a span where all sixteen pixels are fully covered, inside the box and the clip
      var full = sd_rrect(t0, I.clip, I.clipr) <= 0.0 && sd_rrect(t0 + 4.0, I.clip, I.clipr) <= 0.0;
      let o = I.a.xy - I.quad.xy;
      for (var k = 0; k < 16 && full; k++) {
        let q = t0 + vec2<f32>(f32(k % 4), f32(k / 4)) + 0.5;
        full = all(q >= I.quad.xy) && all(q < I.quad.zw);
        if (full) {
          let tq = vec2<i32>(o + floor(q));
          if (kind == 2u) { full = textureLoad(atlas, tq, 0).r >= 1.0; } else { full = textureLoad(texs[tex_of(I)], tq, 0).a >= 1.0; }
        }
      }
      inside = full;
    } else {
      inside = true;
    }
    // the hole's edges make strips: no span in a 4x4 tile it touches
    if (inside && holed(I) && all(t0 < I.hole.zw + 1e-3) && all(t0 + 4.0 > I.hole.xy - 1e-3)) { inside = false; }
    if (inside) { mask |= 1u << i; }
  }
  return mask;
}

const CHUNK: u32 = 256u;
const FULL: u32 = 0x80000000u;

// A fill covering the pixel whole, its clip too (FULL): what shade gives there, coverage 255 on a span, without
// working coverage out
fn full_paint(I: Inst, p: vec2<f32>) -> Src {
  var s: Src;
  s.m = 255u;
  s.c = to8(paint(I, to_local(I, p)));
  s.mode = select(1u, 0u, u32(I.p.z) == 0u);
  s.interior = true;
  return s;
}
var<workgroup> items: array<u32, 256>;
var<workgroup> spans: array<u32, 256>;

@compute @workgroup_size(16, 16)
fn fine(@builtin(workgroup_id) wid: vec3<u32>, @builtin(local_invocation_index) li: u32, @builtin(local_invocation_id) lid: vec3<u32>) {
  let tile = wid.y * pass_.tiles_x + wid.x;
  let at = wid.xy + pass_.origin;
  let gid = at * 16u + lid.xy;
  let start = bins[pass_.offsets + tile];
  let end = bins[pass_.offsets + tile + 1u];
  let tile0 = vec2<f32>(at * 16u);
  let sub = (lid.y / 4u) * 4u + lid.x / 4u;
  var dst = to8(pass_.clear);
  let p = vec2<f32>(gid.xy) + 0.5;
  // a tile one instance covers whole (a 3D view, a page-sized image): no spans to share, so no staging; the same
  // composite the loop below makes
  let lone = end == start + 1u && (bins[start] & FULL) != 0u;
  if (lone) {
    dst = compose(dst, full_paint(insts[bins[start] & 0x7fffffffu], p));
  }
  // the tile's instances in chunks: each thread brings one in and works out its spans, then every pixel composites
  // the chunk in paint order
  for (var base = select(start, end, lone); base < end; base += CHUNK) {
    let n = min(CHUNK, end - base);
    if (li < n) {
      let e = bins[base + li];
      items[li] = e;
      // an instance covering the whole tile (FULL: gpu.rs, covered_rect) is a span on every strip tile
      spans[li] = select(span_mask(insts[e & 0x7fffffffu], tile0), 0xffffu, (e & FULL) != 0u);
    }
    workgroupBarrier();
    for (var k = 0u; k < n; k++) {
      let e = items[k];
      let I = insts[e & 0x7fffffffu];
      if ((e & FULL) != 0u) {
        dst = compose(dst, full_paint(I, p));
      } else {
        dst = compose(dst, shade(I, p, (spans[k] >> sub & 1u) == 1u));
      }
    }
    workgroupBarrier();
  }
  // the target holds the pass's tiles from its origin: the page's own, or a group picture's
  let at_out = wid.xy * 16u + lid.xy;
  if (gid.x < pass_.size.x && gid.y < pass_.size.y && all(at_out < textureDimensions(out))) {
    textureStore(out, vec2<i32>(at_out), vec4<f32>(dst) / 255.0);
  }
}
