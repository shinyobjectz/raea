import bevy_core_pipeline::tonemapping::tonemapping_luminance as luminance;
import bevy_pbr::render::utils::{rand_f, rand_vec2f};
import bevy_render::view::View;
import package::scene::brdf::{evaluate_brdf, evaluate_and_sample_brdf, brdf_pdf, F_AB};
import package::scene::sampling::{sky_portal_mis_weight, generate_random_light_sample, calculate_resolved_light_contribution, trace_visibility, LightContribution, random_emissive_light_pdf, power_heuristic, sample_environment_map_light_and_sun};
import package::pathtracer::media::{media_collision, media_transmittance};
import raea_sys_fog::froxel_volume::{henyey_greenstein, sample_phase};
import package::scene::bindings::{trace_ray, resolve_ray_hit_full, RAY_T_MIN, RAY_T_MAX, MIRROR_ROUGHNESS_THRESHOLD, materials, material_ids, material_alpha, coat_albedo, face_hit, offset_ray_origin, ALPHA_PREMULTIPLIED};

enable wgpu_ray_query;

@group(1) @binding(0) var accumulation_texture: texture_storage_2d<rgba32float, read_write>;
@group(1) @binding(1) var view_output: texture_storage_2d<rgba16float, write>;
@group(1) @binding(2) var<uniform> view: View;

// raea: a light sample's contribution at p (sample_random_light's), dimmed by the media on its way to p
fn sample_light_through_media(p: vec3<f32>, normal: vec3<f32>, rng: ptr<function, u32>) -> LightContribution {
    let sample = generate_random_light_sample(rng);
    let light = sample.resolved_light_sample.world_position;
    var contribution = calculate_resolved_light_contribution(sample.resolved_light_sample, p, normal);
    if trace_visibility(p, light) == 0.0 {
        contribution.radiance = vec3(0.0);
        return contribution;
    }
    let reach = select(RAY_T_MAX, distance(light.xyz, p), light.w == 1.0);
    contribution.radiance *= media_transmittance(p, contribution.wi, 0.0, reach, rng);
    return contribution;
}

