// Ink outlines (outline.rs): the frame as drawn, with a line where the depth steps or the surface turns.
#import bevy_core_pipeline::fullscreen_vertex_shader::FullscreenVertexOutput

struct InkOutline {
    color: vec4<f32>,
    depth: f32,
    normal: f32,
    width: f32,
};

@group(0) @binding(0) var screen: texture_2d<f32>;
@group(0) @binding(1) var screen_sampler: sampler;
@group(0) @binding(2) var<uniform> p: InkOutline;
@group(0) @binding(3) var depth_tex: texture_depth_2d;
@group(0) @binding(4) var normal_tex: texture_2d<f32>;

// distance from the camera, up to a factor (Bevy's reverse-Z: depth is near / distance; nothing drawn is 0, far away)
fn dist(c: vec2<i32>) -> f32 {
    let s = vec2<i32>(textureDimensions(depth_tex));
    let d = textureLoad(depth_tex, clamp(c, vec2<i32>(0), s - vec2<i32>(1)), 0);
    return 1.0 / max(d, 1e-7);
}

fn normal_at(c: vec2<i32>) -> vec3<f32> {
    let s = vec2<i32>(textureDimensions(normal_tex));
    return normalize(textureLoad(normal_tex, clamp(c, vec2<i32>(0), s - vec2<i32>(1)), 0).xyz * 2.0 - 1.0);
}

@fragment
fn fragment(in: FullscreenVertexOutput) -> @location(0) vec4<f32> {
    let here = textureSample(screen, screen_sampler, in.uv);
    let c = vec2<i32>(floor(in.position.xy));
    let z = dist(c);
    let n = normal_at(c);
    if p.width > 1.5 {
        return wide(c, z, n, here);
    }
    let offs = array<vec2<i32>, 4>(vec2<i32>(1, 0), vec2<i32>(-1, 0), vec2<i32>(0, 1), vec2<i32>(0, -1));
    for (var k = 0; k < 4; k++) {
        let o = c + offs[k];
        // a silhouette: the line on the nearer side, so a figure is ringed by it and not what's behind it
        if dist(o) - z > p.depth * z {
            return vec4<f32>(p.color.rgb, here.a);
        }
        // a crease: one side of it (the right and lower neighbours only), so the line is one pixel wide
        if p.normal > 0.0 && k % 2 == 0 && dist(o) < 1e6 && z < 1e6 && 1.0 - dot(n, normal_at(o)) > p.normal {
            return vec4<f32>(p.color.rgb, here.a);
        }
    }
    return here;
}

// A line `width` pixels wide and even (a cartoon's): a silhouette wherever something within the width's radius is farther by the threshold (so the line lies on the nearer side, ringing the figure), a crease wherever the
// surface within the radius on one side (right and down) turns by more than `normal`. 16 directions at three radii.
fn wide(c: vec2<i32>, z: f32, n: vec3<f32>, here: vec4<f32>) -> vec4<f32> {
    let r = p.width;
    for (var k = 0; k < 16; k++) {
        let a = f32(k) * 0.39269908;
        let d = vec2<f32>(cos(a), sin(a));
        for (var j = 1; j <= 3; j++) {
            let o = c + vec2<i32>(round(d * r * f32(j) / 3.0));
            let zo = dist(o);
            if zo - z > p.depth * z {
                return vec4<f32>(p.color.rgb, here.a);
            }
            if p.normal > 0.0 && (d.x + d.y) > 0.0 && zo < 1e6 && z < 1e6 && abs(zo - z) < p.depth * z && 1.0 - dot(n, normal_at(o)) > p.normal {
                return vec4<f32>(p.color.rgb, here.a);
            }
        }
    }
    return here;
}
