// The froxel grid's light pass (froxel.rs): each texel of the atlas is a cell of the grid (the slices laid out as
// tiles), and gets the medium there (every fog volume holding it, summed) and the light it scatters toward the eye per
// metre, as rgb (scattered light) and a (extinction). The lights are Bevy's, sampled as Bevy's march samples them
// (bevy_pbr 0.19.1 volumetric_fog.wgsl, MIT/Apache-2.0: the cascade lookup, the clustered point and spot lights and
// their shadow fetches are its), at `samples` points spread along the cell's depth, and the light of every <Fire>.

#import bevy_core_pipeline::fullscreen_vertex_shader::FullscreenVertexOutput
#import bevy_pbr::mesh_view_bindings::{lights, view, clustered_lights}
#import bevy_pbr::mesh_view_types::{
    DIRECTIONAL_LIGHT_FLAGS_VOLUMETRIC_BIT,
    POINT_LIGHT_FLAGS_SHADOWS_ENABLED_BIT,
    POINT_LIGHT_FLAGS_VOLUMETRIC_BIT,
    POINT_LIGHT_FLAGS_SPOT_LIGHT_Y_NEGATIVE,
}
#import bevy_pbr::shadow_sampling::{sample_shadow_map_hardware, sample_shadow_cubemap, sample_shadow_map, SPOT_SHADOW_TEXEL_SIZE}
#import bevy_pbr::shadows::{get_cascade_index, world_to_directional_light_local}
#import bevy_pbr::view_transformations::{frag_coord_to_ndc, position_ndc_to_view}
#import bevy_render::maths::orthonormalize
#import bevy_pbr::clustered_forward as clustering
#import bevy_pbr::lighting::getDistanceAttenuation

struct Params {
    view_from_clip: mat4x4<f32>,
    grid: vec4<u32>,
    slab: vec4<f32>,
    viewport: vec4<f32>,
    ambient: vec4<f32>,
    counts: vec4<u32>,
    heroes: vec4<u32>,
}

struct Volume {
    local_from_world: mat4x4<f32>,
    scatter: vec4<f32>,
    ambient: vec4<f32>,
    shape: vec4<f32>,
}

struct GridFire {
    center_time: vec4<f32>,
    size_intensity: vec4<f32>,
    color_seed: vec4<f32>,
    speed: vec4<f32>,
}

@group(1) @binding(0) var<uniform> params: Params;
@group(1) @binding(1) var<storage, read> volumes: array<Volume>;
@group(1) @binding(2) var<storage, read> fires: array<GridFire>;
// the frame's depth, for the reference (only)
#ifdef MULTISAMPLED
@group(1) @binding(3) var depth_texture: texture_depth_multisampled_2d;
#else
@group(1) @binding(3) var depth_texture: texture_depth_2d;
#endif

const FRAC_4_PI: f32 = 0.07957747154594767;
// the volumes a point can be in at once that directional lights are phased for
const HELD: u32 = 8u;
const DIRECTIONAL: u32 = 4u;

fn henyey_greenstein(g: f32, neg_LdotV: f32) -> f32 {
    let denom = 1.0 + g * g - 2.0 * g * neg_LdotV;
    return FRAC_4_PI * (1.0 - g * g) / (denom * sqrt(denom));
}

