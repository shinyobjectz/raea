// paint.rs: a surface lit as an illustrator paints it (the paint and cartoon looks). The suns' light on it (how
// square it faces each, and whether it's in that sun's shadow, cut hard at half) is put into `bands` flat tones: lit
// is the surface's own colour, turned from the light its colour times `mid`, in a cast shadow its colour times
// `shadow` (bands 2: one shade, `shadow`, for both; 4 and more: the lit side stepped from `mid` up by how square it
// faces the light). No specular, no ambient gradient, no point lights: chosen colours, not
// physically blended ones. The rest of the look (ragged edges, grain, the sky, a palette) is look.wgsl's mode 6.
#import bevy_pbr::{
    pbr_fragment::pbr_input_from_standard_material,
    pbr_functions::alpha_discard,
    mesh_view_bindings as view_bindings,
    mesh_view_types,
    shadows,
    mesh_types::MESH_FLAGS_SHADOW_RECEIVER_BIT,
}

#ifdef PREPASS_PIPELINE
#import bevy_pbr::{
    prepass_io::{VertexOutput, FragmentOutput},
    pbr_deferred_functions::deferred_output,
}
#else
#import bevy_pbr::{
    forward_io::{VertexOutput, FragmentOutput},
    pbr_functions::main_pass_post_lighting_processing,
}
#endif

struct Paint {
    shadow: vec4<f32>,
    mid: vec4<f32>,
    bands: f32,
    _a: f32,
    _b: f32,
    _c: f32,
}

@group(#{MATERIAL_BIND_GROUP}) @binding(100)
var<uniform> paint: Paint;

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
    let c = pbr_input.material.base_color;
    let view_z = dot(vec4<f32>(
        view_bindings::view.view_from_world[0].z,
        view_bindings::view.view_from_world[1].z,
        view_bindings::view.view_from_world[2].z,
        view_bindings::view.view_from_world[3].z
    ), pbr_input.world_position);
    let suns = view_bindings::lights.n_directional_lights;
    // how square the surface faces the suns (`ndl`), and whether a sun that it faces reaches it (`lit`)
    var ndl = 0.0;
    var lit = false;
    if suns == 0u {
        ndl = 1.0;
        lit = true;
    }
    for (var i: u32 = 0u; i < suns; i = i + 1u) {
        let light = &view_bindings::lights.directional_lights[i];
        let d = dot(pbr_input.N, (*light).direction_to_light);
        if d <= 0.03 {
            continue;
        }
        var s = 1.0;
        if (pbr_input.flags & MESH_FLAGS_SHADOW_RECEIVER_BIT) != 0u
                && ((*light).flags & mesh_view_types::DIRECTIONAL_LIGHT_FLAGS_SHADOWS_ENABLED_BIT) != 0u {
            s = shadows::fetch_directional_shadow(i, pbr_input.world_position, pbr_input.world_normal, view_z, pbr_input.frag_coord.xy);
        }
        // the shadow's filtered edge cut at half: a shadow is a shape, not a gradient
        if s >= 0.5 {
            lit = true;
            ndl = max(ndl, d);
        } else if !lit {
            ndl = -1.0;
        }
    }
    // the tones, as an illustrator sorts them: lit; turned from the light (form shadow, `mid`); facing it but shaded
    // by something else (cast shadow, `shadow`). Two bands: lit and one shade (`shadow`) for both. Four and more: the
    // lit side in bands - 2 steps from `mid` to the surface's own colour, by how square it faces the light.
    let n = max(round(paint.bands), 1.0);
    var tint = vec3<f32>(1.0);
    if n > 1.5 && !lit {
        let shaded = ndl < 0.0;
        if n < 2.5 || shaded {
            tint = paint.shadow.rgb;
        } else {
            tint = paint.mid.rgb;
        }
    } else if n > 3.5 && lit {
        let steps = n - 2.0;
        let j = min(steps, 1.0 + floor(clamp(ndl, 0.0, 1.0) * steps));
        tint = mix(paint.mid.rgb, vec3<f32>(1.0), j / steps);
    }
    var rgb = c.rgb * tint + pbr_input.material.emissive.rgb * view_bindings::view.exposure;
    out.color = vec4<f32>(rgb, c.a);
    out.color = main_pass_post_lighting_processing(pbr_input, out.color);
#endif
    return out;
}
