//! An <Environment> on the camera: equirectangular panoramas (.hdr, .exr, .png, .jpg) made into cubemaps at load,
//! lighting the scene through Bevy's environment-map filter (GeneratedEnvironmentMapLight: a fixed blue noise, so the
//! light is a function of its panorama alone) and drawn as its sky with a Skybox. Every panorama the piece uses is
//! converted once, at one face size, and each frame's row picks one, so a cut from a hall to a street changes the light
//! on that frame, in a render as in play.
//!
//! The filter runs once per panorama, not every frame: each panorama has a bake entity of its own (EnvBake) carrying
//! a GeneratedEnvironmentMapLight with its own diffuse and specular targets, and the camera carries a plain
//! EnvironmentMapLight pointing at the targets of the panorama its row picks. Once Bevy has filtered a bake (its
//! pipelines ready and its bind groups made, seen in the render world), the bake's GeneratedEnvironmentMapLight is
//! taken off and its targets keep the filtered light. Filtering every frame was 2.2 ms of GPU a frame at 1920x1080 in
//! the Rome film (lightprobe_radiance_map and lightprobe_irradiance_map), for the same light each frame.
//!
//! Camera columns: env_srcs (a JSON list of panorama paths), env_i (which one this frame, -1 none; keyed), env_k (its
//! intensity in cd/m²; keyed), env_rot (a turn about +Y, radians; keyed), sky_k (the sky's brightness, 0 no sky;
//! keyed).

use super::*;
use bevy::core_pipeline::Skybox;
use bevy::light::{EnvironmentMapLight, GeneratedEnvironmentMapLight};
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureViewDescriptor, TextureViewDimension};

/// The camera's panoramas as cubemaps, in env_srcs' order, and each one's bake entity (its filtered light).
#[derive(Component)]
pub(super) struct EnvMaps(Vec<Handle<Image>>, Vec<Entity>);

/// A panorama's filtered light: Bevy's generator runs on it (GeneratedEnvironmentMapLight) until it has once, then its
/// EnvironmentMapLight's targets hold the light (retire_bakes).
#[derive(Component)]
pub(super) struct EnvBake;

/// The bakes the render world has seen filtered (their main-world entities): shared by both worlds.
#[derive(Resource, Clone, Default)]
pub(super) struct EnvBaked(std::sync::Arc<std::sync::Mutex<std::collections::HashSet<Entity>>>);

/// The diffuse (32x32) and specular (the source's size, all its mips) targets Bevy's generator fills for a source
/// cubemap `size` texels a face, made as generate_environment_map_light makes them.
fn filter_targets(images: &mut Assets<Image>, size: u32) -> (Handle<Image>, Handle<Image>) {
    use bevy::asset::RenderAssetUsages;
    use bevy::render::render_resource::TextureUsages;
    let mut diffuse = Image::new_fill(Extent3d { width: 32, height: 32, depth_or_array_layers: 6 }, TextureDimension::D2, &[0; 8], TextureFormat::Rgba16Float, RenderAssetUsages::all());
    diffuse.texture_descriptor.usage = TextureUsages::TEXTURE_BINDING | TextureUsages::STORAGE_BINDING;
    diffuse.texture_view_descriptor = Some(TextureViewDescriptor { dimension: Some(TextureViewDimension::Cube), ..default() });
    let mips = 32 - size.leading_zeros();
    let mut specular = Image::new_fill(Extent3d { width: size, height: size, depth_or_array_layers: 6 }, TextureDimension::D2, &[0; 8], TextureFormat::Rgba16Float, RenderAssetUsages::all());
    specular.texture_descriptor.usage = TextureUsages::TEXTURE_BINDING | TextureUsages::STORAGE_BINDING;
    specular.texture_descriptor.mip_level_count = mips;
    specular.data = None;
    specular.texture_view_descriptor = Some(TextureViewDescriptor { dimension: Some(TextureViewDimension::Cube), mip_level_count: Some(mips), ..default() });
    (images.add(diffuse), images.add(specular))
}

/// Bevy's environment filter with 16 times its samples (env_filter.wgsl) in its place: the bake runs once a panorama,
/// and at Bevy's count a glossy coat showed the filter's noise as soft cells. Bevy registers its own in its plugin's
/// finish, so this one goes in at finish too, after it (plugins finish in the order they were added).
struct FilterShader;

impl Plugin for FilterShader {
    fn build(&self, _: &mut App) {}
    fn finish(&self, app: &mut App) {
        app.world().resource::<bevy::asset::io::embedded::EmbeddedAssetRegistry>().insert_asset(
            std::path::PathBuf::new(),
            std::path::Path::new("bevy_pbr/light_probe/environment_filter.wgsl"),
            include_str!("env_filter.wgsl").as_bytes(),
        );
    }
}

