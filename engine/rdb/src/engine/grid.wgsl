// grid.rs: graph paper over the standard material (Bevy's extended-material fragment, its lighting kept)
#import bevy_pbr::{
    pbr_fragment::pbr_input_from_standard_material,
    pbr_functions::alpha_discard,
}

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

struct Grid {
    line: vec4<f32>,
    edge: vec4<f32>,
    cell: f32,
    major: f32,
    line_px: f32,
    edge_px: f32,
    mode: f32,
    toon: f32,
}

@group(#{MATERIAL_BIND_GROUP}) @binding(100)
var<uniform> grid: Grid;

// how much of a line of `px` pixels covers this fragment, for lines at the whole numbers of `coord`
fn lines(coord: vec2<f32>, px: f32) -> f32 {
    let d = abs(fract(coord - 0.5) - 0.5) / max(fwidth(coord), vec2(1e-6));
    let m = min(d.x, d.y);
    return 1.0 - smoothstep(px * 0.5, px * 0.5 + 1.0, m);
}

@fragment
fn fragment(
    in: VertexOutput,
    @builtin(front_facing) is_front: bool,
) -> FragmentOutput {
    var pbr_input = pbr_input_from_standard_material(in, is_front);

    // the face's two axes in the world: the pair it doesn't look along
    let n = abs(normalize(in.world_normal));
    let p = in.world_position.xyz;
    var coord = p.xy;
    if n.x >= n.y && n.x >= n.z {
        coord = p.zy;
    } else if n.y >= n.z {
        coord = p.xz;
    }
#ifdef VERTEX_UVS_A
    // a model: its texture coordinates are where the surface was at rest, in metres
    if grid.mode > 0.5 {
        coord = in.uv;
    }
#endif
    let minor = lines(coord / grid.cell, grid.line_px);
    let major = lines(coord / (grid.cell * grid.major), grid.line_px * 1.5);
    var c = pbr_input.material.base_color.rgb;
    c = mix(c, grid.line.rgb, minor * 0.55 * grid.line.a);
    c = mix(c, grid.line.rgb, major * grid.line.a);
#ifdef VERTEX_UVS_A
    // the face's outline: how far the texture coordinates are from the face's edges, in pixels (meshes only)
    let uv = in.uv;
    let e = min(min(uv.x, 1.0 - uv.x), min(uv.y, 1.0 - uv.y));
    let ep = e / max(fwidth(e), 1e-6);
    let edge = 1.0 - smoothstep(grid.edge_px * 0.5, grid.edge_px * 0.5 + 1.0, ep);
    c = mix(c, grid.edge.rgb, edge * grid.edge.a);
#endif
    pbr_input.material.base_color = vec4(c, pbr_input.material.base_color.a);
    pbr_input.material.base_color = alpha_discard(pbr_input.material, pbr_input.material.base_color);

#ifdef PREPASS_PIPELINE
    let out = deferred_output(in, pbr_input);
#else
    var out: FragmentOutput;
    out.color = apply_pbr_lighting(pbr_input);
    if grid.toon > 0.5 {
        // toon: how lit the surface is (its lit colour against its own), in three flat bands
        let w = vec3(0.2126, 0.7152, 0.0722);
        let lit = dot(out.color.rgb, w) / max(dot(c, w), 1e-4);
        var band = 0.66;
        if lit > 0.72 {
            band = 1.0;
        } else if lit > 0.38 {
            band = 0.84;
        }
        // at the scene's exposure: a lit surface's colour comes out of the light brighter than its own, and tone
        // mapping brings it back, so the bands carry that gain or the body reads greyer than the lit floor
        out.color = vec4(c * band * 1.6, out.color.a);
    }
    out.color = main_pass_post_lighting_processing(pbr_input, out.color);
#endif
    return out;
}
