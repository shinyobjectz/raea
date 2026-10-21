// A model's look (look.rs): its low-resolution render (one texel a cell) drawn over the frame, cell by cell.
//   mode 1, pixel: each cell its texel's colour, and a one-cell outline round the silhouette in `edge`
//   mode 2, site:  raea.io's grain (apps/site/src/components/effects/dither-field.tsx): each cell's light ordered-dithered
//                  with a 4x4 Bayer matrix into the site's six greys, dark or light
//   modes 3 and 4, the whole frame's look (a <Look>): as 2 and 1 over every cell, opaque, and the site's greys keep
//                  `color` of each cell's hue (a red tile is dithered red, not grey)
//   mode 5, ink: the whole frame in two colours, as Return of the Obra Dinn draws it: each cell's light ordered-dithered
//                (an 8x8 Bayer matrix) into `paper` or `edge` (the ink), and where a cell's light steps by more than
//                `line` from a neighbour's, a one-cell line of ink; `band` narrows the dither to the light near
//                `threshold`, so what's darker is solid ink and what's lighter clean paper
//   mode 6, paint: the frame as an editorial illustration over flat-toned surfaces (paint.wgsl lit them): every edge
//                ragged (each pixel reads the frame a little off where it is, by seeded value noise `rag` pixels deep
//                and `rag_scale` pixels across, plus a fine dry-brush break-up), snapped to `pal` if there is one
//                (nearest in Oklab), the sky (`sky_top` to `sky_bottom`) wherever nothing was drawn, four taps a
//                pixel so a snapped edge isn't stair-stepped, and a fixed paper grain over it all (`grain`). Every
//                noise is a function of the pixel and `seed` alone: the same frame paints the same.

struct LookParams {
    mode: u32,
    dark: u32,
    size: vec2<f32>,
    edge: vec4<f32>,
    lift: f32,
    gain: f32,
    color: f32,
    paper: vec4<f32>,
    line: f32,
    threshold: f32,
    band: f32,
    grain: f32,
    rag: f32,
    rag_scale: f32,
    seed: f32,
    sky_top: vec4<f32>,
    sky_bottom: vec4<f32>,
    npal: u32,
    pal: array<vec4<f32>, 16>,
};

