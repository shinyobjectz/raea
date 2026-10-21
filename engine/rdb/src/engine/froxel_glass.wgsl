// froxel_glass.rs: a see-through surface in a froxel-fogged scene, fogged by the grid at its own depth. Bevy's
// standard material's fragment (its lighting kept), then the grid's light and transmittance in front of the surface
// laid over it where Bevy's distance fog would be: the grid's apply pass has fogged what's behind it already.
#import bevy_pbr::{
    pbr_fragment::pbr_input_from_standard_material,
    pbr_functions::alpha_discard,
    pbr_types,
}
#import rdb::froxel_fog::fog_at

#ifdef PREPASS_PIPELINE
#import bevy_pbr::{
    prepass_io::{VertexOutput, FragmentOutput},
    pbr_deferred_functions::deferred_output,
}
#else
#import bevy_pbr::{
    forward_io::{VertexOutput, FragmentOutput},
    pbr_functions::{apply_pbr_lighting, main_pass_post_lighting_processing},
}
#endif

// a colour as the surface's blend takes it, behind the fog in front of it: straight alpha (blend) and premultiplied
// both gain the fog's light where the surface covers, added light (add) none
fn fogged(color: vec4<f32>, flags: u32, fog: vec4<f32>) -> vec4<f32> {
    let mode = flags & pbr_types::STANDARD_MATERIAL_FLAGS_ALPHA_MODE_RESERVED_BITS;
    if (mode == pbr_types::STANDARD_MATERIAL_FLAGS_ALPHA_MODE_ADD) {
        return vec4(color.rgb * fog.a, color.a);
    }
    if (mode == pbr_types::STANDARD_MATERIAL_FLAGS_ALPHA_MODE_PREMULTIPLIED) {
        return vec4(color.rgb * fog.a + fog.rgb * color.a, color.a);
    }
    return vec4(color.rgb * fog.a + fog.rgb, color.a);
}

@fragment
fn fragment(
    in: VertexOutput,
    @builtin(front_facing) is_front: bool,
) -> FragmentOutput {
    var pbr_input = pbr_input_from_standard_material(in, is_front);
    pbr_input.material.base_color = alpha_discard(pbr_input.material, pbr_input.material.base_color);
#ifdef PREPASS_PIPELINE
    let out = deferred_output(in, pbr_input);
#else
    var out: FragmentOutput;
    if (pbr_input.material.flags & pbr_types::STANDARD_MATERIAL_FLAGS_UNLIT_BIT) == 0u {
        out.color = apply_pbr_lighting(pbr_input);
    } else {
        out.color = pbr_input.material.base_color;
    }
    out.color = fogged(out.color, pbr_input.material.flags, fog_at(in.position.xy, in.world_position.xyz));
    out.color = main_pass_post_lighting_processing(pbr_input, out.color);
#endif
    return out;
}
