import bevy_core_pipeline::tonemapping::tonemapping_luminance as luminance;
import bevy_pbr::render::utils::{rand_f, rand_range_u};
import bevy_render::maths::PI;
import bevy_render::utils::octahedral_encode;
import package::scene::brdf::{brdf_pdf, evaluate_and_sample_brdf, evaluate_brdf, EvaluateAndSampleBrdfResult, F_AB};
import package::realtime::presample_light_tiles::unpack_resolved_light_sample;
import package::realtime::bindings::{empty_reservoir, light_tile_resolved_samples, light_tile_samples, Reservoir, constants, view};
import package::scene::sampling::{calculate_resolved_light_contribution, isinf, LightSample, NULL_LIGHT_ID, power_heuristic, trace_visibility, sample_environment_map_light_and_sun, sky_portal_mis_weight};
import package::scene::bindings::{face_hit, light_sources, MIRROR_ROUGHNESS_THRESHOLD, RAY_T_MAX, RAY_T_MIN, resolve_ray_hit_full, ResolvedMaterial, ResolvedRayHitFull, trace_ray};
import package::realtime::world_cache_query::{get_cell_size, query_world_cache, query_world_cache_found, WORLD_CACHE_CELL_LIFETIME};

@if(DLSS_RR_GUIDE_BUFFERS)
import package::realtime::resolve_dlss_rr_textures::{is_delta_mirror, primary_surface_glossy, psr_bounce_reflectance, psr_chain_mirror, psr_environment_miss, psr_init, replace_primary_surface};

enable wgpu_ray_query;

const RECONNECTION_FOOTPRINT_KAPPA = 0.02;
const RECONNECTION_ROUGHNESS_MIN = 0.6;
const RECONNECTION_RELAX_DISTANCE = 1.0;

const CACHE_TERMINATION_MIN_SOLID_ANGLE = PI;
// raea: the least chance a glossy surface's path samples a light directly (NEE), so a small bright light shows in it
const GLOSSY_NEE_FLOOR = 0.25;

/// What tracing one path produced.
///
/// `radiance` is shaded straight into the pixel. With ReSTIR the reservoir additionally carries the
/// candidate chosen for reuse, and `radiance` holds the radiance that can not be reused.
/// Without ReSTIR there is no reservoir and `radiance` is the entire estimate.
struct InitialSamplingResult {
    @if(RESTIR) reservoir: Reservoir,
    radiance: vec3<f32>,
}

/// Path vertices use the following convention: x0 = camera, x1 = primary ray hit (the G-buffer
/// surface), x2 = first BRDF-sampled hit (the reconnection vertex).
struct PathState {
    ray_origin: vec3<f32>,
    // raea: what ray_origin is off the vertex along, RAY_T_MIN out (path_origin)
    origin_normal: vec3<f32>,
    normal: vec3<f32>,
    wo: vec3<f32>,
    material: ResolvedMaterial,
    /// Throughput past x1. With ReSTIR the brdf*cos at x1 is factored out of it, see x1_brdf.
    throughput_past_first_hit: vec3<f32>,
    /// Reconnection vertex x2, the first BRDF-sampled hit shared by every length >= 2 candidate
    @if(RESTIR) x2_position: vec3<f32>,
    @if(RESTIR) x2_normal: vec3<f32>,
    /// If false, candidates built on x2 are shaded directly into radiance instead of
    /// published to the reservoir
    @if(RESTIR) x2_reusable: bool,
    /// brdf*cos at x1 for the direction toward x2, applied at shade time
    @if(RESTIR) x1_brdf: vec3<f32>,
    /// Radiance shaded directly at this pixel
    radiance: vec3<f32>,
    @if(RESTIR) reservoir: Reservoir,
    @if(RESTIR) weight_sum: f32,
    @if(RESTIR) selected_target_function: f32,
}

