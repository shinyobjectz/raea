// raea's toon surface (MeshToonMaterial in @raea/motion/3d; engine/rdb/src/engine/toon.rs): the standard material's inputs
// (base colour and its texture, emissive, opacity, fog), lit in hard bands instead of physically.
//
// The band a point falls in comes from r, the share of the light reaching it that it faces:
//   r = (sum over lights of max(N.L, 0) x shadow x p + ambient) / (sum over lights of p + ambient)
// where p is a light's strength there (its intensity, a point or spot light's distance falloff; not N.L, shadows or a
// spot's cone) in the SDK's units: DirectionalLight intensity 1 = 8000 lux, AmbientLight 1 = 400 cd/m2
// (sdk/react/src/motion/scene3d.rs LUX and AMBIENT). r is 1 where a point faces every light and 0 where it faces none
// and there's no ambient. Ambient raises r everywhere (it flattens the bands); a brighter key light alone doesn't
// move them. Band k shows its colour: the bottom band the shadow colour, the top band the base colour, those
// between mixed in Oklab, unless the material gives each band's colour. The colour is not multiplied by the light's
// brightness (as unlit, so the hex shows as written before tone mapping), only tinted by its hue.
//
// Edges are hard but anti-aliased over one pixel (fwidth), so a moving terminator doesn't crawl.

#import bevy_pbr::{
    pbr_fragment::pbr_input_from_standard_material,
    pbr_functions::alpha_discard,
    mesh_view_bindings as view_bindings,
    mesh_view_types,
    clustered_forward as clustering,
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

struct Toon {
    // linear rgb; a = 1 when given, 0 when derived from the base colour
    shadow: vec4<f32>,
    // linear rgb, a = intensity (0 = none)
    rim: vec4<f32>,
    // linear rgb, a = intensity (0 = none)
    spec: vec4<f32>,
    // x = band count (1..8), y = rim width (0..1 of the Fresnel range), z = the highlight's cosine threshold
    shape: vec4<f32>,
    // where bands 1..7 start, in r (band 0 starts at 0)
    starts: array<vec4<f32>, 2>,
    // each band's colour, linear rgb; a = 1 when given, 0 when mixed from the shadow and base colours
    colors: array<vec4<f32>, 8>,
}

