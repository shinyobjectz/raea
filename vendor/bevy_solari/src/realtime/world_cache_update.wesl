import bevy_core_pipeline::tonemapping::tonemapping_luminance as luminance;
import bevy_pbr::render::utils::{rand_f, rand_range_u, sample_cosine_hemisphere};
import bevy_render::maths::PI;
import package::realtime::presample_light_tiles::unpack_resolved_light_sample;
import package::scene::sampling::{calculate_resolved_light_contribution, trace_visibility, sample_environment_map_light, sky_portal_light_pdf, ray_origin_toward_sample};
import package::scene::bindings::{trace_ray, resolve_ray_hit_full, face_hit, RAY_T_MIN, RAY_T_MAX};
import package::realtime::world_cache_query::query_world_cache_found;
import package::realtime::bindings::{
    light_tile_resolved_samples,
    view,
    constants,
    world_cache,
};

enable wgpu_ray_query;

@compute @workgroup_size(64, 1, 1)
fn sample_di(@builtin(workgroup_id) workgroup_id: vec3<u32>, @builtin(global_invocation_id) active_cell_id: vec3<u32>) {
    if active_cell_id.x >= world_cache.active_cells_count { return; }

    let cell_index = world_cache.active_cell_indices[active_cell_id.x];
    let geometry_data = world_cache.geometry_data[cell_index];
    var rng = cell_index + constants.frame_rng;

    if rand_f(&rng) >= f32(constants.world_cache_cell_updates_soft_target) / f32(world_cache.active_cells_count) { return; }

    // raea: a cell holds only the light that has bounced (and the sky's), so it starts from nothing here. What the lights
    // put on a surface is sampled where a path ends (initial_path.wesl terminate_into_cache) and where sample_gi's ray
    // lands, at the very point: taken at one point for a whole cell, it lost half the light a shade's liner throws
    // from the bulb a few cm off (LightBench's emitter.tsx bounce test: 0.48 of the path tracer's at a 4 cm gap)
    world_cache.active_cells_new_radiance[active_cell_id.x] = vec3(0.0);
}

@compute @workgroup_size(64, 1, 1)
fn sample_gi(@builtin(workgroup_id) workgroup_id: vec3<u32>, @builtin(global_invocation_id) active_cell_id: vec3<u32>) {
    if active_cell_id.x >= world_cache.active_cells_count { return; }

    let cell_index = world_cache.active_cell_indices[active_cell_id.x];
    let geometry_data = world_cache.geometry_data[cell_index];
    var rng = cell_index + constants.frame_rng;

    if rand_f(&rng) >= f32(constants.world_cache_cell_updates_soft_target) / f32(world_cache.active_cells_count) { return; }

    let ray_direction = sample_cosine_hemisphere(geometry_data.world_normal, &rng);
    let ray = trace_ray(geometry_data.world_position + (geometry_data.world_normal * RAY_T_MIN), ray_direction, RAY_T_MIN, constants.world_cache_max_gi_ray_distance, RAY_FLAG_NONE);
    if ray.kind != RAY_QUERY_INTERSECTION_NONE {
        // raea: a hit seen from its back as its front (bindings.wesl face_hit): its own cell, its light from its side; a
        // closed solid's inside is dark, a sample of nothing
        let unfaced = resolve_ray_hit_full(ray);
        if dot(unfaced.geometric_world_normal, ray_direction) > 0.0 && !unfaced.back_lit { return; }
        let ray_hit = face_hit(unfaced, -ray_direction);
        let cell_life = atomicLoad(&world_cache.life[cell_index]);
        let cached = query_world_cache_found(ray_hit.world_position, ray_hit.geometric_world_normal, view.world_position, ray.t, cell_life, &rng);
        // raea: a hit whose cell holds no light yet is no sample for a cell that has light: this update is skipped
        // (blend_new_samples), not blended in as a zero, which cut the chain of bounces inside a cavity. A cell with
        // none yet takes it (the lights' light at the hit alone), or a room with no sky would never fill
        if cached.w == 0.0 && world_cache.radiance[cell_index].a > 0.0 {
            world_cache.active_cells_new_radiance[active_cell_id.x] = vec3(-1.0);
            return;
        }
        // raea: the lights' own light on the surface the ray found, sampled there, with what has bounced onto it
        let direct = sample_random_light_ris(ray_hit.world_position, ray_hit.geometric_world_normal, ray_hit.material.diffuse_transmission, workgroup_id.xy, &rng);
        world_cache.active_cells_new_radiance[active_cell_id.x] += ray_hit.material.base_color * (direct + cached.rgb);
    } else {
        // The ray is cut short at world_cache_max_gi_ray_distance to save on traversal costs, so a miss may be a surface
        // further on. raea: it counts the sky only if the rest of the ray reaches it too (one any-hit query, cheap).
        // Upstream counted every miss as sky: in a hall larger than the cut (photo 5's, 41 x 88 x 38 m, under a sky
        // at its physical share of the sun) the cache filled with sky light, and the hall's dark surfaces came out
        // 2-80 times the path tracer's. A ray blocked past the cut adds nothing (the far surface's light is lost, a
        // darker bias where upstream's was a leak); the default cut is 1000 m, so that's rare.
        var escapes = true;
        let cut = constants.world_cache_max_gi_ray_distance;
        if cut < RAY_T_MAX {
            let origin = geometry_data.world_position + (geometry_data.world_normal * RAY_T_MIN) + ray_direction * cut;
            escapes = trace_ray(origin, ray_direction, 0.0, RAY_T_MAX, RAY_FLAG_TERMINATE_ON_FIRST_HIT).kind == RAY_QUERY_INTERSECTION_NONE;
        }
        // raea: light that came in through a sky portal is light sampling's (the cache's light is what it can't sample, as
        // an emissive's own light is left out of it)
        if escapes && sky_portal_light_pdf(geometry_data.world_position, ray_direction) == 0.0 {
            world_cache.active_cells_new_radiance[active_cell_id.x] += PI * sample_environment_map_light(ray_direction);
        }
    }
}

