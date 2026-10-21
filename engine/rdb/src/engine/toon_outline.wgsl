// raea's ink outline (MeshToonMaterial's `outline`; engine/rdb/src/engine/toon.rs): the inverted hull. A second draw of the
// mesh with its front faces culled, each vertex pushed out in clip space along its normal's direction on screen by a
// fixed number of pixels, so the back faces that remain show as a band of even width around the silhouette and along
// creases, whatever the distance. The mesh drawn here carries smoothed normals (each position's faces averaged), so
// hard edges don't open gaps. Skinned and morphed meshes are posed as Bevy's own mesh shader poses them.

#import bevy_pbr::{
    mesh_bindings::mesh,
    mesh_functions,
    skinning,
    morph::{morph_position, morph_normal},
    forward_io::{Vertex, VertexOutput},
    view_transformations::position_world_to_clip,
    mesh_view_bindings::view,
}
#ifdef TONEMAP_IN_SHADER
#import bevy_core_pipeline::tonemapping::tone_mapping
#endif

struct Outline {
    // linear rgb, a = opacity
    color: vec4<f32>,
    // x = width in pixels, y = how far back the hull is pushed (a share of its depth)
    shape: vec4<f32>,
}

@group(#{MATERIAL_BIND_GROUP}) @binding(0) var<uniform> outline: Outline;

#ifdef MORPH_TARGETS
fn morph_vertex(vertex_in: Vertex, instance_index: u32) -> Vertex {
    var vertex = vertex_in;
    let first_vertex = mesh[instance_index].first_vertex_index;
    let vertex_index = vertex.index - first_vertex;
    let weight_count = bevy_pbr::morph::layer_count(instance_index);
    for (var i: u32 = 0u; i < weight_count; i++) {
        let weight = bevy_pbr::morph::weight_at(i, instance_index);
        if weight == 0.0 {
            continue;
        }
        vertex.position += weight * morph_position(vertex_index, i, instance_index);
#ifdef VERTEX_NORMALS
        vertex.normal += weight * morph_normal(vertex_index, i, instance_index);
#endif
    }
    return vertex;
}
#endif

@vertex
fn vertex(vertex_no_morph: Vertex) -> VertexOutput {
    var out: VertexOutput;
#ifdef MORPH_TARGETS
    var vertex = morph_vertex(vertex_no_morph, vertex_no_morph.instance_index);
#else
    var vertex = vertex_no_morph;
#endif

#ifdef SKINNED
    let world_from_local = skinning::skin_model(vertex.joint_indices, vertex.joint_weights, vertex_no_morph.instance_index);
#else
    let world_from_local = mesh_functions::get_world_from_local(vertex_no_morph.instance_index);
#endif

    out.world_position = mesh_functions::mesh_position_local_to_world(world_from_local, vec4<f32>(vertex.position, 1.0));
    var clip = position_world_to_clip(out.world_position.xyz);

#ifdef VERTEX_NORMALS
#ifdef SKINNED
    let n = skinning::skin_normals(world_from_local, vertex.normal);
#else
    let n = mesh_functions::mesh_normal_local_to_world(vertex.normal, vertex_no_morph.instance_index);
#endif
    out.world_normal = n;
    // the normal's direction on screen, in pixels; the vertex moved that way by the width (NDC spans 2 per viewport)
    let size = view.viewport.zw;
    let n_clip = (view.clip_from_world * vec4<f32>(n, 0.0)).xy;
    let px = n_clip * size;
    let len = length(px);
    if len > 1e-6 {
        let dir = px / len;
        let offset = dir * outline.shape.x * 2.0 / size;
        clip = vec4<f32>(clip.xy + offset * clip.w, clip.z, clip.w);
    }
#endif
    // pushed back a little (reverse-Z: smaller is farther), so where the hull meets the surface at the silhouette it
    // stays behind it and never fights it for the pixel
    clip.z = clip.z * (1.0 - outline.shape.y);
    out.position = clip;
#ifdef VERTEX_OUTPUT_INSTANCE_INDEX
    out.instance_index = vertex_no_morph.instance_index;
#endif
    return out;
}

@fragment
fn fragment(in: VertexOutput) -> @location(0) vec4<f32> {
    var c = outline.color;
#ifdef TONEMAP_IN_SHADER
    c = tone_mapping(c, view.color_grading);
#endif
    return c;
}