@group(#{MATERIAL_BIND_GROUP}) @binding(100) var<uniform> toon: Toon;

const LUX: f32 = 8000.0;
const AMBIENT: f32 = 400.0;
const LUMA: vec3<f32> = vec3<f32>(0.2126, 0.7152, 0.0722);

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

fn linear_from_oklab(c: vec3<f32>) -> vec3<f32> {
    let l_ = c.x + 0.3963377774 * c.y + 0.2158037573 * c.z;
    let m_ = c.x - 0.1055613458 * c.y - 0.0638541728 * c.z;
    let s_ = c.x - 0.0894841775 * c.y - 1.2914855480 * c.z;
    let l = l_ * l_ * l_;
    let m = m_ * m_ * m_;
    let s = s_ * s_ * s_;
    return max(vec3<f32>(
        4.0767416621 * l - 3.3077115913 * m + 0.2309699292 * s,
        -1.2684380046 * l + 2.6097574011 * m - 0.3413193965 * s,
        -0.0041960863 * l - 0.7034186147 * m + 1.7076147010 * s,
    ), vec3<f32>(0.0));
}

// The default shadow colour: the base colour darker (Oklab lightness x 0.62), its chroma kept and its hue turned 14
// degrees towards violet, so a yellow's shadow is a warm ochre and a blue's a deeper indigo, never a grey.
fn derived_shadow(base: vec3<f32>) -> vec3<f32> {
    let lab = oklab(base);
    let c = length(lab.yz);
    var h = atan2(lab.z, lab.y);
    let violet = radians(300.0);
    var d = violet - h;
    d = d - 6.2831853 * round(d / 6.2831853);
    h = h + sign(d) * min(abs(d), radians(14.0));
    return linear_from_oklab(vec3<f32>(lab.x * 0.62, c * cos(h), c * sin(h)));
}

fn start_of(k: u32) -> f32 {
    return toon.starts[k / 4u][k % 4u];
}

// 0 below the edge, 1 above, over about one pixel of x's change
fn edge(e: f32, x: f32, w: f32) -> f32 {
    return clamp((x - e) / max(w, 1e-5) + 0.5, 0.0, 1.0);
}

struct Light {
    // the faced, shadowed light (numerator of r) and all the light that reaches (its denominator)
    faced: f32,
    total: f32,
    // the faced light's colour, for the tint
    rgb: vec3<f32>,
    // the highlight: N.H for the light that reaches the point most strongly (continuous across the terminator, so its
    // screen-space change stays small), and whether that light falls on it (facing it, not shadowed)
    nh: f32,
    nh_seen: f32,
    best: f32,
}

fn gather(N: vec3<f32>, V: vec3<f32>, P: vec4<f32>, world_normal: vec3<f32>, frag_coord: vec4<f32>, flags: u32, is_orthographic: bool, occlusion: vec3<f32>) -> Light {
    var out: Light;
    out.faced = 0.0;
    out.total = 0.0;
    out.rgb = vec3<f32>(0.0);
    out.nh = -1.0;
    out.nh_seen = 0.0;
    out.best = 0.0;
    let receives = (flags & MESH_FLAGS_SHADOW_RECEIVER_BIT) != 0u;

    let view_z = dot(vec4<f32>(
        view_bindings::view.view_from_world[0].z,
        view_bindings::view.view_from_world[1].z,
        view_bindings::view.view_from_world[2].z,
        view_bindings::view.view_from_world[3].z
    ), P);

    // directional lights
    let n_dir = view_bindings::lights.n_directional_lights;
    for (var i: u32 = 0u; i < n_dir; i = i + 1u) {
        let light = &view_bindings::lights.directional_lights[i];
        let L = (*light).direction_to_light.xyz;
        let p = dot((*light).color.rgb, LUMA) / LUX;
        var shadow = 1.0;
        if receives && ((*light).flags & mesh_view_types::DIRECTIONAL_LIGHT_FLAGS_SHADOWS_ENABLED_BIT) != 0u {
            shadow = shadows::fetch_directional_shadow(i, P, world_normal, view_z, frag_coord.xy);
        }
        let ndl = max(dot(N, L), 0.0) * shadow;
        out.faced += ndl * p;
        out.total += p;
        out.rgb += ndl * (*light).color.rgb / LUX;
        if p > out.best {
            out.best = p;
            out.nh = dot(N, normalize(L + V));
            out.nh_seen = select(0.0, 1.0, ndl > 0.0);
        }
    }

    // point and spot lights, from the fragment's cluster
    let cluster_index = clustering::view_fragment_cluster_index(frag_coord.xy, view_z, is_orthographic);
    let ranges = clustering::unpack_clusterable_object_index_ranges(cluster_index);
    for (var i: u32 = ranges.first_point_light_index_offset; i < ranges.first_reflection_probe_index_offset; i = i + 1u) {
        let id = clustering::get_clusterable_object_id(i);
        let light = &view_bindings::clustered_lights.data[id];
        let to_light = (*light).position_radius.xyz - P.xyz;
        let d2 = dot(to_light, to_light);
        let L = normalize(to_light);
        // range window and inverse square, as Bevy's getDistanceAttenuation
        let f = d2 * (*light).color_inverse_square_range.w;
        let window = saturate(1.0 - f * f);
        let falloff = window * window / max(d2, 0.0001);
        // colour_inverse_square_range.rgb is colour x lumens / 4pi
        let rgb = (*light).color_inverse_square_range.rgb * falloff;
        let p = dot(rgb, LUMA) / LUX;
        var cone = 1.0;
        var shadow = 1.0;
        let is_spot = i >= ranges.first_spot_light_index_offset;
        if is_spot {
            var spot_dir = vec3<f32>((*light).light_custom_data.x, 0.0, (*light).light_custom_data.y);
            spot_dir.y = sqrt(max(0.0, 1.0 - spot_dir.x * spot_dir.x - spot_dir.z * spot_dir.z));
            if ((*light).flags & mesh_view_types::POINT_LIGHT_FLAGS_SPOT_LIGHT_Y_NEGATIVE) != 0u {
                spot_dir.y = -spot_dir.y;
            }
            let a = saturate(dot(-spot_dir, L) * (*light).light_custom_data.z + (*light).light_custom_data.w);
            cone = a * a;
            if receives && ((*light).flags & mesh_view_types::POINT_LIGHT_FLAGS_SHADOWS_ENABLED_BIT) != 0u {
                shadow = shadows::fetch_spot_shadow(id, P, world_normal, (*light).shadow_map_near_z, frag_coord.xy);
            }
        } else if receives && ((*light).flags & mesh_view_types::POINT_LIGHT_FLAGS_SHADOWS_ENABLED_BIT) != 0u {
            shadow = shadows::fetch_point_shadow(id, P, world_normal, frag_coord.xy);
        }
        let ndl = max(dot(N, L), 0.0) * shadow * cone;
        out.faced += ndl * p;
        out.total += p;
        out.rgb += ndl * rgb / LUX;
        if p > out.best {
            out.best = p;
            out.nh = dot(N, normalize(L + V));
            out.nh_seen = select(0.0, 1.0, ndl > 0.0);
        }
    }

    // ambient occlusion (AmbientOcclusion's SSAO, 1 without it) takes ambient from creases: their r drops, so they
    // fall into a darker band rather than shading smoothly
    let ambient = view_bindings::lights.ambient_color.rgb / AMBIENT;
    let a = dot(ambient, LUMA);
    out.faced += a * dot(occlusion, LUMA);
    out.total += a;
    out.rgb += ambient * occlusion;
    return out;
}

@fragment
fn fragment(in: VertexOutput, @builtin(front_facing) is_front: bool) -> FragmentOutput {
    var pbr_input = pbr_input_from_standard_material(in, is_front);
    pbr_input.material.base_color = alpha_discard(pbr_input.material, pbr_input.material.base_color);

#ifdef PREPASS_PIPELINE
    let out = deferred_output(in, pbr_input);
#else
    let N = pbr_input.N;
    let V = pbr_input.V;
    let light = gather(N, V, pbr_input.world_position, pbr_input.world_normal, pbr_input.frag_coord, pbr_input.flags, pbr_input.is_orthographic, pbr_input.diffuse_occlusion);
    let r = select(1.0, light.faced / light.total, light.total > 1e-6);
    let fresnel = 1.0 - saturate(dot(N, V));
    // derivatives in uniform control flow, once
    let wr = fwidth(r);
    let wf = fwidth(fresnel);
    let wh = fwidth(light.nh);

    let base = pbr_input.material.base_color.rgb;
    var shadow = toon.shadow.rgb;
    if toon.shadow.a < 0.5 {
        shadow = derived_shadow(base);
    }
    let lab_shadow = oklab(shadow);
    let lab_base = oklab(base);
    let n = u32(toon.shape.x);
    // the band colours, blended across each band edge by its coverage
    var color = select(linear_from_oklab(lab_shadow), toon.colors[0].rgb, toon.colors[0].a > 0.5);
    for (var k: u32 = 1u; k < 8u; k = k + 1u) {
        if k >= n {
            break;
        }
        let t = f32(k) / f32(max(n - 1u, 1u));
        let ck = select(linear_from_oklab(mix(lab_shadow, lab_base, t)), toon.colors[k].rgb, toon.colors[k].a > 0.5);
        color = mix(color, ck, edge(start_of(k), r, wr));
    }

    // the light's hue (its brightest channel at 1): white light leaves the colours as they are
    let hue = light.rgb / max(max(light.rgb.r, max(light.rgb.g, light.rgb.b)), 1e-4);
    color *= select(vec3<f32>(1.0), hue, dot(light.rgb, vec3<f32>(1.0)) > 1e-4);

    // a crisp highlight where the half-vector meets the normal, then the rim
    if toon.spec.a > 0.0 {
        color = mix(color, toon.spec.rgb, toon.spec.a * light.nh_seen * edge(toon.shape.z, light.nh, wh));
    }
    if toon.rim.a > 0.0 {
        color = mix(color, toon.rim.rgb, toon.rim.a * edge(1.0 - toon.shape.y, fresnel, wf));
    }

    color += pbr_input.material.emissive.rgb;
    var out: FragmentOutput;
    out.color = vec4<f32>(color, pbr_input.material.base_color.a);
    out.color = main_pass_post_lighting_processing(pbr_input, out.color);
#endif
    return out;
}
