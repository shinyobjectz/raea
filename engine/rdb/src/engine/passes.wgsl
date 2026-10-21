// A frame's passes over its picture (passes.rs): each value as bytes, written as the linear value the frame's sRGB
// output encodes back to exactly those bytes.

#import bevy_core_pipeline::fullscreen_vertex_shader::FullscreenVertexOutput

struct Params {
    view_from_clip: mat4x4<f32>,
}

@group(0) @binding(0) var<uniform> params: Params;
@group(0) @binding(1) var depth_texture: texture_depth_2d;
@group(0) @binding(2) var normal_texture: texture_2d<f32>;
@group(0) @binding(3) var kept: texture_2d<f32>;

// a byte a channel, as the linear value whose sRGB encoding is that byte
fn bytes(v: vec3<u32>) -> vec4<f32> {
    let b = vec3<f32>(v) / 255.0;
    return vec4(select(pow((b + 0.055) / 1.055, vec3(2.4)), b / 12.92, b <= vec3(0.04045)), 1.0);
}

// metres along the view in millimetres, 24 bits, the low byte in r; past everything 0xffffff
@fragment
fn depth(in: FullscreenVertexOutput) -> @location(0) vec4<f32> {
    let d = textureLoad(depth_texture, vec2<i32>(in.position.xy), 0);
    var mm = 0xffffffu;
    if (d > 0.0) {
        let v = params.view_from_clip * vec4(0.0, 0.0, d, 1.0);
        mm = u32(clamp(-v.z / v.w * 1000.0 + 0.5, 0.0, 16777214.0));
    }
    return bytes(vec3(mm & 255u, (mm >> 8u) & 255u, mm >> 16u));
}

// the world normal as the prepass keeps it (n x 0.5 + 0.5); past everything 0
@fragment
fn normal(in: FullscreenVertexOutput) -> @location(0) vec4<f32> {
    let p = vec2<i32>(in.position.xy);
    if (textureLoad(depth_texture, p, 0) <= 0.0) {
        return vec4(0.0, 0.0, 0.0, 1.0);
    }
    let n = textureLoad(normal_texture, p, 0).xyz;
    return bytes(vec3<u32>(clamp(n * 255.0 + 0.5, vec3(0.0), vec3(255.0))));
}

// the unlit scene as it was before the post-processing
@fragment
fn albedo(in: FullscreenVertexOutput) -> @location(0) vec4<f32> {
    return vec4(textureLoad(kept, vec2<i32>(in.position.xy), 0).rgb, 1.0);
}
