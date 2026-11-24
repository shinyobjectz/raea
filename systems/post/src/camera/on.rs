//! Each group of the camera row's post-processing put on the camera, and taken off (camera.rs's GROUPS).

use super::*;

/// Shadow quality (a <Shadows>): the sun's and spots' maps are 4096 texels a side unless it says otherwise (Bevy's 2048
/// speckles and blurs a film's shadows; smaller on a GPU with little memory of its own, raea_sys_frame::shadow); point
/// lights' cube maps half that, 1024 by default as Bevy's. The filter on the camera.
pub fn shadows(world: &mut World, e: Entity, vals: &Vals) {
    if !preview() {
        let size = match vals.get("shadow_map") { Some(V::N(n)) => *n as usize, _ => raea_sys_frame::shadow::shadow_map_default(world) };
        if let Some(mut m) = world.get_resource_mut::<bevy::light::DirectionalLightShadowMap>() {
            m.size = size;
        }
        if let Some(mut m) = world.get_resource_mut::<bevy::light::PointLightShadowMap>() {
            m.size = if vals.contains_key("shadow_map") { size / 2 } else { 1024 };
        }
    }
    let filter = match vals.get("shadow_filter") { Some(V::S(s)) if s == "hardware" => bevy::light::ShadowFilteringMethod::Hardware2x2, _ => bevy::light::ShadowFilteringMethod::Gaussian };
    world.entity_mut(e).insert(filter);
}

/// The sky a camera's <Atmosphere> spawned (its own entity, despawned when a frame has none).
#[derive(Component)]
pub struct AtmosphereOf(Entity);

/// A procedural sky (<Atmosphere>): Bevy 0.20's scattering atmosphere, Earth's air round a planet whose ground is at y 0,
/// lit by the suns; its sky drawn behind everything, and with `atmo_light` it lights the scene too
/// (AtmosphereEnvironmentMapLight). AtmosphereSettings needs Hdr, which it brings.
pub fn atmosphere(world: &mut World, e: Entity, vals: &Vals) {
    if !vals.get("atmosphere").is_some_and(V::truthy) {
        return;
    }
    use bevy::light::{Atmosphere, atmosphere::ScatteringMedium};
    let medium = world.resource_mut::<Assets<ScatteringMedium>>().add(ScatteringMedium::earth(256, 256));
    let mut atmo = Atmosphere::earth(medium);
    if let Some(a) = vals.get("atmo_albedo").map(V::num) {
        atmo.ground_albedo = Vec3::splat(a as f32);
    }
    let sky = world.spawn((atmo, Mirror)).id();
    world.entity_mut(e).insert((bevy::pbr::AtmosphereSettings::default(), AtmosphereOf(sky)));
    if vals.get("atmo_light").is_some_and(V::truthy) {
        world.entity_mut(e).insert(bevy::light::AtmosphereEnvironmentMapLight::default());
    }
}

pub fn atmosphere_off(world: &mut World, e: Entity) {
    if let Some(AtmosphereOf(sky)) = world.entity_mut(e).take::<AtmosphereOf>() {
        world.despawn(sky);
    }
    world.entity_mut(e).remove::<(bevy::pbr::AtmosphereSettings, bevy::light::AtmosphereEnvironmentMapLight)>();
}

pub fn bloom(world: &mut World, e: Entity, vals: &Vals) {
    let bloom = num(vals, "bloom", 0.0);
    if bloom > 0.0 {
        let mut b = bevy::post_process::bloom::Bloom::NATURAL;
        b.intensity = bloom;
        // a glow added on top of the frame (three.js's UnrealBloom, postprocessing's), where Bevy's natural bloom takes
        // its light from around it
        if vals.get("bloom_additive").is_some_and(V::truthy) {
            b.composite_mode = bevy::post_process::bloom::BloomCompositeMode::Additive;
        }
        if let Some(th) = vals.get("bloom_threshold").map(V::num) {
            b.prefilter.threshold = th as f32;
            b.prefilter.threshold_softness = 0.2;
        }
        world.entity_mut(e).insert(b);
    }
}

