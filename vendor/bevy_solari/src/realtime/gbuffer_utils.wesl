import bevy_pbr::deferred::types::unpack_24bit_normal;
import bevy_pbr::render::rgb9e5::rgb9e5_to_vec3_;
import bevy_render::utils::octahedral_decode;
import bevy_render::view::{View, depth_ndc_to_view_z};
import package::scene::bindings::{facing_normal, ResolvedMaterial};
import package::realtime::bindings::view;

struct ResolvedGPixel {
    world_position: vec3<f32>,
    world_normal: vec3<f32>,
    material: ResolvedMaterial,
}

fn gpixel_resolve(gpixel: vec4<u32>, depth: f32, pixel_id: vec2<u32>, view_size: vec2<f32>, world_from_clip: mat4x4<f32>) -> ResolvedGPixel {
    let world_position = reconstruct_world_position(pixel_id, depth, view_size, world_from_clip);
    var world_normal = octahedral_decode(unpack_24bit_normal(gpixel.a));
    // raea: a smooth surface's interpolated normal past its silhouette (a dune's crest against the ground beyond) faces
    // away from the eye, and the BRDF gives nothing there: black lines along every crest. Bent back to face the eye a
    // little, as the path tracer does a hit's (bindings.wesl facing_normal). The eye from the inverse projection: Bevy's
    // perspective is infinite reverse-z, so clip (0, 0, 1, 0) is the eye; an orthographic view has none and is left be
    let eye = world_from_clip * vec4(0.0, 0.0, 1.0, 0.0);
    if abs(eye.w) > 1e-20 {
        let wo = normalize(eye.xyz / eye.w - world_position);
        world_normal = facing_normal(world_normal, wo, wo);
    }

    let base_rough = unpack4x8unorm(gpixel.r);
    let base_color = pow(base_rough.rgb, vec3(2.2));
    let perceptual_roughness = base_rough.a;
    let roughness = perceptual_roughness * perceptual_roughness;
    let props = unpack4x8unorm(gpixel.b);
    let reflectance = props.r;
    let metallic = props.g;
    // raea: Bevy's G-buffer holds it pre-exposed (bevy_pbr deferred/functions.wesl), so a bright lamp doesn't clip
    let emissive = rgb9e5_to_vec3_(gpixel.g) / view.exposure;
    // raea: the diffuse transmission, in the flags' top five bits (bevy_pbr deferred/functions.wesl)
    let diffuse_transmission = f32((gpixel.a >> 27u) & 31u) / 31.0;
    let material = ResolvedMaterial(base_color, emissive, reflectance, perceptual_roughness, roughness, metallic, diffuse_transmission);

    return ResolvedGPixel(world_position, world_normal, material);
}

fn reconstruct_world_position(pixel_id: vec2<u32>, depth: f32, view_size: vec2<f32>, world_from_clip: mat4x4<f32>) -> vec3<f32> {
    let uv = (vec2<f32>(pixel_id) + 0.5) / view_size;
    let xy_ndc = (uv - vec2(0.5)) * vec2(2.0, -2.0);
    let world_pos = world_from_clip * vec4(xy_ndc, depth, 1.0);
    return world_pos.xyz / world_pos.w;
}

/// Reject if tangent plane differs more than 0.3% or angle between normals more than 90 degrees
fn pixel_dissimilar(depth: f32, world_position: vec3<f32>, other_world_position: vec3<f32>, normal: vec3<f32>, other_normal: vec3<f32>, view: View) -> bool {
    // https://developer.download.nvidia.com/video/gputechconf/gtc/2020/presentations/s22699-fast-denoising-with-self-stabilizing-recurrent-blurs.pdf#page=45
    let tangent_plane_distance = abs(dot(normal, other_world_position - world_position));
    let view_z = -depth_ndc_to_view_z(depth, view.clip_from_view, view.view_from_clip);

    return tangent_plane_distance / view_z > 0.003 || dot(normal, other_normal) < 0.0;
}

fn permute_pixel(pixel_id: vec2<u32>, frame_index: u32, view_size: vec2<f32>) -> vec2<u32> {
    let r = frame_index;
    let offset = vec2<i32>(i32(r & 3u), i32((r >> 2u) & 3u));
    var shifted_pixel_id = vec2<i32>(pixel_id) + offset;
    shifted_pixel_id ^= vec2<i32>(3);
    shifted_pixel_id -= offset;
    if any(shifted_pixel_id < vec2<i32>(0)) || any(shifted_pixel_id >= vec2<i32>(view_size)) {
        return pixel_id;
    }
    return vec2<u32>(shifted_pixel_id);
}