fn new_path_state(world_position: vec3<f32>, world_normal: vec3<f32>, wo: vec3<f32>, material: ResolvedMaterial) -> PathState {
    var path: PathState;
    path.ray_origin = world_position + (world_normal * RAY_T_MIN);
    path.origin_normal = world_normal;
    path.normal = world_normal;
    path.wo = wo;
    path.material = material;
    path.throughput_past_first_hit = vec3(1.0);
    path.radiance = vec3(0.0);
    @if(RESTIR) {
        path.x1_brdf = vec3(0.0);
        path.x2_position = vec3(0.0);
        path.x2_normal = vec3(0.0);
        path.x2_reusable = false;
        path.reservoir = empty_reservoir();
        path.reservoir.confidence_weight = 1.0;
        path.weight_sum = 0.0;
        path.selected_target_function = 0.0;
    }
    return path;
}

fn generate_initial_path(world_position: vec3<f32>, world_normal: vec3<f32>, material: ResolvedMaterial, workgroup_id: vec2<u32>, pixel_id: vec2<u32>, rng: ptr<function, u32>) -> InitialSamplingResult {
    let wo = normalize(view.world_position - world_position);
    let primary_NdotV = max(dot(world_normal, wo), 0.0001);
    let primary_F_ab = F_AB(material.perceptual_roughness, primary_NdotV);

    var path = new_path_state(world_position, world_normal, wo, material);

    @if(DLSS_RR_GUIDE_BUFFERS)
    var psr = psr_init(world_normal, material);

    for (var bounce = 0u; bounce < constants.max_bounces; bounce++) {
        let NdotV = max(dot(path.normal, path.wo), 0.0001);
        let F_ab = F_AB(path.material.perceptual_roughness, NdotV);

        // Stochastic NEE, with probability proportional to how diffuse the vertex is. Mirror-like
        // metals have too narrow a lobe for NEE to help, so mostly skip it there and let
        // BRDF-sampled emissive do the work. Pure dielectrics always run NEE.
        // raea: at least a quarter on a glossy metal short of a perfect mirror, so a small bright light it reflects (a
        // panorama's sun, EnvironmentSun) is sampled often enough to show; NEE's weights divide by p_nee, so this stays
        // unbiased. A perfect mirror still never samples lights and sees the sun in the environment map.
        let p_nee_upstream = mix(1.0, path.material.perceptual_roughness, path.material.metallic);
        let p_nee = select(max(p_nee_upstream, GLOSSY_NEE_FLOOR), p_nee_upstream, path.material.roughness <= MIRROR_ROUGHNESS_THRESHOLD);
        let di_samples = select(constants.secondary_di_samples, constants.primary_di_samples, bounce == 0u);
        generate_nee_candidate(&path, F_ab, p_nee, di_samples, workgroup_id, bounce, rng);

        // raea: Russian roulette on the first bounce (SolariLighting::first_bounce_probability): a workgroup (8 x 8
        // pixels) traces it with probability q, and its light counts 1 / q (in the throughput every bounce-0 candidate
        // is weighted by), so the estimate keeps its mean; a skipped pixel keeps its primary light sample and takes its
        // bounce from reuse. The coin is the workgroup's, not each pixel's: a SIMD group waits for its slowest ray, so
        // pixels skipped at random saved little (photo 3's forest, q 0.5: 10.25 -> 8.97 ms). At 1 nothing is drawn.
        let q = constants.first_bounce_probability;
        if bounce == 0u && q < 1.0 && first_bounce_coin(workgroup_id) >= q { break; }

        // Sample the BRDF and trace the next ray
        var next_bounce = evaluate_and_sample_brdf(path.wo, path.normal, path.material, F_ab, rng);
        if next_bounce.pdf == 0.0 { break; }
        if bounce == 0u && q < 1.0 { next_bounce.throughput /= q; }
        let ray = trace_ray(path_origin(path, next_bounce.wi), next_bounce.wi, RAY_T_MIN, RAY_T_MAX, RAY_FLAG_NONE);

        if ray.kind == RAY_QUERY_INTERSECTION_NONE {
            @if(DLSS_RR_GUIDE_BUFFERS)
            if !psr.finished && !next_bounce.diffuse_selected {
                psr_environment_miss(pixel_id, wo, psr.replace_fully, material, world_normal);
            }

            // raea: the sun only off a mirror's specular lobe, which light sampling can't reach; elsewhere it finds the sun
            // raea: and through a sky portal, weighted against light sampling, which finds that light too
            let environment_map_light_radiance = sample_environment_map_light_and_sun(next_bounce.wi, path.material.roughness <= MIRROR_ROUGHNESS_THRESHOLD && !next_bounce.diffuse_selected)
                * sky_portal_mis_weight(path.ray_origin, next_bounce.wi, next_bounce.pdf, p_nee * f32(di_samples));
            @if(RESTIR) {
                generate_environment_map_light_candidate(&path, next_bounce, environment_map_light_radiance, primary_F_ab, bounce, rng);
            } @else {
                path.radiance += path.throughput_past_first_hit * next_bounce.throughput * environment_map_light_radiance;
            }

            break;
        }

        var ray_hit = resolve_ray_hit_full(ray);
        // raea: a hit seen from its back shaded as its front (bindings.wesl face_hit), and a normal turned from the ray (a
        // crest seen in a mirror) bent back to face it, as the primary's is. Its light only from its front, as light
        // sampling finds it
        let hit_front = dot(ray_hit.geometric_world_normal, next_bounce.wi) <= 0.0;
        ray_hit = face_hit(ray_hit, -next_bounce.wi);
        let p_brdf = next_bounce.pdf;

        @if(DLSS_RR_GUIDE_BUFFERS)
        if !psr.finished {
            if next_bounce.diffuse_selected {
                // Took the non-specular lobe, so not a mirror reflection. Keep the guide-buffer defaults.
                psr.finished = true;
            } else {
                psr.throughput *= psr_bounce_reflectance(path.wo, next_bounce.wi, path.normal, path.material, F_ab, isinf(p_brdf));
                psr.reflection_length += ray.t;

                if !primary_surface_glossy(material) && is_delta_mirror(ray_hit.material) {
                    // Still in the mirror chain, fold this mirror's reflection into the chain.
                    psr = psr_chain_mirror(psr, ray_hit.world_normal);
                } else {
                    psr.finished = true;
                    replace_primary_surface(pixel_id, psr, ray_hit, material, world_position, world_normal, wo, primary_F_ab);
                }
            }
        }

        // Capture x2, the first BRDF-sampled hit
        if bounce == 0u {
            @if(RESTIR) {
                path.x2_position = ray_hit.world_position;
                path.x2_normal = ray_hit.world_normal;

                path.x1_brdf = evaluate_brdf(wo, next_bounce.wi, world_normal, material, primary_F_ab);

                path.x2_reusable = reconnection_reusable(ray.t, p_brdf, next_bounce.wi, next_bounce.diffuse_selected, ray_hit, world_position, material.perceptual_roughness, primary_NdotV);

                // The primary brdf*cos is applied at shade time, so divide it out of next_bounce.throughput
                // to leave 1/pdf (or 1/specular_weight for mirrors, avoiding the 1/INF = 0 that would kill
                // mirror GI).
                path.throughput_past_first_hit *= next_bounce.throughput / max(path.x1_brdf, vec3(0.0001));
            } @else {
                // Nothing is reused, so there is no shade-time reconnection to factor the primary
                // brdf*cos out for.
                path.throughput_past_first_hit *= next_bounce.throughput;
            }
        } else {
            // Later bounces keep the full brdf*cos/pdf for L_at_reconnection.
            path.throughput_past_first_hit *= next_bounce.throughput;
        }

        // Resample emissive hits
        if any(ray_hit.material.emissive > vec3(0.0)) && hit_front {
            generate_emissive_candidate(&path, ray_hit, next_bounce.wi, p_brdf, ray.t, p_nee, di_samples, bounce, rng);
        }

        // raea: a closed solid's inside (face_hit): dark, nothing more to find
        if !hit_front && !ray_hit.back_lit { break; }

        // Try terminating into the world cache
        if terminate_into_cache(&path, ray_hit, next_bounce.wi, ray.t, p_brdf, bounce, workgroup_id, rng) {
            break;
        }

        // Advance to the next vertex
        path.ray_origin = ray_hit.world_position + (ray_hit.geometric_world_normal * RAY_T_MIN);
        path.origin_normal = ray_hit.geometric_world_normal;
        path.normal = ray_hit.world_normal;
        path.wo = -next_bounce.wi;
        path.material = ray_hit.material;

        // Russian roulette for early termination
        if bounce > 0u {
            // With ReSTIR, throughput_past_first_hit has the primary brdf*cos divided out (so it can be
            // re-applied at shade time), which inflates it. Multiply x1_brdf back in to get the true
            // energy-bounded path throughput, which is the correct quantity for the RR survival
            // probability. Without ReSTIR it already is that quantity.
            var full_throughput = path.throughput_past_first_hit;
            @if(RESTIR)
            full_throughput *= max(path.x1_brdf, vec3(0.0001));
            let rr = saturate(luminance(full_throughput));
            if rand_f(rng) >= rr { break; }
            path.throughput_past_first_hit /= rr;
        }
    }

    var result: InitialSamplingResult;
    result.radiance = path.radiance;
    @if(RESTIR) {
        if path.selected_target_function > 0.0 {
            path.reservoir.unbiased_contribution_weight = path.weight_sum / path.selected_target_function;
        }
        result.reservoir = path.reservoir;
    }
    return result;
}

