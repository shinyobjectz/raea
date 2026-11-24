import bevy_pbr::render::pbr_lighting::D_GGX;
import bevy_pbr::render::utils::{rand_vec2f, rand_u, rand_range_u};
import bevy_render::maths::{PI_2, orthonormalize};
import package::scene::bindings::{trace_ray, trace_ray_previous_frame, RAY_T_MIN, RAY_T_MAX, light_sources, directional_lights, environment_map, environment_map_sampler, environment_map_light, LightSource, LIGHT_SOURCE_KIND_DIRECTIONAL, LIGHT_SOURCE_KIND_SKY_PORTAL, sky_portal, resolve_triangle_data_full, ResolvedRayHitFull, MIRROR_ROUGHNESS_THRESHOLD, offset_ray_origin};

enable wgpu_ray_query;

fn power_heuristic(f: f32, g: f32) -> f32 {
    return balance_heuristic(f * f, g * g);
}

fn balance_heuristic(f: f32, g: f32) -> f32 {
    // Need to guard against NaNs since ReSTIR reservoirs can have UCW=0
    if f == 0.0 {
        return 0.0;
    }
    // When NaN, fall back to even weights of 0.5 : 0.5
    let weight = 1.0 / (1.0 + (g / f));
    return select(weight, 0.5, isnan(weight));
}

/// https://gpuopen.com/download/Bounded_VNDF_Sampling_for_Smith-GGX_Reflections.pdf (Listing 1)
/// Result is invalid when output.z <= 0.0, and must be discarded
fn sample_ggx_vndf(wi_tangent: vec3<f32>, roughness: f32, rng: ptr<function, u32>) -> vec3<f32> {
    // Mirror BRDF case
    if roughness <= MIRROR_ROUGHNESS_THRESHOLD {
        return vec3(-wi_tangent.xy, wi_tangent.z);
    }

    let i = wi_tangent;
    let rand = rand_vec2f(rng);
    let i_std = normalize(vec3(i.xy * roughness, i.z));
    let phi = PI_2 * rand.x;
    let a = roughness;
    let s = 1.0 + length(vec2(i.xy));
    let a2 = a * a;
    let s2 = s * s;
    let k = (1.0 - a2) * s2 / (s2 + a2 * i.z * i.z);
    let b = select(i_std.z, k * i_std.z, i.z > 0.0);
    let z = fma(1.0 - rand.y, 1.0 + b, -b);
    let sin_theta = sqrt(saturate(1.0 - z * z));
    let o_std = vec3(sin_theta * cos(phi), sin_theta * sin(phi), z);
    let m_std = i_std + o_std;
    let m = normalize(vec3(m_std.xy * roughness, m_std.z));
    return 2.0 * dot(i, m) * m - i;
}

fn ggx_vndf_sample_invalid(ray_tangent: vec3<f32>) -> bool {
    return !(ray_tangent.z > 0.0);
}

/// https://gpuopen.com/download/Bounded_VNDF_Sampling_for_Smith-GGX_Reflections.pdf (Listing 2)
fn ggx_vndf_pdf(wi_tangent: vec3<f32>, wo_tangent: vec3<f32>, roughness: f32) -> f32 {
    // Mirror BRDF case
    if roughness <= MIRROR_ROUGHNESS_THRESHOLD {
        let mirror_wo = vec3(-wi_tangent.xy, wi_tangent.z);
        if distance(mirror_wo, wo_tangent) < 0.0001 {
            return bitcast<f32>(0x7F800000u); // INF
        } else {
            return 0.0;
        }
    }

    let i = wi_tangent;
    let o = wo_tangent;
    let m = normalize(i + o);
    let ndf = D_GGX(roughness, saturate(m.z));
    let ai = roughness * i.xy;
    let len2 = dot(ai, ai);
    let t = sqrt(len2 + i.z * i.z);
    var pdf: f32;
    if i.z >= 0.0 {
        let a = roughness;
        let s = 1.0 + length(i.xy);
        let a2 = a * a;
        let s2 = s * s;
        let k = (1.0 - a2) * s2 / (s2 + a2 * i.z * i.z);
        pdf = ndf / (2.0 * (k * i.z + t));
    } else {
        pdf = ndf * (t - i.z) / (2.0 * len2);
    }

    return select(pdf, 0.0, isnan(pdf));
}