@compute @workgroup_size(64, 1, 1)
fn blend_new_samples(@builtin(global_invocation_id) active_cell_id: vec3<u32>) {
    if active_cell_id.x >= world_cache.active_cells_count { return; }

    let cell_index = world_cache.active_cell_indices[active_cell_id.x];
    var rng = cell_index + constants.frame_rng;

    if rand_f(&rng) >= f32(constants.world_cache_cell_updates_soft_target) / f32(world_cache.active_cells_count) { return; }

    let old_radiance = world_cache.radiance[cell_index];
    let new_radiance = world_cache.active_cells_new_radiance[active_cell_id.x];
    if new_radiance.x < 0.0 { return; } // raea: sample_gi's ray found no light in the cache; no sample this frame
    let luminance_delta = world_cache.luminance_deltas[cell_index];

    // https://bsky.app/profile/gboisse.bsky.social/post/3m5blga3ftk2a
    let sample_count = min(old_radiance.a + 1.0, constants.world_cache_max_temporal_samples);
    let alpha = abs(luminance_delta) / max(luminance(old_radiance.rgb), 0.001);
    let max_sample_count = mix(constants.world_cache_max_temporal_samples, 1.0, pow(saturate(alpha), 1.0 / 8.0));
    var blend_amount = 1.0 / min(sample_count, max_sample_count);
    if bool(constants.reset) {
        blend_amount = 1.0;
    }

    let blended_radiance = mix(old_radiance.rgb, new_radiance, blend_amount);
    let blended_luminance_delta = select(mix(luminance_delta, luminance(blended_radiance) - luminance(old_radiance.rgb), 1.0 / 8.0), 0.0, bool(constants.reset));

    world_cache.radiance[cell_index] = vec4(blended_radiance, sample_count);
    world_cache.luminance_deltas[cell_index] = blended_luminance_delta;
}

fn sample_random_light_ris(world_position: vec3<f32>, world_normal: vec3<f32>, diffuse_transmission: f32, workgroup_id: vec2<u32>, rng: ptr<function, u32>) -> vec3<f32> {
    var workgroup_rng = (workgroup_id.x * 5782582u) + workgroup_id.y;
    let light_tile_start = rand_range_u(128u, &workgroup_rng) * 1024u;

    var weight_sum = 0.0;
    var selected_sample_radiance = vec3(0.0);
    var selected_sample_target_function = 0.0;
    var selected_sample_world_position = vec4(0.0);
    let mis_weight = 1.0 / f32(constants.world_cache_direct_light_sample_count);
    for (var i = 0u; i < constants.world_cache_direct_light_sample_count; i++) {
        let tile_sample = light_tile_start + rand_range_u(1024u, rng);
        let resolved_light_sample = unpack_resolved_light_sample(light_tile_resolved_samples[tile_sample], view.exposure);
        let light_contribution = calculate_resolved_light_contribution(resolved_light_sample, world_position, world_normal);

        // raea: light from behind a thin surface for the share it lets through (diffuse transmission), the rest from in front
        let cos_l = dot(light_contribution.wi, world_normal);
        let cos_share = select(diffuse_transmission * -cos_l, (1.0 - diffuse_transmission) * cos_l, cos_l >= 0.0);
        let contribution = light_contribution.radiance * min(max(cos_share, 0.0), 1.0);
        let target_function = luminance(contribution);
        let resampling_weight = mis_weight * (target_function * light_contribution.inverse_pdf);

        weight_sum += resampling_weight;

        if rand_f(rng) * weight_sum < resampling_weight {
            selected_sample_radiance = contribution;
            selected_sample_target_function = target_function;
            selected_sample_world_position = resolved_light_sample.world_position;
        }
    }

    var unbiased_contribution_weight = 0.0;
    if any(selected_sample_radiance != vec3(0.0)) {
        let inverse_target_function = select(0.0, 1.0 / selected_sample_target_function, selected_sample_target_function > 0.0);
        unbiased_contribution_weight = weight_sum * inverse_target_function;

        unbiased_contribution_weight *= trace_visibility(ray_origin_toward_sample(world_position, world_normal, selected_sample_world_position), selected_sample_world_position);
    }

    return selected_sample_radiance * unbiased_contribution_weight;
}