// raea: where a ray from the path's vertex toward wi starts: ray_origin, off the side the normal faces, or as far off the
// other for a ray through to the far side (diffuse transmission)
fn path_origin(path: PathState, wi: vec3<f32>) -> vec3<f32> {
    return select(path.ray_origin - path.origin_normal * (2.0 * RAY_T_MIN), path.ray_origin, dot(path.normal, wi) >= 0.0);
}

// raea: one draw in [0, 1) for a whole workgroup this frame (first_bounce_probability's coin)
fn first_bounce_coin(workgroup_id: vec2<u32>) -> f32 {
    var wg_rng = (workgroup_id.x * 5782582u) ^ (workgroup_id.y * 1103515245u) ^ constants.frame_rng;
    wg_rng = wg_rng * 747796405u + 2891336453u;
    return rand_f(&wg_rng);
}

fn generate_nee_candidate(
    path: ptr<function, PathState>,
    F_ab: vec2<f32>,
    p_nee: f32,
    di_samples: u32,
    workgroup_id: vec2<u32>,
    bounce: u32,
    rng: ptr<function, u32>,
) {
    if rand_f(rng) >= p_nee { return; }

    let di = sample_light_ris((*path).ray_origin, (*path).origin_normal, (*path).normal, (*path).wo, (*path).material, F_ab, di_samples, workgroup_id, bounce, rng);
    let di_target_function = luminance(di.brdf_radiance);
    if di_target_function <= 0.0 { return; }

    // MIS against the BRDF strategy. RIS over N candidates makes the effective NEE pdf at the
    // winner roughly N * light_pdf(winner), so scale by p_nee for the stochastic gate.
    var nee_mis_weight = 1.0;
    if di.brdf_rays_can_hit && di.inverse_solid_angle_pdf > 0.0 {
        let p_nee_strategy = f32(di_samples) * (1.0 / di.inverse_solid_angle_pdf) * p_nee;
        let p_brdf_at_nee = brdf_pdf((*path).wo, di.wi, (*path).normal, (*path).material, F_ab);
        nee_mis_weight = power_heuristic(p_nee_strategy, p_brdf_at_nee);
    }

    if bounce == 0u {
        @if(RESTIR) {
            // Bounce 0: Candidate is the light sample, stored by reference and re-resolved each frame
            // nee_mis_weight goes into the target function since it gets recomputed per-pixel during reuse
            let target_function = di_target_function * nee_mis_weight;
            let resampling_weight = target_function * di.unbiased_contribution_weight / p_nee;

            (*path).weight_sum += resampling_weight;
            if rand_f(rng) * (*path).weight_sum < resampling_weight {
                (*path).reservoir.light_sample = di.light_sample;
                (*path).selected_target_function = target_function;
            }
        } @else {
            (*path).radiance += di.brdf_radiance * di.unbiased_contribution_weight * nee_mis_weight / p_nee;
        }
    } else {
        // Deeper bounces: Candidate is the reconnection radiance at x2
        let L_at_reconnection = (*path).throughput_past_first_hit * di.brdf_radiance * di.unbiased_contribution_weight * nee_mis_weight / p_nee;

        @if(RESTIR) {
            resample_reconnection_candidate(path, L_at_reconnection, rng);
        } @else {
            (*path).radiance += L_at_reconnection;
        }
    }
}