fn isinf(x: f32) -> bool {
    return (bitcast<u32>(x) & 0x7fffffffu) == 0x7f800000u;
}

fn isnan(x: f32) -> bool {
    return (bitcast<u32>(x) & 0x7fffffffu) > 0x7f800000u;
}

const NULL_LIGHT_ID = 0xFFFFFFFFu;

struct LightSample {
    light_id: u32,
    seed: u32,
}

struct ResolvedLightSample {
    world_position: vec4<f32>,
    world_normal: vec3<f32>,
    radiance: vec3<f32>,
    inverse_pdf: f32,
}

struct LightContribution {
    radiance: vec3<f32>,
    inverse_pdf: f32,
    /// inverse_pdf may not be in solid angle measure (e.g. area measure)
    /// but inverse_solid_angle_pdf is always in solid angle measure
    /// for cases where it's required, e.g. MIS between light sampling techniques
    inverse_solid_angle_pdf: f32,
    wi: vec3<f32>,
    brdf_rays_can_hit: bool,
}

struct GenerateRandomLightSampleResult {
    light_sample: LightSample,
    resolved_light_sample: ResolvedLightSample,
}

fn sample_random_light(ray_origin: vec3<f32>, origin_world_normal: vec3<f32>, rng: ptr<function, u32>) -> LightContribution {
    let sample = generate_random_light_sample(rng);
    var light_contribution = calculate_resolved_light_contribution(sample.resolved_light_sample, ray_origin, origin_world_normal);
    light_contribution.radiance *= trace_visibility(ray_origin, sample.resolved_light_sample.world_position);
    return light_contribution;
}

fn random_emissive_light_pdf(hit: ResolvedRayHitFull, ray_distance: f32, NdotV: f32) -> f32 {
    let light_count = arrayLength(&light_sources);
    let area_pdf = 1.0 / (f32(light_count) * f32(hit.triangle_count) * hit.triangle_area);
    return area_pdf * (ray_distance * ray_distance) / NdotV;
}

// raea: the environment map's light along a direction, its sun disk (EnvironmentSun) showing the sky around it: the
// sun is a directional light that light sampling finds
fn sample_environment_map_light(world_direction: vec3<f32>) -> vec3<f32> {
    return sample_environment_map_light_and_sun(world_direction, false);
}

// raea: the environment map's light along a direction, with its sun where `sun` (a ray that samples no lights: a
// camera ray, or one off a mirror)
fn sample_environment_map_light_and_sun(world_direction: vec3<f32>, sun: bool) -> vec3<f32> {
    if environment_map_light.intensity == 0.0 { return vec3(0.0); }

    var sample_direction = environment_map_light.light_from_world * world_direction;
    if !sun && dot(sample_direction, environment_map_light.sun_direction) > environment_map_light.sun_cos {
        return environment_map_light.sky * environment_map_light.intensity;
    }
    sample_direction.z = -sample_direction.z;

    // Hardcoded to sample mip 0 (unfiltered radiance)
    return textureSampleLevel(environment_map, environment_map_sampler, sample_direction, 0.0).rgb * environment_map_light.intensity;
}

fn generate_random_light_sample(rng: ptr<function, u32>) -> GenerateRandomLightSampleResult {
    let light_count = arrayLength(&light_sources);
    let light_id = rand_range_u(light_count, rng);

    let light_source = light_sources[light_id];

    var triangle_id = 0u;
    // an emissive mesh (raea: an even kind; directional lights and sky portals are odd)
    if (light_source.kind & 1u) == 0u {
        let triangle_count = light_source.kind >> 1u;
        triangle_id = rand_range_u(triangle_count, rng);
    }

    let seed = rand_u(rng);
    let light_sample = LightSample((light_id << 16u) | triangle_id, seed);

    var resolved_light_sample = resolve_light_sample(light_sample, light_source);
    resolved_light_sample.inverse_pdf *= f32(light_count);

    return GenerateRandomLightSampleResult(light_sample, resolved_light_sample);
}