@group(#{MATERIAL_BIND_GROUP}) @binding(0) var<uniform> p: LookParams;
@group(#{MATERIAL_BIND_GROUP}) @binding(1) var cells: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(2) var cells_sampler: sampler;

struct VIn {
    @location(0) position: vec3<f32>,
    @location(2) uv: vec2<f32>,
};

struct VOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

// the quad (a unit Rectangle) covers the whole frame whatever the camera
@vertex
fn vertex(v: VIn) -> VOut {
    var o: VOut;
    // a model's cells on the near plane; the frame's (opaque, drawn first) on the far one, so a model's go over them
    var z = 1.0;
    if p.mode >= 3u {
        z = 0.000001;
    }
    o.clip = vec4<f32>(v.position.x * 2.0, v.position.y * 2.0, z, 1.0);
    o.uv = v.uv;
    return o;
}

fn cell(c: vec2<i32>) -> vec4<f32> {
    let s = vec2<i32>(p.size);
    if c.x < 0 || c.y < 0 || c.x >= s.x || c.y >= s.y {
        return vec4<f32>(0.0);
    }
    return textureLoad(cells, c, 0);
}

fn to_linear(c: f32) -> f32 {
    if c <= 0.04045 {
        return c / 12.92;
    }
    return pow((c + 0.055) / 1.055, 2.4);
}

fn to_srgb(c: f32) -> f32 {
    if c <= 0.0031308 {
        return c * 12.92;
    }
    return 1.055 * pow(c, 1.0 / 2.4) - 0.055;
}

const BAYER = array<f32, 16>(0.0, 8.0, 2.0, 10.0, 12.0, 4.0, 14.0, 6.0, 3.0, 11.0, 1.0, 9.0, 15.0, 7.0, 13.0, 5.0);
const DARK = array<f32, 6>(0.0, 14.0, 30.0, 62.0, 130.0, 228.0);
// the light greys run on to the site's ink (the dark ones mirrored): its field stops at 121, but a figure needs the ink
const LIGHT = array<f32, 6>(250.0, 240.0, 225.0, 193.0, 125.0, 27.0);

@fragment
fn fragment(i: VOut) -> @location(0) vec4<f32> {
    let c = vec2<i32>(floor(i.uv * p.size));
    let t = cell(c);
    if p.mode == 6u {
        return vec4<f32>(paint(i.uv), 1.0);
    }
    if p.mode == 5u {
        return vec4<f32>(ink(c), 1.0);
    }
    if p.mode == 4u {
        return vec4<f32>(t.rgb, 1.0);
    }
    if p.mode == 3u {
        return vec4<f32>(site(t.rgb, c, true), 1.0);
    }
    let near = max(max(cell(c + vec2<i32>(1, 0)).a, cell(c - vec2<i32>(1, 0)).a), max(cell(c + vec2<i32>(0, 1)).a, cell(c - vec2<i32>(0, 1)).a));
    if t.a < 0.5 {
        if near > 0.5 && p.edge.a > 0.0 && p.mode == 1u {
            return vec4<f32>(p.edge.rgb, 1.0);
        }
        discard;
    }
    if p.mode == 1u {
        return vec4<f32>(t.rgb, 1.0);
    }
    return vec4<f32>(site(t.rgb, c, false), 1.0);
}

/// A cell's colour in the site's grain: its light (lifted so a dark surface still reads as ink) ordered-dithered into
/// the six greys; with `hue`, the grey takes `p.color` of the cell's hue (its brightest channel at the grey's level).
fn site(rgb: vec3<f32>, c: vec2<i32>, hue: bool) -> vec3<f32> {
    // keeping the hue, a cell's light is its brightest channel (a saturated red is bright, though its luminance isn't)
    var y = dot(rgb, vec3<f32>(0.2126, 0.7152, 0.0722));
    if hue {
        y = mix(y, max(max(rgb.r, rgb.g), rgb.b), p.color);
    }
    let lum = to_srgb(y);
    // on the light greys ink is darkness: a dark surface is the strong grey, a white one (the cat's eyes) the paper
    var light = lum;
    if p.dark == 0u {
        light = 1.0 - lum;
    }
    let lit = clamp(p.lift + light * p.gain, 0.0, 1.0);
    let b = (BAYER[(c.y & 3) * 4 + (c.x & 3)] + 0.5) / 16.0;
    let v = clamp(lit + b * 0.22 - 0.06, 0.0, 0.999);
    let k = u32(floor(v * 6.0));
    var g = LIGHT[k];
    if p.dark == 1u {
        g = DARK[k];
    }
    let l = to_linear(g / 255.0);
    if !hue {
        return vec3<f32>(l);
    }
    let top = max(max(rgb.r, rgb.g), rgb.b);
    if top <= 0.0 {
        return vec3<f32>(l);
    }
    return l * mix(vec3<f32>(1.0), rgb / top, p.color);
}

fn light_of(c: vec2<i32>) -> f32 {
    let s = vec2<i32>(p.size);
    let k = clamp(c, vec2<i32>(0), s - vec2<i32>(1));
    let rgb = textureLoad(cells, k, 0).rgb;
    return clamp(p.lift + to_srgb(dot(rgb, vec3<f32>(0.2126, 0.7152, 0.0722))) * p.gain, 0.0, 1.0);
}

/// the 8x8 Bayer matrix's threshold at a cell, in (0, 1)
fn bayer8(c: vec2<i32>) -> f32 {
    let x = u32(c.x) & 7u;
    let y = u32(c.y) & 7u;
    let z = x ^ y;
    // bit-reversed interleave of y and x ^ y
    let v = ((z & 1u) << 5u) | ((y & 1u) << 4u) | ((z & 2u) << 2u) | ((y & 2u) << 1u) | ((z & 4u) >> 1u) | ((y & 4u) >> 2u);
    return (f32(v) + 0.5) / 64.0;
}

/// A cell in the ink look: ink where the light steps (the lighter side of the step, so a figure is ringed), else the
/// light dithered between ink and paper.
fn ink(c: vec2<i32>) -> vec3<f32> {
    let l = light_of(c);
    if p.line > 0.0 {
        let lo = min(min(light_of(c + vec2<i32>(1, 0)), light_of(c - vec2<i32>(1, 0))), min(light_of(c + vec2<i32>(0, 1)), light_of(c - vec2<i32>(0, 1))));
        if l - lo > p.line {
            return p.edge.rgb;
        }
    }
    // hard ink and paper either side of `threshold`, dithered only across a `band` of light around it
    if (l - p.threshold) / max(p.band, 1e-4) + 0.5 > bayer8(c) {
        return p.paper.rgb;
    }
    return p.edge.rgb;
}

// ---- paint (mode 6) ----

fn pcg(v: u32) -> u32 {
    let st = v * 747796405u + 2891336453u;
    let w = ((st >> ((st >> 28u) + 4u)) ^ st) * 277803737u;
    return (w >> 22u) ^ w;
}

/// a value in [0, 1) for an integer lattice point and a salt
fn hash3(x: i32, y: i32, s: u32) -> f32 {
    return f32(pcg(pcg(u32(x) ^ pcg(u32(y) + s * 0x9e3779b9u))) & 0xffffffu) / 16777216.0;
}

/// smooth value noise in [0, 1)
fn vnoise(q: vec2<f32>, s: u32) -> f32 {
    let i = floor(q);
    let f = q - i;
    let u = f * f * (3.0 - 2.0 * f);
    let x = i32(i.x);
    let y = i32(i.y);
    let a = hash3(x, y, s);
    let b = hash3(x + 1, y, s);
    let c = hash3(x, y + 1, s);
    let d = hash3(x + 1, y + 1, s);
    return mix(mix(a, b, u.x), mix(c, d, u.x), u.y);
}

fn oklab(c: vec3<f32>) -> vec3<f32> {
    let l = 0.4122214708 * c.r + 0.5363325363 * c.g + 0.0514459929 * c.b;
    let m = 0.2119034982 * c.r + 0.6806995451 * c.g + 0.1073969566 * c.b;
    let s = 0.0883024619 * c.r + 0.2817188376 * c.g + 0.6299787005 * c.b;
    let l_ = pow(max(l, 0.0), 1.0 / 3.0);
    let m_ = pow(max(m, 0.0), 1.0 / 3.0);
    let s_ = pow(max(s, 0.0), 1.0 / 3.0);
    return vec3<f32>(
        0.2104542553 * l_ + 0.7936177850 * m_ - 0.0040720468 * s_,
        1.9779984951 * l_ - 2.4285922050 * m_ + 0.4505937099 * s_,
        0.0259040371 * l_ + 0.7827717662 * m_ - 0.8086757660 * s_,
    );
}

/// a frame colour as painted: the sky where nothing was drawn, else the colour, snapped to the palette if there is one
fn paint_tone(t: vec4<f32>, v: f32) -> vec3<f32> {
    if t.a < 0.5 && p.sky_top.a > 0.0 {
        return mix(p.sky_top.rgb, p.sky_bottom.rgb, clamp(v, 0.0, 1.0));
    }
    if p.npal == 0u {
        return t.rgb;
    }
    let q = oklab(t.rgb);
    var best = p.pal[0].rgb;
    var bd = 1e9;
    for (var k = 0u; k < p.npal; k = k + 1u) {
        let d = oklab(p.pal[k].rgb) - q;
        // lightness counts double: a tone's value is what an illustrator matches first
        let e = d.x * d.x * 2.0 + d.y * d.y + d.z * d.z;
        if e < bd {
            bd = e;
            best = p.pal[k].rgb;
        }
    }
    return best;
}

fn paint_at(x: vec2<f32>) -> vec4<f32> {
    let s = vec2<i32>(p.size);
    let k = clamp(vec2<i32>(floor(x)), vec2<i32>(0), s - vec2<i32>(1));
    return textureLoad(cells, k, 0);
}

fn paint(uv: vec2<f32>) -> vec3<f32> {
    let px = uv * p.size;
    let sd = u32(i32(p.seed)) * 7919u + 17u;
    // where this pixel reads the frame: a wander `rag_scale` across and `rag` deep, and a fine break-up a fifth of it
    let q = px / p.rag_scale;
    var off = vec2<f32>(vnoise(q, sd) - 0.5, vnoise(q + vec2<f32>(41.3, 17.9), sd + 1u) - 0.5) * 2.0;
    let fq = px / max(p.rag_scale * 0.12, 1.5);
    off += vec2<f32>(vnoise(fq, sd + 2u) - 0.5, vnoise(fq + vec2<f32>(9.1, 3.7), sd + 3u) - 0.5) * 1.2;
    let at = px + off * p.rag;
    // four taps, each painted, averaged: a snapped edge comes out smooth
    var col = vec3<f32>(0.0);
    let taps = array<vec2<f32>, 4>(vec2<f32>(-0.25, -0.25), vec2<f32>(0.25, -0.25), vec2<f32>(-0.25, 0.25), vec2<f32>(0.25, 0.25));
    for (var k = 0; k < 4; k++) {
        col += paint_tone(paint_at(at + taps[k]), uv.y);
    }
    col *= 0.25;
    // paper: a fixed tooth (two scales of value noise) and a fine grain, in the frame's sRGB light
    if p.grain > 0.0 {
        let tooth = (vnoise(px / 2.2, 911u) - 0.5) * 0.6 + (vnoise(px / 7.0, 912u) - 0.5) * 0.5 + (hash3(i32(px.x), i32(px.y), 913u) - 0.5) * 0.7;
        let g = tooth * p.grain * 0.16;
        col = vec3<f32>(to_linear(clamp(to_srgb(col.r) + g, 0.0, 1.0)), to_linear(clamp(to_srgb(col.g) + g, 0.0, 1.0)), to_linear(clamp(to_srgb(col.b) + g, 0.0, 1.0)));
    }
    return col;
}