// the medium at a point of the view (every volume holding it, summed) and the light it scatters toward the eye per
// metre: rgb scattered, a extinction. `pixel` is the frame's pixel the point is seen through (its light cluster)
fn medium_light(P_view: vec3<f32>, pixel: vec2<f32>) -> vec4<f32> {
    let P_world = (view.world_from_view * vec4(P_view, 1.0)).xyz;
    let V = normalize(P_world - view.world_position);
    let is_orthographic = view.clip_from_view[3].w == 1.0;
    let exposure = view.exposure;

    // the medium: every volume holding the point
    var sigma_t = 0.0;
    var k = vec3(0.0);
    var amb = vec3(0.0);
    var held: array<u32, HELD>;
    var weight: array<f32, HELD>;
    var count = 0u;
    for (var v = 0u; v < params.counts.x; v += 1u) {
        let vol = &volumes[v];
        let local = ((*vol).local_from_world * vec4(P_world, 1.0)).xyz;
        if (any(abs(local) > vec3(0.5))) {
            continue;
        }
        // thinning going up from the box's floor
        let w = exp(-(*vol).shape.x * (local.y + 0.5) * (*vol).shape.y);
        sigma_t += (*vol).scatter.a * w;
        k += (*vol).scatter.rgb * w;
        amb += (*vol).ambient.rgb * w;
        if (count < HELD) {
            held[count] = v;
            weight[count] = w;
            count += 1u;
        }
    }
    if (count == 0u) {
        return vec4(0.0, 0.0, 0.0, sigma_t);
    }
    var s = amb * params.ambient.rgb;

    // directional lights, with the phase of each volume they're in (the volumetric ones, but the first heroes.z of
    // them: froxel_hero.wgsl's)
    var suns = 0u;
    for (var li = 0u; li < min(lights.n_directional_lights, DIRECTIONAL); li += 1u) {
        let light = &lights.directional_lights[li];
        if (((*light).flags & DIRECTIONAL_LIGHT_FLAGS_VOLUMETRIC_BIT) == 0u) {
            continue;
        }
        suns += 1u;
        if (suns <= params.heroes.z) {
            continue;
        }
        let L = (*light).direction_to_light.xyz;
        let depth_offset = (*light).shadow_depth_bias * L;
        let cascade_index = get_cascade_index(li, P_view.z);
        let light_local = world_to_directional_light_local(li, cascade_index, vec4(P_world + depth_offset, 1.0));
        var lit = f32(light_local.w != 0.0);
        if (lit != 0.0) {
            let array_index = i32((*light).depth_texture_base_index + cascade_index);
            lit = sample_shadow_map_hardware(light_local.xy, light_local.z, array_index);
        }
        if (lit == 0.0) {
            continue;
        }
        let neg_LdotV = dot(normalize(L), V);
        for (var h = 0u; h < count; h += 1u) {
            let vol = &volumes[held[h]];
            s += (*light).color.rgb * henyey_greenstein((*vol).ambient.a, neg_LdotV) * (*vol).scatter.rgb * weight[h] * lit * exposure;
        }
    }

    // point and spot lights (no phase, as Bevy's march)
    var ps = vec3(0.0);
    let cluster_index = clustering::view_fragment_cluster_index(pixel, P_view.z, is_orthographic);
    let ranges = clustering::unpack_clusterable_object_index_ranges(cluster_index);
    for (var i: u32 = ranges.first_point_light_index_offset; i < ranges.first_reflection_probe_index_offset; i = i + 1u) {
        let light_id = clustering::get_clusterable_object_id(i);
        let light = &clustered_lights.data[light_id];
        if (((*light).flags & POINT_LIGHT_FLAGS_VOLUMETRIC_BIT) == 0u || light_id == params.heroes.x || light_id == params.heroes.y) {
            continue;
        }
        let light_to_frag = (*light).position_radius.xyz - P_world;
        let distance_square = dot(light_to_frag, light_to_frag);
        var atten = getDistanceAttenuation(distance_square, (*light).color_inverse_square_range.w);
        if (i < ranges.first_spot_light_index_offset) {
            if (((*light).flags & POINT_LIGHT_FLAGS_SHADOWS_ENABLED_BIT) != 0u) {
                atten *= fetch_point_shadow_without_normal(light_id, vec4(P_world, 1.0), pixel);
            }
        } else {
            var spot_dir = vec3<f32>((*light).light_custom_data.x, 0.0, (*light).light_custom_data.y);
            spot_dir.y = sqrt(max(0.0, 1.0 - spot_dir.x * spot_dir.x - spot_dir.z * spot_dir.z));
            if (((*light).flags & POINT_LIGHT_FLAGS_SPOT_LIGHT_Y_NEGATIVE) != 0u) {
                spot_dir.y = -spot_dir.y;
            }
            let cd = dot(-spot_dir, normalize(light_to_frag));
            let spot = saturate(cd * (*light).light_custom_data.z + (*light).light_custom_data.w);
            atten *= spot * spot;
            if (atten > 0.0 && ((*light).flags & POINT_LIGHT_FLAGS_SHADOWS_ENABLED_BIT) != 0u) {
                atten *= fetch_spot_shadow_without_normal(light_id, vec4(P_world, 1.0), pixel);
            }
        }
        ps += (*light).color_inverse_square_range.rgb * atten;
    }
    s += k * ps * exposure;

    // fires: each flame lights the fog around it, a glowing body about half its box's front area, flickering as
    // the SDK's Fire flickers its PointLight. Not inside its box (its material draws the flame), and like the
    // flame's own light not scaled by the camera's exposure
    var fire_light = vec3(0.0);
    for (var f = 0u; f < params.counts.z; f += 1u) {
        let fire = &fires[f];
        let size = (*fire).size_intensity.xyz;
        let d = P_world - (*fire).center_time.xyz;
        if (all(abs(d) < size * 0.5)) {
            continue;
        }
        let t = (*fire).center_time.w;
        let seed = (*fire).color_seed.w * 1.37;
        let sp = (*fire).speed.x / sqrt(max(size.y, 0.05));
        let flicker = 0.82 + 0.1 * sin(t * 11.0 * sp + seed) + 0.06 * sin(t * 17.3 * sp + 2.1 * seed) + 0.04 * sin(t * 5.1 * sp + 0.7 * seed);
        let r2 = max(dot(d, d), dot(size, size) * 0.25);
        fire_light += (*fire).color_seed.rgb * (*fire).size_intensity.w * flicker * (0.5 * size.x * size.y) / r2;
    }
    s += k * fire_light;
    return vec4(s, sigma_t);
}