struct DiSample {
    unbiased_contribution_weight: f32,
    light_sample: LightSample,
    wi: vec3<f32>,
    brdf_radiance: vec3<f32>,
    inverse_solid_angle_pdf: f32,
    brdf_rays_can_hit: bool,
}

fn sample_light_ris(ray_origin: vec3<f32>, origin_normal: vec3<f32>, normal: vec3<f32>, wo: vec3<f32>, material: ResolvedMaterial, F_ab: vec2<f32>, di_samples: u32, workgroup_id: vec2<u32>, bounce: u32, rng: ptr<function, u32>) -> DiSample {
    var workgroup_rng = (workgroup_id.x * 5782582u) + workgroup_id.y + bounce;
    let light_tile_start = rand_range_u(128u, &workgroup_rng) * 1024u;

    var weight_sum = 0.0;
    var selected_target_function = 0.0;
    var selected_tile_sample = 0u;
    var selected_world_position = vec4(0.0);
    var selected_wi = vec3(0.0);
    var selected_brdf_radiance = vec3(0.0);
    var selected_inverse_solid_angle_pdf = 0.0;
    var selected_brdf_rays_can_hit = false;
    let mis_weight = 1.0 / f32(di_samples);
    for (var i = 0u; i < di_samples; i++) {
        let tile_sample = light_tile_start + rand_range_u(1024u, rng);
        let resolved_light_sample = unpack_resolved_light_sample(light_tile_resolved_samples[tile_sample], view.exposure);
        let light_contribution = calculate_resolved_light_contribution(resolved_light_sample, ray_origin, normal);
        let brdf_current = evaluate_brdf(wo, light_contribution.wi, normal, material, F_ab);
        let brdf_radiance = brdf_current * light_contribution.radiance;

        let target_function = luminance(brdf_radiance);
        let resampling_weight = mis_weight * (target_function * light_contribution.inverse_pdf);

        weight_sum += resampling_weight;

        if rand_f(rng) * weight_sum < resampling_weight {
            selected_target_function = target_function;
            selected_tile_sample = tile_sample;
            selected_world_position = resolved_light_sample.world_position;
            selected_wi = light_contribution.wi;
            selected_inverse_solid_angle_pdf = light_contribution.inverse_solid_angle_pdf;
            selected_brdf_rays_can_hit = light_contribution.brdf_rays_can_hit;
            selected_brdf_radiance = brdf_radiance;
        }
    }

    var unbiased_contribution_weight = 0.0;
    if selected_target_function > 0.0 {
        unbiased_contribution_weight = weight_sum / selected_target_function;
        // raea: from the far side for light let through to this one (diffuse transmission)
        let origin = select(ray_origin - origin_normal * (2.0 * RAY_T_MIN), ray_origin, dot(normal, selected_wi) >= 0.0);
        unbiased_contribution_weight *= trace_visibility(origin, selected_world_position);
    }

    return DiSample(unbiased_contribution_weight, light_tile_samples[selected_tile_sample], selected_wi, selected_brdf_radiance, selected_inverse_solid_angle_pdf, selected_brdf_rays_can_hit);
}