fn resolve_light_sample(light_sample: LightSample, light_source: LightSource) -> ResolvedLightSample {
    // raea: a sky portal: a point on the opening, w 2, its radiance white; the environment map's light through it
    // depends on where it's seen from, so calculate_resolved_light_contribution looks it up
    if light_source.kind == LIGHT_SOURCE_KIND_SKY_PORTAL {
        if light_source.id >= environment_map_light.portal_count {
            return ResolvedLightSample(vec4(0.0, 0.0, 0.0, 2.0), vec3(0.0, 1.0, 0.0), vec3(0.0), 0.0);
        }
        let portal = sky_portal(light_source.id);
        var rng = light_sample.seed;
        let r = rand_vec2f(&rng);
        return ResolvedLightSample(vec4(portal.corner + portal.u * r.x + portal.v * r.y, 2.0), portal.normal, vec3(1.0), portal.area);
    }
    if light_source.kind == LIGHT_SOURCE_KIND_DIRECTIONAL {
        let directional_light = directional_lights[light_source.id];

        var direction_to_light: vec3<f32>;
@if(!NO_DIRECTIONAL_LIGHT_SOFT_SHADOWS) {
        // Sample a random direction within a cone whose base is the sun approximated as a disk
        // https://www.realtimerendering.com/raytracinggems/unofficial_RayTracingGems_v1.9.pdf#0004286901.INDD%3ASec30%3A305
        var rng = light_sample.seed;
        let random = rand_vec2f(&rng);
        let cos_theta = (1.0 - random.x) + random.x * directional_light.cos_theta_max;
        let sin_theta = sqrt(1.0 - cos_theta * cos_theta);
        let phi = random.y * PI_2;
        let x = cos(phi) * sin_theta;
        let y = sin(phi) * sin_theta;
        direction_to_light = vec3(x, y, cos_theta);

        // Rotate the ray so that the cone it was sampled from is aligned with the light direction
        direction_to_light = orthonormalize(directional_light.direction_to_light) * direction_to_light;
} @else {
        direction_to_light = directional_light.direction_to_light;
}

        return ResolvedLightSample(
        vec4(direction_to_light, 0.0),
        -direction_to_light,
        directional_light.luminance,
        directional_light.inverse_pdf,
    );
} else {
        let triangle_count = light_source.kind >> 1u;
        let triangle_id = light_sample.light_id & 0xFFFFu;
        let barycentrics = triangle_barycentrics(light_sample.seed);
        let triangle_data = resolve_triangle_data_full(light_source.id, triangle_id, barycentrics);

        return ResolvedLightSample(
            vec4(triangle_data.world_position, 1.0),
            triangle_data.world_normal,
            triangle_data.material.emissive.rgb,
            f32(triangle_count) * triangle_data.triangle_area,
        );
    }
}

fn calculate_resolved_light_contribution(resolved_light_sample: ResolvedLightSample, ray_origin: vec3<f32>, origin_world_normal: vec3<f32>) -> LightContribution {
    // raea: a sky portal's point (w 2) is placed as an emissive's is, lit by the environment map along the way through
    // it, from either side
    let portal = resolved_light_sample.world_position.w == 2.0;
    let w = min(resolved_light_sample.world_position.w, 1.0);
    let ray = resolved_light_sample.world_position.xyz - (w * ray_origin);
    let light_distance = length(ray);
    let wi = ray / light_distance;

    var cos_theta_light = saturate(dot(-wi, resolved_light_sample.world_normal));
    var emitted = resolved_light_sample.radiance;
    if portal {
        cos_theta_light = abs(dot(wi, resolved_light_sample.world_normal));
        emitted *= sample_environment_map_light(wi);
    }
    let light_distance_squared = light_distance * light_distance;
    let denominator = cos_theta_light / light_distance_squared;

    let radiance = emitted * denominator;
    let inverse_solid_angle_pdf = resolved_light_sample.inverse_pdf * denominator;

    return LightContribution(radiance, resolved_light_sample.inverse_pdf, inverse_solid_angle_pdf, wi, w == 1.0);
}