pub fn fog(world: &mut World, e: Entity, vals: &Vals) {
    if vals.contains_key("fog_color") || vals.contains_key("fog_end") || vals.contains_key("fog_density") {
        let falloff = match vals.get("fog_density") {
            Some(d) => FogFalloff::Exponential { density: d.num() as f32 },
            None => FogFalloff::Linear { start: num(vals, "fog_start", 5.0), end: num(vals, "fog_end", 20.0) },
        };
        world.entity_mut(e).insert(DistanceFog { color: colour(vals.get("fog_color"), [0.5, 0.5, 0.55, 1.0]), falloff, ..default() });
    }
}

pub fn tonemap(world: &mut World, e: Entity, vals: &Vals) {
    let Some(V::S(tm)) = vals.get("tonemap") else { return };
    use bevy::core_pipeline::tonemapping::Tonemapping as T;
    let t = match tm.as_str() {
        "none" => T::None,
        "reinhard" => T::Reinhard,
        "aces" => T::AcesFitted,
        "agx" => T::AgX,
        "neutral" | "blender" => T::BlenderFilmic,
        _ => T::TonyMcMapface,
    };
    // "none" draws as Bevy 0.19's None did (bevy_api::Untonemapped)
    if t == T::None {
        world.entity_mut(e).insert((t, bevy_api::Untonemapped));
    } else {
        world.entity_mut(e).insert(t).remove::<bevy_api::Untonemapped>();
    }
}

pub fn tonemap_off(world: &mut World, e: Entity) {
    world.entity_mut(e).insert(bevy::core_pipeline::tonemapping::Tonemapping::default()).remove::<bevy_api::Untonemapped>();
}

pub fn exposure(world: &mut World, e: Entity, vals: &Vals) {
    if let Some(ev) = vals.get("exposure") {
        world.entity_mut(e).insert(bevy::camera::Exposure { ev100: ev.num() as f32 });
    }
}

/// A <Grade>: Bevy's colour grading (before tone mapping, but for the global saturation).
pub fn grading(world: &mut World, e: Entity, vals: &Vals) {
    if !vals.keys().any(|k| k.starts_with("grade_")) {
        return;
    }
    use bevy::render::view::{ColorGrading, ColorGradingSection};
    let mut cg = ColorGrading::default();
    cg.global.temperature = num(vals, "grade_temperature", 0.0);
    cg.global.tint = num(vals, "grade_tint", 0.0);
    cg.global.post_saturation = num(vals, "grade_saturation", 1.0);
    // stops on every pixel just before the curve (Bevy's tonemapping applies it), where <ToneMapping exposure>, the
    // camera's, lights only lit and emissive shading: three's toneMappingExposure (and its ACES's 1/0.6) is this one
    cg.global.exposure = num(vals, "grade_exposure", 0.0);
    for (pre, sec) in [("grade_s_", &mut cg.shadows), ("grade_m_", &mut cg.midtones), ("grade_h_", &mut cg.highlights)] {
        let d = ColorGradingSection::default();
        *sec = ColorGradingSection {
            saturation: num(vals, &format!("{pre}saturation"), d.saturation),
            contrast: num(vals, &format!("{pre}contrast"), d.contrast),
            gamma: num(vals, &format!("{pre}gamma"), d.gamma),
            gain: num(vals, &format!("{pre}gain"), d.gain),
            lift: num(vals, &format!("{pre}lift"), d.lift),
        };
    }
    world.entity_mut(e).insert(cg);
}

/// A <Grade>'s vignette (sync_frame_look moves it to the composite camera under a frame look).
pub fn vignette(world: &mut World, e: Entity, vals: &Vals) {
    if let Some(v) = vals.get("vignette").map(V::num) {
        use bevy::post_process::effect_stack::Vignette;
        let d = Vignette::default();
        world.entity_mut(e).insert(Vignette {
            intensity: v as f32,
            radius: num(vals, "vignette_radius", d.radius),
            smoothness: num(vals, "vignette_smooth", d.smoothness),
            color: colour(vals.get("vignette_color"), [0.0, 0.0, 0.0, 1.0]),
            ..d
        });
    }
}