fn generate_emissive_candidate(
    path: ptr<function, PathState>,
    ray_hit: ResolvedRayHitFull,
    wi: vec3<f32>,
    p_brdf: f32,
    ray_t: f32,
    p_nee: f32,
    di_samples: u32,
    bounce: u32,
    rng: ptr<function, u32>,
) {
    let NdotV_hit = max(dot(ray_hit.world_normal, -wi), 0.0001);
    let light_count = arrayLength(&light_sources);
    let area_pdf = 1.0 / (f32(light_count) * f32(ray_hit.triangle_count) * ray_hit.triangle_area);
    let p_light = area_pdf * ray_t * ray_t / NdotV_hit;
    let emissive_mis_weight = power_heuristic(p_brdf, p_light * p_nee * f32(di_samples));

    @if(RESTIR) {
        if !(*path).x2_reusable {
            // x1 -> x2 not reuse-safe (mirror/sharp lobe or failed gate): shade directly at this pixel
            // instead of publishing, since a reuse shift would waste it or make a firefly. Mirror lobes
            // always land here (p_brdf = INF, footprint 0), where emissive_mis_weight is 1.
            (*path).radiance += (*path).x1_brdf * (*path).throughput_past_first_hit * ray_hit.material.emissive * emissive_mis_weight;
        } else if bounce == 0u {
            // Bounce 0: Candidate is the emissive hit
            let target_function = luminance((*path).x1_brdf * ray_hit.material.emissive) * emissive_mis_weight;
            let resampling_weight = luminance((*path).x1_brdf * (*path).throughput_past_first_hit * ray_hit.material.emissive) * emissive_mis_weight;

            (*path).weight_sum += resampling_weight;
            if rand_f(rng) * (*path).weight_sum < resampling_weight {
                (*path).reservoir.light_sample = LightSample(NULL_LIGHT_ID, bitcast<u32>(area_pdf));
                (*path).reservoir.sample_point_world_position = (*path).x2_position;
                (*path).reservoir.sample_point_world_normal = octahedral_encode((*path).x2_normal);
                (*path).reservoir.radiance = ray_hit.material.emissive;
                (*path).selected_target_function = target_function;
            }
        } else {
            // Deeper bounces: Candidate is the reconnection radiance at x2
            let emissive_L_at_reconnection = (*path).throughput_past_first_hit * ray_hit.material.emissive * emissive_mis_weight;
            resample_reconnection_candidate(path, emissive_L_at_reconnection, rng);
        }
    } @else {
        (*path).radiance += (*path).throughput_past_first_hit * ray_hit.material.emissive * emissive_mis_weight;
    }
}