/// The bakes' bookkeeping: EnvBaked in both worlds, the render world noting each bake once Bevy's generator could run
/// on it, and the main world taking its generator off after.
pub(super) fn install(app: &mut App) {
    app.add_plugins(FilterShader);
    let baked = EnvBaked::default();
    app.insert_resource(baked.clone()).add_systems(Update, retire_bakes);
    if let Some(render) = app.get_sub_app_mut(bevy::render::RenderApp) {
        render.insert_resource(baked).add_systems(
            bevy::render::Render,
            note_baked.after(bevy::pbr::generate::filtering_system).before(bevy::render::RenderSystems::Render),
        );
    }
}

/// Render world: a bake whose generator ran this frame (every pipeline it takes is ready and its bind groups are made,
/// what downsampling_system and filtering_system need before they dispatch).
fn note_baked(
    q: Query<&bevy::render::sync_world::MainEntity, (With<bevy::pbr::generate::GeneratorBindGroups>, With<bevy::pbr::generate::RenderEnvironmentMap>)>,
    pipelines: Option<Res<bevy::pbr::generate::GeneratorPipelines>>,
    cache: Res<bevy::render::render_resource::PipelineCache>,
    baked: Res<EnvBaked>,
) {
    let Some(p) = pipelines else { return };
    if [p.downsample_first, p.downsample_second, p.copy, p.radiance, p.irradiance].iter().any(|id| cache.get_compute_pipeline(*id).is_none()) {
        return;
    }
    let mut set = baked.0.lock().unwrap();
    for m in &q {
        set.insert(m.id());
    }
}

/// Main world: a bake the render world has filtered keeps its targets and stops filtering.
fn retire_bakes(mut commands: Commands, q: Query<Entity, (With<EnvBake>, With<GeneratedEnvironmentMapLight>)>, baked: Res<EnvBaked>) {
    let set = baked.0.lock().unwrap();
    for e in &q {
        if set.contains(&e) {
            commands.entity(e).remove::<GeneratedEnvironmentMapLight>();
        }
    }
}

/// A panorama's linear RGB, row by row from the top.
struct Equirect {
    w: usize,
    h: usize,
    px: Vec<[f32; 3]>,
}

fn srgb_to_linear(c: f32) -> f32 {
    if c <= 0.04045 { c / 12.92 } else { ((c + 0.055) / 1.055).powf(2.4) }
}

