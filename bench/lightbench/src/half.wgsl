// LightBench's half-resolution bounce (LB7, joined after the other three): `trace_half` traces the bounce for one
// pixel of each 2 x 2 block, a different one each frame so four frames cover them all, into `bounce_half`; the lighting pass
// then takes each pixel's bounce from the four nearest blocks' samples, weighted by distance and by how near each
// sample's surface is to the pixel's (its normal, and the pixel's plane), as Lumen and MegaLights upsample theirs.

// each block's bounced light (rgb) and 1 where it holds a surface's
@group(0) @binding(13) var<storage, read_write> bounce_half: array<vec4<f32>>;

// a G-buffer pixel's surface: its point, its normal, and w of the point 0 for the sky
struct Pixel { pos: vec4<f32>, n: vec3<f32> }

fn pixel_at(px: vec2<i32>) -> Pixel {
    let depth = textureLoad(depth_tex, px, 0);
    if depth <= 0.0 {
        return Pixel(vec4(0.0), vec3(0.0));
    }
    let g = textureLoad(gbuf, px, 0);
    let n = oct_decode(vec2(f32(g.a & 0xFFFu), f32((g.a >> 12u) & 0xFFFu)) / 4095.0);
    let uv = (vec2<f32>(px) + 0.5) / vec2<f32>(p.size.xy);
    let w = p.world_from_clip * vec4(uv.x * 2.0 - 1.0, 1.0 - uv.y * 2.0, depth, 1.0);
    return Pixel(vec4(w.xyz / w.w, 1.0), n);
}

fn half_size() -> vec2<u32> {
    return (p.size.xy + 1u) / 2u;
}

// the pixel block b samples this frame
fn half_pixel(b: vec2<u32>) -> vec2<i32> {
    let o = array(vec2(0u, 0u), vec2(1u, 1u), vec2(1u, 0u), vec2(0u, 1u))[p.size.w % 4u];
    return vec2<i32>(min(b * 2u + o, p.size.xy - 1u));
}

@compute @workgroup_size(8, 8, 1)
fn trace_half(@builtin(global_invocation_id) id: vec3<u32>) {
    let hs = half_size();
    if id.x >= hs.x || id.y >= hs.y {
        return;
    }
    let b = id.y * hs.x + id.x;
    let px = half_pixel(id.xy);
    let s = pixel_at(px);
    if s.pos.w == 0.0 {
        bounce_half[b] = vec4(0.0);
        return;
    }
    seed = pcg(u32(px.x) + u32(px.y) * p.size.x + p.size.w * p.size.x * p.size.y + 0x68e31da4u);
    rays = 0u;
    let o = s.pos.xyz + s.n * (2e-3 + 5e-4 * length(s.pos.xyz - p.eye.xyz));
    var e = vec3(0.0);
    if p.gi.y != 0u {
        e = cached(s.n, o);
    } else {
        e = bounced(s.n, o);
    }
    bounce_half[b] = vec4(e, 1.0);
    if rays > 0u {
        atomicAdd(&stats[0], rays);
    }
}

// a pixel's bounced light from the four blocks around it
fn upsampled(px: vec2<i32>, pos: vec3<f32>, n: vec3<f32>, dist: f32) -> vec3<f32> {
    let hs = vec2<i32>(half_size());
    let b0 = (px - 1) / 2;
    var sum = vec3(0.0);
    var wsum = 0.0;
    for (var k = 0; k < 4; k++) {
        let b = clamp(b0 + vec2(k & 1, k >> 1), vec2(0), hs - 1);
        let v = bounce_half[u32(b.y * hs.x + b.x)];
        if v.w == 0.0 {
            continue;
        }
        let spx = half_pixel(vec2<u32>(b));
        let s = pixel_at(spx);
        let near = max(0.0, 1.0 - length(vec2<f32>(spx - px)) / 3.0);
        let facing = pow(max(dot(n, s.n), 0.0), 8.0);
        let plane = exp(-abs(dot(n, s.pos.xyz - pos)) / (0.01 * dist + 0.005));
        let w = (near + 1e-3) * facing * plane;
        sum += v.rgb * w;
        wsum += w;
    }
    return select(vec3(0.0), sum / wsum, wsum > 1e-6);
}