@fragment
fn light(in: FullscreenVertexOutput) -> @location(0) vec4<f32> {
    let gx = params.grid.x;
    let gy = params.grid.y;
    let gz = params.grid.z;
    let cols = params.grid.w;
    let a = vec2<u32>(in.position.xy);
    let slice = (a.y / gy) * cols + a.x / gx;
    if (slice >= gz) {
        return vec4(0.0);
    }
    let cell = vec2<f32>(vec2(a.x % gx, a.y % gy));
    let tile = params.slab.x;
    let near = params.slab.y;
    let log_ratio = params.slab.w;
    // the cell's centre in the frame, and the view ray through it (a point on the near plane: reverse Z, so NDC z 1)
    let pixel = min(params.viewport.xy + (cell + 0.5) * tile, params.viewport.xy + params.viewport.zw - 0.5);
    let ndc = frag_coord_to_ndc(vec4(pixel, 1.0, 1.0));
    let q = position_ndc_to_view(vec3(ndc.xy, 1.0));
    let n = params.counts.y;

    var sum = vec4(0.0);
    for (var j = 0u; j < n; j += 1u) {
        // a point at this sample's depth in the cell, evenly spaced in log depth
        let depth = near * exp(log_ratio * (f32(slice) + (f32(j) + 0.5) / f32(n)) / f32(gz));
        sum += medium_light(q * (depth / -q.z), pixel);
    }
    return sum / f32(n);
}