/// Ambient occlusion (no multisampling with it: its noise is a fixed pattern without TAA's jitter, so a frame's
/// occlusion is a function of its rows) and the anti-aliasing that replaces multisampling (sync sets Msaa).
pub fn aa(world: &mut World, e: Entity, vals: &Vals) {
    if let Some(V::S(q)) = vals.get("ssao").filter(|_| !preview()) {
        use bevy::pbr::{ScreenSpaceAmbientOcclusion, ScreenSpaceAmbientOcclusionQualityLevel as Q};
        let quality_level = match q.as_str() { "low" => Q::Low, "high" => Q::High, "ultra" => Q::Ultra, _ => Q::Medium };
        world.entity_mut(e).insert(ScreenSpaceAmbientOcclusion {
            quality_level,
            constant_object_thickness: num(vals, "ssao_thickness", 0.25),
            // the radius is Bevy's default (0.20's is 0.19's hard-coded 0.5 x 1.457)
            ..default()
        });
    }
    // raea's temporal filter (steady.rs): <AntiAliasing mode="steady">, or on every 3D camera with RAEA_STEADY (LT1's
    // scenes measured with it). Its depth and motion prepasses are read single-sampled (PrepassRead keeps MSAA off).
    let steady = matches!(vals.get("aa"), Some(V::S(m)) if m == "steady") || std::env::var_os("RAEA_STEADY").is_some();
    if steady && !preview() && world.get::<Camera3d>(e).is_some() {
        world.entity_mut(e).insert((crate::steady::Steady::of(vals), raea_sys_compose::passes::PrepassRead, Msaa::Off));
    }
    match vals.get("aa") {
        Some(V::S(m)) if m == "smaa" && !preview() => {
            world.entity_mut(e).insert(bevy::anti_alias::smaa::Smaa { preset: bevy::anti_alias::smaa::SmaaPreset::High });
        }
        Some(V::S(m)) if m == "fxaa" && !preview() => {
            world.entity_mut(e).insert(bevy::anti_alias::fxaa::Fxaa::default());
        }
        _ => {}
    }
}

/// Depth of field, its aperture and focus then following the frame (sync_camera). A frame without dof_max has none: a
/// play's row keeps the last aperture it was sent.
pub fn dof(world: &mut World, e: Entity, vals: &Vals) {
    if let Some(f) = vals.get("dof_aperture").map(V::num).filter(|_| vals.contains_key("dof_max")) {
        use bevy::post_process::dof::{DepthOfField, DepthOfFieldMode};
        let mode = if matches!(vals.get("dof_mode"), Some(V::S(m)) if m == "gaussian") { DepthOfFieldMode::Gaussian } else { DepthOfFieldMode::Bokeh };
        // the background counts as far as the camera sees, not infinitely far (its blur would be the largest)
        world.entity_mut(e).insert(DepthOfField { mode, focal_distance: num(vals, "focus", 5.0), aperture_f_stops: f as f32, max_circle_of_confusion_diameter: num(vals, "dof_max", 64.0), max_depth: num(vals, "far", 100.0), ..default() });
    }
}

/// What goes on once, its values then following the frame elsewhere: ink outlines (sync_frame_look), the colour
/// fringes and the lens (sync_camera).
pub fn once(world: &mut World, e: Entity, vals: &Vals) {
    // ink outlines from the depth and normal prepasses (Msaa::Off, set where the camera is made)
    if vals.contains_key("outline") {
        let line = colour(vals.get("outline"), [0.0, 0.0, 0.0, 1.0]).to_linear().to_f32_array();
        world.entity_mut(e).insert((
            crate::outline::InkOutline { color: Vec4::from_array(line), depth: num(vals, "outline_depth", 0.04), normal: num(vals, "outline_normal", 0.3), width: num(vals, "outline_width", 1.0).max(1.0), far: num(vals, "outline_far", 0.0) / num(vals, "near", 0.1), skip: raea_sys_compose::compose::bits(vals, "outline_skip") },
            bevy::core_pipeline::prepass::DepthPrepass,
            bevy::core_pipeline::prepass::NormalPrepass,
        ));
    }
    if let Some(a) = vals.get("aberration").map(V::num) {
        use bevy::post_process::effect_stack::ChromaticAberration;
        // its intensity is each frame's (sync_camera)
        world.entity_mut(e).insert(ChromaticAberration { intensity: a as f32, ..default() });
    }
    if crate::lens::has_lens(vals) {
        // Fisheye / LensDistortion: its values come from each frame's row (sync_camera)
        world.entity_mut(e).insert(crate::lens::Lens::default());
    }
}