// raea: the pdf light sampling (generate_random_light_sample) has for the direction wi from origin, where wi leaves
// through a sky portal (solid angle); 0 where it crosses none
fn sky_portal_light_pdf(origin: vec3<f32>, wi: vec3<f32>) -> f32 {
    for (var i = 0u; i < environment_map_light.portal_count; i++) {
        let portal = sky_portal(i);
        let cos_portal = dot(wi, portal.normal);
        if abs(cos_portal) < 1e-6 { continue; }
        let t = dot(portal.corner - origin, portal.normal) / cos_portal;
        if t <= 0.0 { continue; }
        // where it crosses, in the portal's (u, v)
        let h = origin + wi * t - portal.corner;
        let uu = dot(portal.u, portal.u);
        let uv = dot(portal.u, portal.v);
        let vv = dot(portal.v, portal.v);
        let det = uu * vv - uv * uv;
        let a = (dot(h, portal.u) * vv - dot(h, portal.v) * uv) / det;
        let b = (dot(h, portal.v) * uu - dot(h, portal.u) * uv) / det;
        if a < 0.0 || a > 1.0 || b < 0.0 || b > 1.0 { continue; }
        return t * t / (f32(arrayLength(&light_sources)) * portal.area * abs(cos_portal));
    }
    return 0.0;
}

// raea: what a ray that escaped to the sky along wi keeps of the environment map's light, where it left through a sky
// portal that light sampling also finds that light through: the power heuristic against light sampling's pdf, scaled
// by nee_scale (the candidates the ray's vertex drew, times the chance it drew any). A ray drawn with no pdf (bounce_pdf
// 0: a camera ray) or along a mirror's delta lobe (infinite) keeps all of it, as does one that crossed no portal.
fn sky_portal_mis_weight(origin: vec3<f32>, wi: vec3<f32>, bounce_pdf: f32, nee_scale: f32) -> f32 {
    if bounce_pdf <= 0.0 || isinf(bounce_pdf) || environment_map_light.portal_count == 0u { return 1.0; }
    let p_light = sky_portal_light_pdf(origin, wi);
    if p_light <= 0.0 { return 1.0; }
    return power_heuristic(bounce_pdf, p_light * nee_scale);
}

// raea: where a visibility ray from a surface at p toward a light sample's point starts (w 0: the point is a direction),
// off the side the light comes in by: the far side for light let through (diffuse transmission)
fn ray_origin_toward_sample(p: vec3<f32>, normal: vec3<f32>, point: vec4<f32>) -> vec3<f32> {
    return offset_ray_origin(p, normal, normal, select(point.xyz - p, point.xyz, point.w == 0.0));
}

fn trace_visibility(ray_origin: vec3<f32>, point: vec4<f32>) -> f32 {
    var ray_direction = point.xyz;
    var ray_t_max = RAY_T_MAX;

    if point.w == 1.0 {
        let ray = ray_direction - ray_origin;
        let dist = length(ray);
        ray_direction = ray / dist;
        ray_t_max = dist - RAY_T_MIN;
    } else if point.w == 2.0 {
        // raea: a sky portal's point: through it, out to the sky
        ray_direction = normalize(point.xyz - ray_origin);
    }

    if ray_t_max < RAY_T_MIN { return 0.0; }

    let ray_hit = trace_ray(ray_origin, ray_direction, RAY_T_MIN, ray_t_max, RAY_FLAG_TERMINATE_ON_FIRST_HIT);
    return f32(ray_hit.kind == RAY_QUERY_INTERSECTION_NONE);
}

fn trace_visibility_previous_frame(ray_origin: vec3<f32>, point: vec4<f32>) -> f32 {
    var ray_direction = point.xyz;
    var ray_t_max = RAY_T_MAX;

    if point.w == 1.0 {
        let ray = ray_direction - ray_origin;
        let dist = length(ray);
        ray_direction = ray / dist;
        ray_t_max = dist - RAY_T_MIN;
    } else if point.w == 2.0 {
        // raea: a sky portal's point: through it, out to the sky
        ray_direction = normalize(point.xyz - ray_origin);
    }

    if ray_t_max < RAY_T_MIN { return 0.0; }

    let ray_hit = trace_ray_previous_frame(ray_origin, ray_direction, RAY_T_MIN, ray_t_max, RAY_FLAG_TERMINATE_ON_FIRST_HIT);
    return f32(ray_hit.kind == RAY_QUERY_INTERSECTION_NONE);
}

/// https://www.realtimerendering.com/raytracinggems/unofficial_RayTracingGems_v1.9.pdf#0004286901.INDD%3ASec22%3A297
fn triangle_barycentrics(seed: u32) -> vec3<f32> {
    var rng = seed;
    var barycentrics = rand_vec2f(&rng);
    if barycentrics.x + barycentrics.y > 1.0 { barycentrics = 1.0 - barycentrics; }
    return vec3(1.0 - barycentrics.x - barycentrics.y, barycentrics);
}