// The ground truth (RAEA_FOG_REFERENCE=grid, sdk/motion/bench/bevy/fog-score.py): the same medium and lights as the grid's
// cells, but marched per pixel with no grid and no hero sampling, `counts.w` steps evenly in view depth from the
// camera (or the volumes' near depth) to the surface, each a point at the step's middle integrated as the grid
// integrates a cell (Hillaire's step). Blended onto the frame as the apply is.
@fragment
fn reference(in: FullscreenVertexOutput) -> @location(0) vec4<f32> {
    let pixel = in.position.xy;
    let ndc_depth = textureLoad(depth_texture, vec2<i32>(pixel), 0);
    var far = params.slab.z;
    if (ndc_depth > 0.0) {
        let p = params.view_from_clip * vec4(0.0, 0.0, ndc_depth, 1.0);
        far = min(-p.z / p.w, far);
    }
    // the grid's near depth is held at a quarter metre or more; nearer than that the fog may start at the camera
    let near = select(params.slab.y, 0.0, params.slab.y <= 0.26);
    let q = position_ndc_to_view(vec3(frag_coord_to_ndc(vec4(pixel, 1.0, 1.0)).xy, 1.0));
    let stretch = length(q) / -q.z;
    let n = params.counts.w;
    let step = max(far - near, 0.0) / f32(n);
    var light = vec3(0.0);
    var shows = 1.0;
    for (var j = 0u; j < n; j += 1u) {
        let depth = near + (f32(j) + 0.5) * step;
        let m = medium_light(q * (depth / -q.z), pixel);
        let len = step * stretch;
        let through = exp(-m.a * len);
        if (m.a > 1e-7) {
            light += shows * m.rgb * (1.0 - through) / m.a;
        } else {
            light += shows * m.rgb * len;
        }
        shows *= through;
    }
    return vec4(light, shows);
}

// Bevy's (volumetric_fog.wgsl)
fn fetch_point_shadow_without_normal(light_id: u32, frag_position: vec4<f32>, frag_coord_xy: vec2<f32>) -> f32 {
    let light = &clustered_lights.data[light_id];
    let surface_to_light = (*light).position_radius.xyz - frag_position.xyz;
    let surface_to_light_abs = abs(surface_to_light);
    let distance_to_light = max(surface_to_light_abs.x, max(surface_to_light_abs.y, surface_to_light_abs.z));
    let depth_offset = (*light).shadow_depth_bias * normalize(surface_to_light.xyz);
    let offset_position = frag_position.xyz + depth_offset;
    let frag_ls = offset_position.xyz - (*light).position_radius.xyz;
    let abs_position_ls = abs(frag_ls);
    let major_axis_magnitude = max(abs_position_ls.x, max(abs_position_ls.y, abs_position_ls.z));
    let zw = -major_axis_magnitude * (*light).light_custom_data.xy + (*light).light_custom_data.zw;
    let depth = zw.x / zw.y;
    let flip_z = vec3(1.0, 1.0, -1.0);
    return sample_shadow_cubemap(frag_ls * flip_z, distance_to_light, depth, light_id, frag_coord_xy);
}

// Bevy's (volumetric_fog.wgsl)
fn fetch_spot_shadow_without_normal(light_id: u32, frag_position: vec4<f32>, frag_coord_xy: vec2<f32>) -> f32 {
    let light = &clustered_lights.data[light_id];
    let surface_to_light = (*light).position_radius.xyz - frag_position.xyz;
    var spot_dir = vec3<f32>((*light).light_custom_data.x, 0.0, (*light).light_custom_data.y);
    spot_dir.y = sqrt(max(0.0, 1.0 - spot_dir.x * spot_dir.x - spot_dir.z * spot_dir.z));
    if (((*light).flags & POINT_LIGHT_FLAGS_SPOT_LIGHT_Y_NEGATIVE) != 0u) {
        spot_dir.y = -spot_dir.y;
    }
    let fwd = -spot_dir;
    let offset_position = -surface_to_light + ((*light).shadow_depth_bias * normalize(surface_to_light));
    let light_inv_rot = orthonormalize(fwd);
    let projected_position = offset_position * light_inv_rot;
    let f_div_minus_z = 1.0 / ((*light).spot_light_tan_angle * -projected_position.z);
    let shadow_xy_ndc = projected_position.xy * f_div_minus_z;
    let shadow_uv = shadow_xy_ndc * vec2<f32>(0.5, -0.5) + vec2<f32>(0.5, 0.5);
    let depth = 0.1 / -projected_position.z;
    return sample_shadow_map(shadow_uv, depth, i32(light_id) + lights.spot_light_shadowmap_offset, frag_coord_xy, SPOT_SHADOW_TEXEL_SIZE);
}
