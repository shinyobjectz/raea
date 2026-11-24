// MetalFX's guides (metalfx.rs): written from Bevy's G-buffer (main) or Solari's guides (main_psr), with the
// emission split out; `restore` puts the emission back after the denoiser.
// Bevy's G-buffer (bevy_pbr deferred/functions.wesl, types.wesl) to MetalFX's guides: r = base colour (sRGB-ish, pow
// 1/2.2) and perceptual roughness as unorm4x8; b = reflectance, metallic, occlusion, clearcoat as unorm4x8; a = a 24-bit
// octahedral normal (two 12-bit halves) and flags; g = emissive as rgb9e5, pre-exposed (times the camera's exposure).
// What a surface emits is no noise. Denoised as lighting, a screen's picture or a sign's letters came out smeared, its
// texture taken for noise against a near-black albedo (photo 2's monitor). So the emission's share of each pixel's
// colour goes to MetalFX's denoise strength mask, which leaves a pixel marked 1 undenoised while still gathering its
// frames (which anti-aliases its edges), and the emission, compressed as e / (1 + e), is added to its albedo, so what
// the denoiser does smooth isn't swollen against a near-black one. A pixel with nothing in the G-buffer (a forward
// mesh, the sky) is raster-lit, with nothing to denoise, and is marked 1. Measured on photo 2's monitor:
// RAEA_SOLARI_EMISSIVE=split takes the emission (times the exposure, as Solari adds it) out of the colour into `emit`
// and `restore` puts it back after MetalFX, the first way, and kept out of the denoiser an emissive edge stayed
// stepped and crawled with the jitter (500 pixels jumping 80 levels a frame on the window's and curtain's edges);
// emission as the albedo where it led, the screen's dark parts kept their noise; added to every albedo with no mask,
// the picture blurred; split only where it was weak, pixels on the boundary counted it twice and sparkled
@group(0) @binding(0) var gbuf: texture_2d<u32>;
@group(0) @binding(1) var diffuse: texture_storage_2d<rgba16float, write>;
@group(0) @binding(2) var specular: texture_storage_2d<rgba16float, write>;
@group(0) @binding(3) var normal: texture_storage_2d<rgba16float, write>;
@group(0) @binding(4) var rough: texture_storage_2d<r32float, write>;
@group(0) @binding(5) var color: texture_storage_2d<rgba16float, read_write>;
// [0].x: the camera's exposure, .y: 1 to split all emission out (RAEA_SOLARI_EMISSIVE=split); [1]: the output's size
// and the input's (the corner of the colour the main pass drew); [2].xyz: the camera's back, world-space; [3].xy: the
// jitter MetalFX is told, in input pixels
@group(0) @binding(6) var<uniform> params: array<vec4<f32>, 4>;
// the emission `main` took out of the colour, at the input's size, for `restore` to put back
@group(0) @binding(7) var emit: texture_storage_2d<rgba16float, read_write>;
// MetalFX's denoise strength mask: 1 leaves a pixel undenoised
@group(0) @binding(8) var mask: texture_storage_2d<r32float, write>;
fn rgb9e5(v: u32) -> vec3<f32> {
    let scale = exp2(f32(i32(extractBits(v, 27u, 5u)) - 15 - 9));
    return vec3<f32>(f32(extractBits(v, 0u, 9u)), f32(extractBits(v, 9u, 9u)), f32(extractBits(v, 18u, 9u))) * scale;
}
fn unorm4x8(p: u32) -> vec4<f32> {
    return vec4<f32>(f32(p & 255u), f32((p >> 8u) & 255u), f32((p >> 16u) & 255u), f32(p >> 24u)) / 255.0;
}
fn octahedral(v: vec2<f32>) -> vec3<f32> {
    let f = v * 2.0 - 1.0;
    var n = vec3<f32>(f, 1.0 - abs(f.x) - abs(f.y));
    let t = saturate(-n.z);
    let w = select(vec2<f32>(t), vec2<f32>(-t), n.xy >= vec2<f32>(0.0));
    n = vec3<f32>(n.xy + w, n.z);
    return normalize(n);
}
// A surface as the denoiser's guides take it: diffuse albedo, specular albedo, world normal, perceptual roughness
struct Surface {
    diffuse: vec3<f32>,
    specular: vec3<f32>,
    normal: vec3<f32>,
    rough: f32,
}
// from Bevy's G-buffer: specular albedo is F0 (Bevy's 0.16 reflectance² for dielectrics, the base colour for metals)
fn from_gbuffer(g: vec4<u32>) -> Surface {
    let a = unorm4x8(g.r);
    let props = unorm4x8(g.b);
    let base = pow(a.rgb, vec3<f32>(2.2));
    let metallic = props.y;
    let f0 = mix(vec3<f32>(0.16 * props.x * props.x), base, metallic);
    let n = octahedral(vec2<f32>(f32(g.a & 0xFFFu), f32((g.a >> 12u) & 0xFFFu)) / 4095.0);
    return Surface(base * (1.0 - metallic), f0, n, a.w);
}
// Solari's guides (bevy_solari realtime/guides.rs), written by its lighting pass: the surface's own, or for a mirror's
// pixel those of what it reflects (primary surface replacement), so what a mirror shows keeps its detail
@group(0) @binding(9) var guide_diffuse: texture_2d<f32>;
@group(0) @binding(10) var guide_specular: texture_2d<f32>;
@group(0) @binding(11) var guide_normal: texture_2d<f32>;
fn from_guides(p: vec2<i32>) -> Surface {
    let nr = textureLoad(guide_normal, p, 0);
    return Surface(textureLoad(guide_diffuse, p, 0).rgb, textureLoad(guide_specular, p, 0).rgb, nr.xyz, nr.w);
}
// a pixel with nothing deferred (a forward mesh, the sky): its colour as its albedo, so there's nothing to smooth.
// True when it was one
fn undeferred(p: vec2<i32>, g: vec4<u32>) -> bool {
    if (any(g != vec4<u32>(0u))) { return false; }
    sky(p);
    return true;
}
fn sky(p: vec2<i32>) {
    let c = textureLoad(color, p).rgb;
    textureStore(diffuse, p, vec4<f32>(c / max(1.0, max(c.r, max(c.g, c.b))), 1.0));
    textureStore(specular, p, vec4<f32>(0.0, 0.0, 0.0, 1.0));
    textureStore(normal, p, vec4<f32>(params[2].xyz, 0.0));
    textureStore(rough, p, vec4<f32>(1.0, 0.0, 0.0, 0.0));
    textureStore(emit, p, vec4<f32>(0.0));
    textureStore(mask, p, vec4<f32>(1.0, 0.0, 0.0, 0.0));
}
fn store(p: vec2<i32>, g: vec4<u32>, s: Surface) {
    textureStore(normal, p, vec4<f32>(s.normal, 0.0));
    textureStore(rough, p, vec4<f32>(s.rough, 0.0, 0.0, 0.0));
    textureStore(specular, p, vec4<f32>(s.specular, 1.0));
    let c = textureLoad(color, p);
    // the G-buffer's emission is pre-exposed already: as Solari adds it to the colour
    let e = rgb9e5(g.g);
    if (params[0].y > 0.5) {
        let taken = min(e, c.rgb);
        textureStore(diffuse, p, vec4<f32>(s.diffuse, 1.0));
        textureStore(color, p, vec4<f32>(c.rgb - taken, c.a));
        textureStore(emit, p, vec4<f32>(taken, 0.0));
        textureStore(mask, p, vec4<f32>(0.0));
        return;
    }
    let luma = vec3<f32>(0.2126, 0.7152, 0.0722);
    let share = dot(e, luma) / max(dot(c.rgb, luma), 1e-4);
    textureStore(diffuse, p, vec4<f32>(min(s.diffuse + e / (vec3<f32>(1.0) + e), vec3<f32>(1.0)), 1.0));
    textureStore(mask, p, vec4<f32>(smoothstep(0.3, 0.7, share), 0.0, 0.0, 0.0));
}
@compute @workgroup_size(8, 8, 1)
fn main(@builtin(global_invocation_id) id: vec3<u32>) {
    let size = textureDimensions(diffuse);
    if (id.x >= size.x || id.y >= size.y) { return; }
    let p = vec2<i32>(id.xy);
    let g = textureLoad(gbuf, p, 0);
    if (undeferred(p, g)) { return; }
    store(p, g, from_gbuffer(g));
}
@compute @workgroup_size(8, 8, 1)
fn main_psr(@builtin(global_invocation_id) id: vec3<u32>) {
    let size = textureDimensions(diffuse);
    if (id.x >= size.x || id.y >= size.y) { return; }
    let p = vec2<i32>(id.xy);
    let g = textureLoad(gbuf, p, 0);
    if (undeferred(p, g)) { return; }
    // a mirror's pixel that shows the sky (Solari's diffuse albedo's alpha 1): guided as the sky's own
    if (textureLoad(guide_diffuse, p, 0).a > 0.5) {
        sky(p);
        return;
    }
    store(p, g, from_guides(p));
}
fn emitted(i: vec2<i32>) -> vec3<f32> {
    return textureLoad(emit, clamp(i, vec2<i32>(0), vec2<i32>(params[1].zw) - 1)).rgb;
}
@compute @workgroup_size(8, 8, 1)
fn restore(@builtin(global_invocation_id) id: vec3<u32>) {
    let size = vec2<u32>(params[1].xy);
    if (id.x >= size.x || id.y >= size.y) { return; }
    let p = vec2<i32>(id.xy);
    // where this output pixel lies in the jittered G-buffer, between its pixels' centres
    let q = (vec2<f32>(id.xy) + 0.5) * params[1].zw / params[1].xy - params[3].xy - 0.5;
    let i = vec2<i32>(floor(q));
    let t = q - floor(q);
    let e = mix(mix(emitted(i), emitted(i + vec2<i32>(1, 0)), t.x), mix(emitted(i + vec2<i32>(0, 1)), emitted(i + vec2<i32>(1, 1)), t.x), t.y);
    let c = textureLoad(color, p);
    textureStore(color, p, vec4<f32>(c.rgb + e, c.a));
}
