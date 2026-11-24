//! The sun's shadows fade out where they end (docs/realism.feature LT21). Bevy blends each shadow cascade into the next
//! over their overlap, but draws everything past the last cascade's far bound fully lit, so a city's shadows stop in a
//! straight line across the street (Shane, 11-17, on the 540p fly: "the shadow having a clear cull distance"). Bevy's
//! shadows.wesl is swapped for one whose last cascade fades over the same overlap into the sun's far shadow map, and
//! that map shades everything past it (raea_sys_frame's swap_embedded_shader, which checks Bevy's file is still the one
//! this was written against). The map is raea-sys-rt's (far.rs: traced against the ray scene, bound by raea's bevy_pbr
//! as RaeaFarSun); with none, its size is 0 and it reads lit, so the last cascade fades to lit as before.

use bevy::prelude::*;

/// FNV-1a of bevy_pbr 0.20's render/shadows.wesl, the file the fade was written against
const UPSTREAM: u64 = 0xcd13_e017_3c8e_a62c;

/// the end of fetch_directional_shadow's blend into the next cascade, where the last cascade's fade goes
const FROM: &str = "            shadow = mix(shadow, next_shadow, (-view_z - next_near_bound) / (this_far_bound - next_near_bound));
        }
    }
    return shadow;
}";

const TO: &str = "            shadow = mix(shadow, next_shadow, (-view_z - next_near_bound) / (this_far_bound - next_near_bound));
        }
    } else {
        // raea: the last cascade fades over the cascades' overlap into the far shadow map (lit with none), so the
        // shadows don't end in a line
        let far_bound = (*light).cascades[cascade_index].far_bound;
        let fade_from = (1.0 - (*light).cascades_overlap_proportion) * far_bound;
        if (-view_z >= fade_from) {
            let far = raea_far_sun_shadow(light_id, frag_position.xyz, surface_normal);
            shadow = mix(shadow, far, (-view_z - fade_from) / max(far_bound - fade_from, 0.0001));
        }
    }
    return shadow;
}

// raea: the sun's shadow from its far shadow map (raea-sys-rt far.rs): each texel of the sun's view holds how high along
// the sun its ray first hit a shadow caster, kept at its column modulo the map's size so the map can follow the eye. A
// point is lit where it lies no lower along the sun than its texels' hits (the four nearest, bilinear), with a bias for
// its slope; pushed a texel off its surface first, so a surface doesn't shadow itself. Lit with no map (a size of 0),
// for a light that isn't the map's sun, outside the map, and fading to lit over its outer tenth.
fn raea_far_sun_shadow(light_id: u32, p: vec3<f32>, n: vec3<f32>) -> f32 {
    let f = view_bindings::raea_far_sun;
    let size = f.map.x;
    let to_light = view_bindings::lights.directional_lights[light_id].direction_to_light;
    let ndl = dot(n, f.dir.xyz);
    if (size == 0 || dot(to_light, f.dir.xyz) < 0.9999 || ndl <= 0.0) {
        return 1.0;
    }
    let texel = f.dir.w;
    let q = p + n * texel;
    let a = vec2(dot(q, f.u.xyz), dot(q, f.v.xyz)) / texel - 0.5;
    let half = f32(size) * 0.5;
    let edge = max(abs(a.x + 0.5 - f32(f.map.y)), abs(a.y + 0.5 - f32(f.map.z))) / half;
    if (edge >= 1.0) {
        return 1.0;
    }
    let depth = dot(q, f.dir.xyz) + f.u.w + texel * min(sqrt(max(1.0 - ndl * ndl, 0.0)) / ndl, 8.0);
    let i = vec2<i32>(floor(a));
    let w = fract(a);
    let lo = f.map.yz - vec2(size / 2);
    let hi = lo + vec2(size - 1);
    var lit: array<f32, 4>;
    for (var k = 0; k < 4; k++) {
        let t = clamp(i + vec2(k & 1, k >> 1u), lo, hi);
        let at = ((t % vec2(size)) + vec2(size)) % vec2(size);
        lit[k] = select(0.0, 1.0, textureLoad(view_bindings::raea_far_sun_map, at, 0).r <= depth);
    }
    let s = mix(mix(lit[0], lit[1], w.x), mix(lit[2], lit[3], w.x), w.y);
    return mix(s, 1.0, saturate((edge - 0.9) / 0.1));
}";

/// past the last cascade, where Bevy returns lit: the far shadow map instead
const PAST_FROM: &str = "    if (cascade_index >= (*light).num_cascades) {
        return 1.0;
    }";

const PAST_TO: &str = "    if (cascade_index >= (*light).num_cascades) {
        // raea: past the last cascade, the sun's far shadow map (lit with none)
        return raea_far_sun_shadow(light_id, frag_position.xyz, surface_normal);
    }";

/// Bevy's shadows.wesl with the last cascade's fade into the far shadow map, and the map past it
fn faded(bevy: &str) -> String {
    assert_eq!(bevy.matches(FROM).count(), 1, "raea: Bevy's shadows.wesl no longer ends its cascade blend where raea's fade goes (docs/bevy-upgrade.md)");
    assert_eq!(bevy.matches(PAST_FROM).count(), 1, "raea: Bevy's shadows.wesl no longer returns lit past its last cascade where raea's far shadow goes (docs/bevy-upgrade.md)");
    bevy.replace(FROM, TO).replace(PAST_FROM, PAST_TO)
}

/// Swaps Bevy's shadows.wesl for the faded one; after PbrPlugin, which embeds it. Without PBR there's nothing to fade.
pub fn install(app: &mut App) {
    if !raea_sys_frame::bevy_api::swap_embedded_shader(app, "bevy_pbr/render/shadows.wesl", UPSTREAM, faded) {
        debug!("raea-sys-lod: no bevy_pbr shadows.wesl to fade");
        return;
    }
    // PbrPlugin started loading Bevy's file when it was built, and an embedded file is read on the IO threads at once,
    // so by the time this plugin is built it already holds Bevy's bytes (on 11-17 the fly drew bit for bit as before
    // the swap): read again, from the swapped file
    if let Some(server) = app.world().get_resource::<AssetServer>() {
        server.reload("embedded://bevy_pbr/render/shadows.wesl");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The fade replaces the end of the cascade blend once and adds the last cascade's branch, into the far map; past
    /// the cascades, the far map instead of lit.
    #[test]
    fn the_last_cascade_fades_into_the_far_map() {
        let bevy = format!("fn fetch_directional_shadow() -> f32 {{\n{PAST_FROM}\n    if (next_cascade_index < n) {{\n        if (x) {{\n{FROM}\n");
        let ours = faded(&bevy);
        assert_eq!(ours.matches("raea: the last cascade fades").count(), 1);
        assert!(ours.contains("shadow = mix(shadow, far,"));
        assert!(ours.contains("return raea_far_sun_shadow(light_id"));
        assert_eq!(ours.matches("fn raea_far_sun_shadow").count(), 1);
    }
}