/// A panorama file decoded: Radiance HDR and 8-bit pictures through `image`, OpenEXR through ffmpeg (as a PFM, so
/// the engine needs no EXR decoder of its own).
fn equirect(path: &Path) -> Result<Equirect, String> {
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("").to_ascii_lowercase();
    if ext == "exr" {
        let out = std::process::Command::new("ffmpeg")
            .args(["-v", "error", "-i"]).arg(path)
            .args(["-frames:v", "1", "-c:v", "pfm", "-pix_fmt", "gbrpf32le", "-f", "image2pipe", "-"])
            .output()
            .map_err(|e| format!("{}: an .exr is read with ffmpeg, which didn't run ({e}); give an .hdr instead", path.display()))?;
        if !out.status.success() {
            return Err(format!("{}: ffmpeg couldn't read it: {}", path.display(), String::from_utf8_lossy(&out.stderr).trim()));
        }
        return pfm(&out.stdout).ok_or_else(|| format!("{}: ffmpeg's PFM didn't parse", path.display()));
    }
    let img = image::open(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let hdr = matches!(img, image::DynamicImage::ImageRgb32F(_) | image::DynamicImage::ImageRgba32F(_));
    let rgb = img.to_rgb32f();
    let (w, h) = (rgb.width() as usize, rgb.height() as usize);
    // an 8-bit picture is sRGB-encoded; a float one is already linear light
    let px = rgb.pixels().map(|p| if hdr { [p[0], p[1], p[2]] } else { [srgb_to_linear(p[0]), srgb_to_linear(p[1]), srgb_to_linear(p[2])] }).collect();
    Ok(Equirect { w, h, px })
}

/// A PFM (colour, "PF"): its header, then RGB floats row by row from the bottom; a negative scale is little-endian.
fn pfm(b: &[u8]) -> Option<Equirect> {
    let mut fields = Vec::new();
    let mut i = 0;
    while fields.len() < 4 {
        while i < b.len() && b[i].is_ascii_whitespace() {
            i += 1;
        }
        let s = i;
        while i < b.len() && !b[i].is_ascii_whitespace() {
            i += 1;
        }
        fields.push(std::str::from_utf8(&b[s..i]).ok()?.to_string());
    }
    i += 1;
    if fields[0] != "PF" {
        return None;
    }
    let (w, h): (usize, usize) = (fields[1].parse().ok()?, fields[2].parse().ok()?);
    let le = fields[3].parse::<f32>().ok()? < 0.0;
    let data = b.get(i..i + w * h * 12)?;
    let f = |k: usize| {
        let x = [data[k * 4], data[k * 4 + 1], data[k * 4 + 2], data[k * 4 + 3]];
        if le { f32::from_le_bytes(x) } else { f32::from_be_bytes(x) }
    };
    let mut px = vec![[0.0f32; 3]; w * h];
    for y in 0..h {
        for x in 0..w {
            let k = ((h - 1 - y) * w + x) * 3;
            px[y * w + x] = [f(k), f(k + 1), f(k + 2)];
        }
    }
    Some(Equirect { w, h, px })
}

impl Equirect {
    /// The panorama's light along a world direction (Bevy's: +Y up, -Z ahead, +X right), bilinear. The middle of the
    /// picture is straight ahead (-Z) and its right half to the right, as a panorama viewer shows it.
    fn sample(&self, d: Vec3) -> [f32; 3] {
        let d = d.normalize();
        let lon = d.x.atan2(-d.z);
        let u = 0.5 + lon / std::f32::consts::TAU;
        let v = d.y.clamp(-1.0, 1.0).acos() / std::f32::consts::PI;
        let fx = u * self.w as f32 - 0.5;
        let fy = (v * self.h as f32 - 0.5).clamp(0.0, (self.h - 1) as f32);
        let (x0, y0) = (fx.floor(), fy.floor());
        let (tx, ty) = (fx - x0, fy - y0);
        let wrap = |x: f32| (x as i64).rem_euclid(self.w as i64) as usize;
        let (xa, xb) = (wrap(x0), wrap(x0 + 1.0));
        let (ya, yb) = (y0 as usize, (y0 as usize + 1).min(self.h - 1));
        let p = |x: usize, y: usize| self.px[y * self.w + x];
        let mut out = [0.0; 3];
        for k in 0..3 {
            let top = p(xa, ya)[k] * (1.0 - tx) + p(xb, ya)[k] * tx;
            let bot = p(xa, yb)[k] * (1.0 - tx) + p(xb, yb)[k] * tx;
            out[k] = top * (1.0 - ty) + bot * ty;
        }
        out
    }
}

/// An f32 as IEEE half (round to nearest; above 65504 clamps, below the normal range flushes to 0).
fn half(x: f32) -> u16 {
    let x = if x.is_finite() { x.clamp(-65504.0, 65504.0) } else { 0.0 };
    let b = x.to_bits();
    let sign = ((b >> 16) & 0x8000) as u16;
    let exp = ((b >> 23) & 0xff) as i32 - 127 + 15;
    if exp <= 0 {
        return sign;
    }
    let mant = b & 0x7f_ffff;
    let mut h = ((exp as u32) << 10) | (mant >> 13);
    if mant & 0x1000 != 0 && (mant & 0x2fff) != 0 {
        h += 1;
    }
    sign | (h.min(0x7bff) as u16)
}

/// A panorama as a cubemap of `size` (a power of two) a face, Rgba16Float, in wgpu's face order (+X -X +Y -Y +Z -Z).
/// Bevy samples cubemaps with z negated (they're left-handed), so a texel's world direction is its face direction
/// with z flipped. Each texel averages four samples.
fn cubemap(eq: &Equirect, size: u32) -> Image {
    let n = size as usize;
    let mut data = Vec::with_capacity(6 * n * n * 8);
    for face in 0..6 {
        for y in 0..n {
            for x in 0..n {
                let mut acc = [0.0f32; 3];
                for (ox, oy) in [(0.25, 0.25), (0.75, 0.25), (0.25, 0.75), (0.75, 0.75)] {
                    let u = 2.0 * (x as f32 + ox) / n as f32 - 1.0;
                    let v = 2.0 * (y as f32 + oy) / n as f32 - 1.0;
                    let c = match face {
                        0 => Vec3::new(1.0, -v, -u),
                        1 => Vec3::new(-1.0, -v, u),
                        2 => Vec3::new(u, 1.0, v),
                        3 => Vec3::new(u, -1.0, -v),
                        4 => Vec3::new(u, -v, 1.0),
                        _ => Vec3::new(-u, -v, -1.0),
                    };
                    let s = eq.sample(Vec3::new(c.x, c.y, -c.z));
                    for k in 0..3 {
                        acc[k] += s[k] * 0.25;
                    }
                }
                for c in [acc[0], acc[1], acc[2], 1.0] {
                    data.extend_from_slice(&half(c).to_le_bytes());
                }
            }
        }
    }
    let mut img = Image::new(
        Extent3d { width: size, height: size, depth_or_array_layers: 6 },
        TextureDimension::D2,
        data,
        TextureFormat::Rgba16Float,
        bevy::asset::RenderAssetUsages::RENDER_WORLD,
    );
    img.texture_view_descriptor = Some(TextureViewDescriptor { dimension: Some(TextureViewDimension::Cube), ..default() });
    img.sampler = bevy::image::ImageSampler::linear();
    img
}

/// The camera's environment from its row: every panorama decoded and made a cubemap (one face size for all: the
/// largest panorama's width / 4, a power of two from 64 to 2048, since the generated light's targets are sized by the
/// first), then the light and the sky set up as the first frame has them (sync_environment moves them per frame).
pub(super) fn environment(world: &mut World, e: Entity, vals: &BTreeMap<String, V>, inserted: &[Tup], dir: &Path) -> Result<(), String> {
    let Some(V::S(list)) = vals.get("env_srcs") else { return Ok(()) };
    let srcs: Vec<String> = serde_json::from_str(list).map_err(|e| format!("camera: env_srcs: {e}"))?;
    if srcs.is_empty() || preview() {
        return Ok(());
    }
    let mut eqs = Vec::new();
    for s in &srcs {
        let path = resolve_src(inserted, s, dir).map_err(|e| format!("Environment: {e}"))?;
        eqs.push(equirect(&path).map_err(|e| format!("Environment: {e}"))?);
    }
    let widest = eqs.iter().map(|q| q.w).max().unwrap_or(1024) as u32;
    let size = (widest / 4).max(64).next_power_of_two().min(2048);
    let maps: Vec<Handle<Image>> = eqs.iter().map(|q| {
        let img = cubemap(q, size);
        world.resource_mut::<Assets<Image>>().add(img)
    }).collect();
    // each panorama's light, filtered once by its own bake
    let mut lights = Vec::new();
    let mut bakes = Vec::new();
    for m in &maps {
        let (diffuse_map, specular_map) = filter_targets(&mut world.resource_mut::<Assets<Image>>(), size);
        let light = EnvironmentMapLight { diffuse_map, specular_map, intensity: 0.0, ..default() };
        bakes.push(world.spawn((EnvBake, GeneratedEnvironmentMapLight { environment_map: m.clone(), intensity: 0.0, ..default() }, light.clone())).id());
        lights.push(light);
    }
    let i = num(vals, "env_i", 0.0).round().max(0.0) as usize;
    let rot = Quat::from_rotation_y(num(vals, "env_rot", 0.0));
    let at = &lights[i.min(lights.len() - 1)];
    world.entity_mut(e).insert((
        EnvironmentMapLight { diffuse_map: at.diffuse_map.clone(), specular_map: at.specular_map.clone(), intensity: if num(vals, "env_i", 0.0) < -0.5 { 0.0 } else { num(vals, "env_k", 1000.0) }, rotation: rot, ..default() },
        EnvMaps(maps.clone(), bakes),
    ));
    if vals.contains_key("sky_k") {
        let k = num(vals, "sky_k", 0.0);
        world.entity_mut(e).insert(Skybox { image: (k > 0.0 && num(vals, "env_i", 0.0) > -0.5).then(|| maps[i.min(maps.len() - 1)].clone()), brightness: k, rotation: rot });
    }
    Ok(())
}

/// Each frame's panorama, light, turn and sky: the camera's light points at the targets of the panorama's bake, so
/// switching the source switches the light that frame.
pub(super) fn sync_environment(frame: Res<FrameRows>, mut q: Query<(Entity, &EnvMaps, &mut EnvironmentMapLight, Option<&mut Skybox>), Without<EnvBake>>, bakes: Query<&EnvironmentMapLight, With<EnvBake>>) {
    for (e, maps, mut light, sky) in &mut q {
        let Some(v) = frame.vals(e) else { continue };
        let v = &v;
        let at = num(v, "env_i", 0.0).round();
        let on = at > -0.5;
        let idx = (at.max(0.0) as usize).min(maps.0.len() - 1);
        let handle = &maps.0[idx];
        let k = if on { num(v, "env_k", 1000.0) } else { 0.0 };
        let rot = Quat::from_rotation_y(num(v, "env_rot", 0.0));
        if let Some(b) = maps.1.get(idx).and_then(|b| bakes.get(*b).ok()) {
            if light.diffuse_map != b.diffuse_map || light.specular_map != b.specular_map {
                light.diffuse_map = b.diffuse_map.clone();
                light.specular_map = b.specular_map.clone();
            }
        }
        if light.intensity != k || light.rotation != rot {
            light.intensity = k;
            light.rotation = rot;
        }
        if let Some(mut s) = sky {
            let b = num(v, "sky_k", 0.0);
            let img = (on && b > 0.0).then(|| handle.clone());
            if s.image != img || s.brightness != b || s.rotation != rot {
                s.image = img;
                s.brightness = b;
                s.rotation = rot;
            }
        }
    }
}