@if(RESTIR)
fn generate_environment_map_light_candidate(
    path: ptr<function, PathState>,
    next_bounce: EvaluateAndSampleBrdfResult,
    environment_map_light_radiance: vec3<f32>,
    primary_F_ab: vec2<f32>,
    bounce: u32,
    rng: ptr<function, u32>,
) {
    if all(environment_map_light_radiance == vec3(0.0)) { return; }

    if bounce == 0u {
        // Bounce 0: Candidate is the sky direction
        if !x1_lobe_reusable(next_bounce.pdf, next_bounce.diffuse_selected, (*path).material.perceptual_roughness) {
            (*path).radiance += next_bounce.throughput * environment_map_light_radiance;
            return;
        }

        // Publish the candidate as a point RAY_T_MAX along wi.
        // Neighbors see almost the same direction, and the reconnection jacobian evaluates to ~1.
        //
        // TODO: Once the environment map light is a proper LightSource that we sample during NEE,
        // store this as a LightSample instead.
        let x1_brdf = evaluate_brdf((*path).wo, next_bounce.wi, (*path).normal, (*path).material, primary_F_ab);
        let target_function = luminance(x1_brdf * environment_map_light_radiance);
        let resampling_weight = luminance(next_bounce.throughput * environment_map_light_radiance);

        (*path).weight_sum += resampling_weight;
        if rand_f(rng) * (*path).weight_sum < resampling_weight {
            (*path).reservoir.light_sample = LightSample(NULL_LIGHT_ID, 0u);
            (*path).reservoir.sample_point_world_position = (*path).ray_origin + next_bounce.wi * RAY_T_MAX;
            (*path).reservoir.sample_point_world_normal = octahedral_encode(-next_bounce.wi);
            (*path).reservoir.radiance = environment_map_light_radiance;
            (*path).selected_target_function = target_function;
        }
    } else {
        // Deeper bounces: Candidate is the reconnection radiance at x2
        let L_at_reconnection = (*path).throughput_past_first_hit * next_bounce.throughput * environment_map_light_radiance;
        resample_reconnection_candidate(path, L_at_reconnection, rng);
    }
}