@compute @workgroup_size(8, 8, 1)
fn pathtrace(@builtin(global_invocation_id) global_id: vec3<u32>) {
    if any(global_id.xy >= vec2u(view.viewport.zw)) {
        return;
    }

    let old_color = textureLoad(accumulation_texture, global_id.xy);

    // Setup RNG
    let pixel_index = global_id.x + global_id.y * u32(view.viewport.z);
    let frame_index = u32(old_color.a) * 5782582u;
    var rng = pixel_index + frame_index;

    // Shoot the first ray from the camera
    let pixel_center = vec2<f32>(global_id.xy) + 0.5;
    let jitter = rand_vec2f(&rng) - 0.5;
    let pixel_uv = (pixel_center + jitter) / view.viewport.zw;
    let pixel_ndc = (pixel_uv * 2.0) - 1.0;
    let primary_ray_target = view.world_from_clip * vec4(pixel_ndc.x, -pixel_ndc.y, 1.0, 1.0);
    var ray_origin = view.world_position;
    var ray_direction = normalize((primary_ray_target.xyz / primary_ray_target.w) - ray_origin);
    var ray_t_min = 0.0;

    // Path trace
    var radiance = vec3(0.0);
    var throughput = vec3(1.0);
    var p_bounce = 0.0;
    // raea: whether this ray samples no lights, so sees the environment map's sun itself (a camera ray, one off a mirror)
    var sees_sun = true;
    loop {
        let ray = trace_ray(ray_origin, ray_direction, ray_t_min, RAY_T_MAX, RAY_FLAG_NONE);
        // raea: the media (fog volumes, media.wesl) before the surface: a collision scatters the path there. Light is
        // sampled at it by the phase (weighted against the phase's own rays reaching an emissive surface), and the
        // path goes on along a direction drawn from the phase, which the phase's pdf cancels
        let t_hit = select(RAY_T_MAX, ray.t, ray.kind != RAY_QUERY_INTERSECTION_NONE);
        let collision = media_collision(ray_origin, ray_direction, ray_t_min, t_hit, &rng);
        if collision.t >= 0.0 {
            let p = ray_origin + ray_direction * collision.t;
            throughput *= collision.albedo;
            let light = sample_light_through_media(p, ray_direction, &rng);
            let phase = henyey_greenstein(collision.g, dot(light.wi, ray_direction));
            var weight = 1.0;
            if light.brdf_rays_can_hit {
                weight = power_heuristic(1.0 / light.inverse_solid_angle_pdf, phase);
            }
            radiance += weight * throughput * light.radiance * light.inverse_pdf * phase;
            let next = sample_phase(collision.g, ray_direction, rand_vec2f(&rng));
            p_bounce = henyey_greenstein(collision.g, dot(next, ray_direction));
            ray_origin = p;
            ray_direction = next;
            ray_t_min = 0.0;
            sees_sun = false;
            let rr = saturate(luminance(throughput));
            if rand_f(&rng) >= rr { break; }
            throughput /= rr;
            continue;
        }
        if ray.kind != RAY_QUERY_INTERSECTION_NONE {
            var ray_hit = resolve_ray_hit_full(ray);
            // raea: a premultiplied surface the ray stopped at: shaded in full for its alpha's share of the stops, else as
            // its clear coat alone, weighted back to the coat's full strength (bindings.wesl alpha_accepts)
            let hit_material = materials[material_ids[ray.instance_custom_data]];
            if hit_material.alpha_mode == ALPHA_PREMULTIPLIED {
                let alpha = material_alpha(hit_material, ray_hit.uv);
                let coat = coat_albedo(hit_material.reflectance, abs(dot(ray_hit.geometric_world_normal, ray_direction)));
                if rand_f(&rng) * (alpha + (1.0 - alpha) * coat) >= alpha {
                    ray_hit.material.base_color = vec3(0.0);
                    ray_hit.material.metallic = 0.0;
                    throughput /= coat;
                }
            }
            let wo = -ray_direction;
            // raea: a hit seen from its back shaded as its front (bindings.wesl face_hit); its light only from its front,
            // as light sampling finds it
            let hit_front = dot(ray_hit.geometric_world_normal, wo) >= 0.0;
            if !hit_front && !ray_hit.back_lit { break; }
            ray_hit = face_hit(ray_hit, wo);
            let NdotV = max(dot(ray_hit.world_normal, wo), 0.0001);
            let F_ab = F_AB(ray_hit.material.perceptual_roughness, NdotV);

            // Emissive contribution
            var mis_weight = 1.0;
            if p_bounce != 0.0 { // Not first bounce
                let p_light = random_emissive_light_pdf(ray_hit, ray.t, NdotV);
                mis_weight = power_heuristic(p_bounce, p_light);
            }
            if hit_front {
                radiance += mis_weight * throughput * ray_hit.material.emissive;
            }

            // Sample direct lighting, but only if the surface is not mirror-like
            let is_perfectly_specular = ray_hit.material.roughness <= MIRROR_ROUGHNESS_THRESHOLD && ray_hit.material.metallic > 0.9999;
            if !is_perfectly_specular {
                let direct_lighting = sample_light_through_media(ray_hit.world_position, ray_hit.world_normal, &rng);

                mis_weight = 1.0;
                if direct_lighting.brdf_rays_can_hit {
                    let pdf_of_bounce = brdf_pdf(wo, direct_lighting.wi, ray_hit.world_normal, ray_hit.material, F_ab);
                    mis_weight = power_heuristic(1.0 / direct_lighting.inverse_solid_angle_pdf, pdf_of_bounce);
                }

                let direct_lighting_brdf = evaluate_brdf(wo, direct_lighting.wi, ray_hit.world_normal, ray_hit.material, F_ab);
                radiance += mis_weight * throughput * direct_lighting.radiance * direct_lighting.inverse_pdf * direct_lighting_brdf;
            }

            // Sample new ray direction from the material BRDF for next bounce and apply BRDF
            let next_bounce = evaluate_and_sample_brdf(wo, ray_hit.world_normal, ray_hit.material, F_ab, &rng);
            if next_bounce.pdf == 0.0 { break; }
            // raea: off a mirror's specular lobe, which light sampling can't reach, the ray sees the sun itself
            sees_sun = ray_hit.material.roughness <= MIRROR_ROUGHNESS_THRESHOLD && !next_bounce.diffuse_selected;
            ray_direction = next_bounce.wi;
            ray_origin = offset_ray_origin(ray_hit.world_position, ray_hit.geometric_world_normal, ray_hit.world_normal, ray_direction);
            ray_t_min = RAY_T_MIN;
            p_bounce = next_bounce.pdf;
            throughput *= next_bounce.throughput;

            // Russian roulette for early termination
            let rr = saturate(luminance(throughput));
            if rand_f(&rng) >= rr { break; }
            throughput /= rr;
        } else {
            // raea: through a sky portal, weighted against light sampling, which finds that light too
            radiance += throughput * sample_environment_map_light_and_sun(ray_direction, sees_sun) * sky_portal_mis_weight(ray_origin, ray_direction, p_bounce, 1.0);
            break;
        }
    }

    // Camera exposure
    radiance *= view.exposure;

    // Accumulation over time via running average
    let new_color = mix(old_color.rgb, radiance, 1.0 / (old_color.a + 1.0));
    textureStore(accumulation_texture, global_id.xy, vec4(new_color, old_color.a + 1.0));
    textureStore(view_output, global_id.xy, vec4(new_color, 1.0));
}