fn terminate_into_cache(
    path: ptr<function, PathState>,
    ray_hit: ResolvedRayHitFull,
    wi: vec3<f32>,
    ray_t: f32,
    p_brdf: f32,
    bounce: u32,
    workgroup_id: vec2<u32>,
    rng: ptr<function, u32>,
) -> bool {
    // Only terminate into the world cache when the bounce was from a wide-enough BRDF sample
    // because the cache is less noisy than continuing the path for rough surfaces,
    // but less accurate for smooth surfaces
    let lobe_solid_angle = 1.0 / p_brdf;
    let broad_enough_to_terminate = lobe_solid_angle >= CACHE_TERMINATION_MIN_SOLID_ANGLE;
    let forced_terminate = bounce == constants.max_bounces - 1u;
    if !(broad_enough_to_terminate || forced_terminate) { return false; }

    // Only use the cache when the ray cleared the cache cell (diagonal = sqrt(3) * cell_size). Short
    // rays land in a cell that may straddle occluders and leak light through corners.
    // raea: a miss continues the path rather than end it on a zero. On the last bounce there is nothing to continue to,
    // so the path ends here anyway, with the lights' light on the hit and no cache: before, it ended with nothing, and
    // with max_bounces 1 every short first bounce (under a sofa, a trunk's foot, between moss pillows) lost its light
    var rng_copy = *rng;
    let world_cache_cell_size = get_cell_size(ray_hit.world_position, view.world_position, ray_t, &rng_copy);
    var cached_radiance = vec3(0.0);
    if ray_t > sqrt(3.0) * world_cache_cell_size {
        let cached = query_world_cache_found(ray_hit.world_position, ray_hit.geometric_world_normal, view.world_position, ray_t, WORLD_CACHE_CELL_LIFETIME, rng);
        if cached.w == 0.0 && !forced_terminate { return false; }
        cached_radiance = cached.rgb * f32(cached.w != 0.0);
    } else if !forced_terminate {
        return false;
    }

    // raea: the cache holds only light that has bounced (and the sky's, which no light sample finds); the lights' own
    // light on the hit is sampled at the hit with its full BRDF, alone since the path ends here (world_cache_update.wesl).
    // With as many candidates as the cache took for it (world_cache_direct_light_sample_count): with secondary_di_samples
    // (2) the sun on a sunlit desk, picked among photo 2's lights, came in spikes the denoiser dropped (0.79 of the path
    // tracer's light, from 0.86)
    let hit_wo = -wi;
    let hit_F_ab = F_AB(ray_hit.material.perceptual_roughness, max(dot(ray_hit.world_normal, hit_wo), 0.0001));
    let hit_origin = ray_hit.world_position + (ray_hit.geometric_world_normal * RAY_T_MIN);
    let di = sample_light_ris(hit_origin, ray_hit.geometric_world_normal, ray_hit.world_normal, hit_wo, ray_hit.material, hit_F_ab, max(constants.world_cache_direct_light_sample_count, 1u), workgroup_id, bounce + 1u, rng);

    let cache_outgoing = di.brdf_radiance * di.unbiased_contribution_weight + (ray_hit.material.base_color / PI) * cached_radiance;
    let cache_L_at_reconnection = (*path).throughput_past_first_hit * cache_outgoing;

    @if(RESTIR) {
        resample_reconnection_candidate(path, cache_L_at_reconnection, rng);
    } @else {
        (*path).radiance += cache_L_at_reconnection;
    }

    return true;
}

/// Resample radiance arriving at x1 through the reconnection vertex x2. `L_at_reconnection` excludes the
/// brdf*cos at x1, which is re-applied at shade time from whichever pixel ends up reusing the candidate.
@if(RESTIR)
fn resample_reconnection_candidate(path: ptr<function, PathState>, L_at_reconnection: vec3<f32>, rng: ptr<function, u32>) {
    if !(*path).x2_reusable {
        // x1 -> x2 not reuse-safe: shade directly at this pixel instead of publishing.
        (*path).radiance += (*path).x1_brdf * L_at_reconnection;
        return;
    }

    let target_function = luminance((*path).x1_brdf * L_at_reconnection);
    let resampling_weight = target_function;

    (*path).weight_sum += resampling_weight;
    if rand_f(rng) * (*path).weight_sum < resampling_weight {
        (*path).reservoir.light_sample = LightSample(NULL_LIGHT_ID, 0u);
        (*path).reservoir.sample_point_world_position = (*path).x2_position;
        (*path).reservoir.sample_point_world_normal = octahedral_encode((*path).x2_normal);
        (*path).reservoir.radiance = L_at_reconnection;
        (*path).selected_target_function = target_function;
    }
}

/// ReSTIR PT Enhanced: Algorithmic Advances for Faster and More Robust ReSTIR Path Tracing
/// Section 4 (sorta)
/// https://research.nvidia.com/labs/rtr/publication/lin2026restirptenhanced/lin2026restirptenhanced.pdf
@if(RESTIR)
fn reconnection_reusable(ray_t: f32, p_brdf: f32, wi: vec3<f32>, diffuse_selected: bool, ray_hit: ResolvedRayHitFull, world_position: vec3<f32>, x1_perceptual_roughness: f32, primary_NdotV: f32) -> bool {
    // ray_footprint = t^2 / (p_brdf * cos_x2) is the area a sample represents at x2. It goes to 0 for
    // mirror lobes (p_brdf = INF) and shrinks for sharp lobes or short segments. Compared against a
    // uniform 1/(4*PI) primary footprint, so the test trades roughness against distance.
    let cos_x2 = max(dot(ray_hit.world_normal, -wi), 0.0001);
    let ray_footprint = (ray_t * ray_t) / (p_brdf * cos_x2);
    let primary_dist = length(view.world_position - world_position);
    let primary_footprint = 4.0 * PI * primary_dist * primary_dist / primary_NdotV;
    let footprint_ok = ray_footprint >= (RECONNECTION_FOOTPRINT_KAPPA / 100.0) * primary_footprint;

    let x1_lobe_ok = x1_lobe_reusable(p_brdf, diffuse_selected, x1_perceptual_roughness);

    // Guard at x2. A sharp reflector there makes the stored radiance view-dependent and wrong to
    // reuse from a neighbor's direction. The roughness floor relaxes with segment length: a distant
    // glossy x2 is seen by neighbors from nearly the same direction, so the view-dependence washes out.
    // Diffuse, rough, and emissive vertices are always reuse-safe.
    let x2_is_light = any(ray_hit.material.emissive > vec3(0.0));
    let x2_roughness = mix(1.0, ray_hit.material.perceptual_roughness, ray_hit.material.metallic);
    let x2_roughness_floor = RECONNECTION_ROUGHNESS_MIN * saturate(RECONNECTION_RELAX_DISTANCE / ray_t);
    let x2_end_ok = x2_is_light || x2_roughness >= x2_roughness_floor;

    return footprint_ok && x1_lobe_ok && x2_end_ok;
}

/// Whether the lobe sampled at x1 is wide enough for its candidates to be reused by neighbors.
@if(RESTIR)
fn x1_lobe_reusable(p_brdf: f32, diffuse_selected: bool, x1_perceptual_roughness: f32) -> bool {
    return !isinf(p_brdf) && (diffuse_selected || x1_perceptual_roughness >= RECONNECTION_ROUGHNESS_MIN);
}
