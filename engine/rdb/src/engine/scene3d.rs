//! The 3D kinds @raea/motion's 3D SDK writes (sdk/motion/docs/bevy-coverage.md), drawn with Bevy: lights of three types,
//! materials past colour, roughness and metalness, glTF models with their animation clips, the camera's
//! post-processing, and the systems that move all of it frame by frame from the rows. Every value a frame draws is a
//! column of the frame's rows (keys, rules), so a frame is a function of the rows, as everywhere in the engine.
//!
//! Columns (all optional; the defaults are what the engine drew before these existed):
//!   camera: x y z look_x look_y look_z fov near far, ambient ambient_color, bloom (intensity, 0 = none)
//!           bloom_threshold, fog_color fog_start fog_end | fog_density, tonemap ('aces' 'agx' 'tony' 'reinhard'
//!           'none'), exposure (EV100), grade_temperature grade_tint grade_saturation, grade_{s,m,h}_{saturation,
//!           contrast,gamma,gain,lift} (shadows, midtones, highlights), vignette vignette_radius vignette_smooth
//!           vignette_color, aberration, vfog (1: volumetric fog) vfog_steps vfog_dither vfog_resolution vfog_ambient vfog_ambient_color, ssao
//!           ('low' 'medium' 'high' 'ultra') ssao_thickness, aa ('smaa' 'fxaa' 'none'; absent: 4x MSAA), lens_*
//!           (Fisheye and LensDistortion: lens.rs), mb_* (MotionBlur: motion_blur.rs)
//!   light:  type ('directional' default, 'point', 'spot'), color, shadows (1/0), volumetric (1: shafts through
//!           fog volumes; turns shadows on), soft (soft shadows, PCSS: a sun's size in metres as seen from the scene,
//!           a point or spot light's radius);
//!           directional: lux dir_x dir_y dir_z;  point and spot: x y z lumens range radius;
//!           spot: dir_x dir_y dir_z outer inner (radians)
//!   mesh:   shape color rough metal, emissive emissive_k, opacity, unlit, clearcoat clearcoat_rough, reflectance, toon
//!           (a MeshToonMaterial, and its keyed sh_r sh_g sh_b rim_k spec_k outline_w: toon.rs),
//!           map (a picture for the base colour), normal_map, rough_map, metal_map, mr_map (green roughness, blue
//!           metalness), ao_map (red), repeat_u repeat_v (every map tiled that many times a face), x y z rot_x rot_y rot_z (radians, YXZ), scale | scale_x scale_y
//!           scale_z, visible, col_r col_g col_b (an animated colour, sRGB 0..1, wins over color)
//!   model:  src (a .glb or .gltf), scene (index), x y z rot_x rot_y rot_z scale | scale_x.., visible, anim (a clip's
//!           name, or its index as a number), anim_t (seconds into the clip), anim_loop (1 = wrap, 0 = hold the end),
//!           toon (MeshToonMaterials over its glTF materials: toon.rs)
//!   fogvolume: x y z rot_x rot_y rot_z scale | scale_x.. (a unit box of air, scaled), density, color | col_r col_g
//!           col_b, absorption, scattering, anisotropy, visible (Bevy draws every fog volume, so a hidden one is
//!           shrunk to nothing)

use super::*;
use bevy::animation::{AnimationPlayer, graph::{AnimationGraph, AnimationGraphHandle, AnimationNodeIndex}};
use bevy::gltf::Gltf;
use bevy::world_serialization::WorldAssetRoot;

/// A model row's glTF: loaded by the asset server, then its scene spawned under the row's entity and its clip set up.
#[derive(Component)]
pub(super) struct Model3d {
    gltf: Handle<Gltf>,
    scene: usize,
    anim: Option<V>,
    state: ModelState,
    src: String,
    /// its row's id (another model's attach names it)
    id: String,
    /// MeshToonMaterials over its materials (toon.rs), applied once its scene's meshes exist
    toon: Vec<super::toon::ToonSpec>,
    toon_wait: u32,
    /// its materials re-covered (surface.rs), before its toon surfaces
    surfaces: Vec<super::surface::Surface>,
}

impl Model3d {
    /// Loaded, spawned and its clip set up.
    pub(super) fn ready(&self) -> bool {
        matches!(self.state, ModelState::Ready { .. })
    }
}

#[derive(PartialEq)]
enum ModelState {
    Loading,
    Spawned(u32),
    Ready { player: Option<(Entity, AnimationNodeIndex, Handle<bevy::animation::AnimationClip>)> },
}

fn flag(vals: &BTreeMap<String, V>, k: &str, d: bool) -> bool {
    vals.get(k).map(V::truthy).unwrap_or(d)
}

fn scale_of(vals: &BTreeMap<String, V>) -> Vec3 {
    let s = num(vals, "scale", 1.0);
    Vec3::new(num(vals, "scale_x", s), num(vals, "scale_y", s), num(vals, "scale_z", s))
}

fn rot_of(vals: &BTreeMap<String, V>) -> Quat {
    Quat::from_euler(EulerRot::YXZ, num(vals, "rot_y", 0.0), num(vals, "rot_x", 0.0), num(vals, "rot_z", 0.0))
}

/// A mesh or model row's transform.
pub(super) fn transform_of(vals: &BTreeMap<String, V>) -> Transform {
    Transform::from_xyz(num(vals, "x", 0.0), num(vals, "y", 0.0), num(vals, "z", 0.0)).with_rotation(rot_of(vals)).with_scale(scale_of(vals))
}

/// Texture coordinates for a Manifold mesh (it has none): each vertex projected along its face's main axis onto the
/// mesh's bounds, so a picture map covers each side of a box once, the right way round seen from outside.
pub(super) fn box_uvs(pos: &[[f32; 3]], nor: &[[f32; 3]]) -> Vec<[f32; 2]> {
    let mut lo = [f32::MAX; 3];
    let mut hi = [f32::MIN; 3];
    for p in pos {
        for k in 0..3 {
            lo[k] = lo[k].min(p[k]);
            hi[k] = hi[k].max(p[k]);
        }
    }
    let span = |k: usize| (hi[k] - lo[k]).max(1e-6);
    let a = |p: &[f32; 3], k: usize| (p[k] - lo[k]) / span(k);
    let b = |p: &[f32; 3], k: usize| (hi[k] - p[k]) / span(k);
    pos.iter().zip(nor).map(|(p, n)| {
        let (ax, ay, az) = (n[0].abs(), n[1].abs(), n[2].abs());
        if ax >= ay && ax >= az {
            if n[0] > 0.0 { [b(p, 2), b(p, 1)] } else { [a(p, 2), b(p, 1)] }
        } else if ay >= az {
            if n[1] > 0.0 { [a(p, 0), a(p, 2)] } else { [a(p, 0), b(p, 2)] }
        } else if n[2] > 0.0 {
            [a(p, 0), b(p, 1)]
        } else {
            [b(p, 0), b(p, 1)]
        }
    }).collect()
}

/// A picture given inline as a data: URL (`data:image/png;base64,...`, `data:image/svg+xml;utf8,<svg ...>`), decoded in
/// memory: a raster through `image`, an SVG drawn by usvg and vello_cpu at its own size (at least 512 px on its long
/// side, at most 4096). Before this a data: URL was read as a file path and failed ("map: .../data:image/svg+xml...: No
/// such file"), the way void's agent first wrote a label into a perfume shot.
pub(super) fn data_picture(src: &str) -> Result<image::RgbaImage, String> {
    use base64::Engine as _;
    let rest = src.strip_prefix("data:").ok_or("not a data: URL")?;
    let (meta, payload) = rest.split_once(',').ok_or("a data: URL with no comma")?;
    let base64 = meta.split(';').any(|p| p.eq_ignore_ascii_case("base64"));
    let mime = meta.split(';').next().unwrap_or("").to_ascii_lowercase();
    let bytes = if base64 {
        base64::engine::general_purpose::STANDARD.decode(payload.trim()).map_err(|e| format!("its base64: {e}"))?
    } else {
        percent_decode(payload)
    };
    let svg = mime == "image/svg+xml" || (mime.is_empty() && bytes.trim_ascii_start().starts_with(b"<svg"));
    if !svg {
        return image::load_from_memory(&bytes).map(|i| i.to_rgba8()).map_err(|e| format!("a {mime} picture: {e}"));
    }
    svg_picture(&bytes)
}

/// An SVG's bytes drawn by usvg and vello_cpu (anyrender), at least 512 px on its long side, at most 4096.
#[cfg(feature = "native")]
fn svg_picture(bytes: &[u8]) -> Result<image::RgbaImage, String> {
    let tree = anyrender_svg::usvg::Tree::from_data(bytes,&anyrender_svg::usvg::Options::default()).map_err(|e| format!("its SVG: {e}"))?;
    let (sw, sh) = (tree.size().width().max(1.0) as f64, tree.size().height().max(1.0) as f64);
    let k = (512.0 / sw.max(sh)).max(1.0).min(4096.0 / sw.max(sh));
    let (w, h) = ((sw * k).round().max(1.0) as u32, (sh * k).round().max(1.0) as u32);
    use anyrender::ImageRenderer as _;
    let mut r = anyrender_vello_cpu::VelloCpuImageRenderer::new(w, h);
    let mut px = vec![0u8; (w * h * 4) as usize];
    r.render(|scene| anyrender_svg::render_svg_tree(scene, &tree, kurbo::Affine::scale(k)), &mut px);
    // vello_cpu paints premultiplied alpha; a texture wants it straight
    for p in px.chunks_exact_mut(4) {
        let a = p[3] as u32;
        if a > 0 && a < 255 {
            for c in &mut p[..3] {
                *c = ((*c as u32 * 255 + a / 2) / a).min(255) as u8;
            }
        }
    }
    image::RgbaImage::from_raw(w, h, px).ok_or_else(|| "the SVG's pixels".to_string())
}

/// Without native the CPU's SVG renderer isn't linked (a browser build can draw an SVG in the page instead).
#[cfg(not(feature = "native"))]
fn svg_picture(_bytes: &[u8]) -> Result<image::RgbaImage, String> {
    Err("an SVG picture: SVG textures are drawn only in the native build".into())
}

fn percent_decode(s: &str) -> Vec<u8> {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' && i + 2 < b.len() {
            if let Some(x) = std::str::from_utf8(&b[i + 1..i + 3]).ok().and_then(|h| u8::from_str_radix(h, 16).ok()) {
                out.push(x);
                i += 3;
                continue;
            }
        }
        out.push(b[i]);
        i += 1;
    }
    out
}

/// A material's maps, made once per load (file or data: URL, colour or data, clamped or repeating): a floor of tiles
/// and its plinths share one texture each. Cleared at each load, so a reload frees the last one's.
#[derive(Resource, Default)]
pub(super) struct Maps(pub(super) HashMap<String, Handle<Image>>);

/// How a map's texels are read: colour (sRGB, the base colour) or data (linear: normals, roughness, occlusion).
#[derive(Clone, Copy, PartialEq)]
enum MapKind {
    Colour,
    Data,
}

/// A picture as a material's texture: a full mip chain (2x2 box averages, in linear light for colour) sampled
/// trilinearly and anisotropically, so a tiled floor going away from the camera doesn't shimmer; repeating when the
/// material tiles its maps, else clamped (a label's edge doesn't bleed into the other side).
fn map_image(pic: image::RgbaImage, kind: MapKind, repeat: bool) -> Image {
    let (w, h) = pic.dimensions();
    let srgb = kind == MapKind::Colour;
    let (data, levels) = mips_rgba8(pic.into_raw(), w, h, srgb);
    let mut img = Image::new(
        bevy::render::render_resource::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
        bevy::render::render_resource::TextureDimension::D2,
        data,
        if srgb { TextureFormat::Rgba8UnormSrgb } else { TextureFormat::Rgba8Unorm },
        bevy::asset::RenderAssetUsages::default(),
    );
    img.texture_descriptor.mip_level_count = levels;
    let address = if repeat { bevy::image::ImageAddressMode::Repeat } else { bevy::image::ImageAddressMode::ClampToEdge };
    img.sampler = bevy::image::ImageSampler::Descriptor(bevy::image::ImageSamplerDescriptor {
        anisotropy_clamp: 16,
        address_mode_u: address,
        address_mode_v: address,
        ..bevy::image::ImageSamplerDescriptor::linear()
    });
    img
}

/// RGBA8 texels with every mip level after them (2x2 box averages; in linear light when `srgb`): the data and the
/// level count.
pub(super) fn mips_rgba8(mut data: Vec<u8>, w: u32, h: u32, srgb: bool) -> (Vec<u8>, u32) {
    let to_lin: Vec<f32> = (0..256).map(|i| {
        let c = i as f32 / 255.0;
        if srgb { if c <= 0.04045 { c / 12.92 } else { ((c + 0.055) / 1.055).powf(2.4) } } else { c }
    }).collect();
    let enc = |l: f32| -> u8 {
        let c = if !srgb { l } else if l <= 0.0031308 { l * 12.92 } else { 1.055 * l.powf(1.0 / 2.4) - 0.055 };
        (c.clamp(0.0, 1.0) * 255.0 + 0.5) as u8
    };
    let mut cur: Vec<[f32; 4]> = data[..(w * h * 4) as usize].chunks_exact(4).map(|p| [to_lin[p[0] as usize], to_lin[p[1] as usize], to_lin[p[2] as usize], p[3] as f32 / 255.0]).collect();
    let (mut cw, mut ch) = (w, h);
    let mut levels = 1;
    while cw > 1 || ch > 1 {
        let (nw, nh) = ((cw / 2).max(1), (ch / 2).max(1));
        let mut next = vec![[0.0f32; 4]; (nw * nh) as usize];
        for y in 0..nh {
            for x in 0..nw {
                let mut acc = [0.0f32; 4];
                for (dx, dy) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
                    let p = cur[((y * 2 + dy).min(ch - 1) * cw + (x * 2 + dx).min(cw - 1)) as usize];
                    for k in 0..4 {
                        acc[k] += p[k] * 0.25;
                    }
                }
                next[(y * nw + x) as usize] = acc;
            }
        }
        for p in &next {
            data.extend_from_slice(&[enc(p[0]), enc(p[1]), enc(p[2]), (p[3].clamp(0.0, 1.0) * 255.0 + 0.5) as u8]);
        }
        cur = next;
        (cw, ch) = (nw, nh);
        levels += 1;
    }
    (data, levels)
}

/// A glTF's textures given their mip chains: Bevy's glTF loader makes none, so a stone's normal map, minified 4-16x
/// on a column across the street, aliased into per-pixel speckle wherever the sun lit it (worst along the terminator,
/// where N.L flips sign texel to texel), and the albedo shimmered as the camera moved. RGBA8 images still in main-world
/// memory only (compressed and float ones keep what they have); sampled trilinearly, 16x anisotropic, keeping the
/// file's address modes. An image two models share is done once.
fn mip_gltf_textures(g: &Gltf, mats: &Assets<bevy::gltf::GltfMaterial>, images: &mut Assets<Image>) {
    for mh in &g.materials {
        let Some(m) = mats.get(mh) else { continue };
        let texs = [&m.base_color_texture, &m.normal_map_texture, &m.metallic_roughness_texture, &m.occlusion_texture, &m.emissive_texture];
        for t in texs.into_iter().flatten() {
            let Some(img) = images.get(t) else { continue };
            let fmt = img.texture_descriptor.format;
            let srgb = fmt == TextureFormat::Rgba8UnormSrgb;
            if img.texture_descriptor.mip_level_count > 1 || !(srgb || fmt == TextureFormat::Rgba8Unorm) || img.data.is_none() || img.texture_descriptor.size.depth_or_array_layers != 1 {
                continue;
            }
            let Some(mut img) = images.get_mut(t) else { continue };
            let (w, h) = (img.texture_descriptor.size.width, img.texture_descriptor.size.height);
            let (data, levels) = mips_rgba8(img.data.take().unwrap_or_default(), w, h, srgb);
            img.data = Some(data);
            img.texture_descriptor.mip_level_count = levels;
            let mut d = match &img.sampler {
                bevy::image::ImageSampler::Descriptor(d) => d.clone(),
                _ => bevy::image::ImageSamplerDescriptor::default(),
            };
            d.mag_filter = bevy::image::ImageFilterMode::Linear;
            d.min_filter = bevy::image::ImageFilterMode::Linear;
            d.mipmap_filter = bevy::image::ImageFilterMode::Linear;
            d.anisotropy_clamp = 16;
            img.sampler = bevy::image::ImageSampler::Descriptor(d);
        }
    }
}

/// A map column's picture, decoded: a file (resolved as every media path is) or a data: URL.
fn map_picture(t: &Tup, inserted: &[Tup], dir: &Path, col: &str, src: &str) -> Result<image::RgbaImage, String> {
    if src.starts_with("data:") {
        return data_picture(src).map_err(|e| format!("mesh {}: {col} (a data: URL): {e}", t.id));
    }
    let path = resolve_src(inserted, src, dir).map_err(|e| format!("mesh {}: {col}: {e}", t.id))?;
    Ok(image::open(&path).map_err(|e| format!("mesh {}: {col}: {}: {e}", t.id, path.display()))?.to_rgba8())
}

/// A map's texture from the load's cache, or made: `make` builds the picture (one file, or two packed into one).
fn cached_map(world: &mut World, key: String, kind: MapKind, repeat: bool, make: impl FnOnce() -> Result<image::RgbaImage, String>) -> Result<Handle<Image>, String> {
    let key = format!("{key}|{}|{}", kind == MapKind::Colour, repeat);
    if let Some(h) = world.get_resource::<Maps>().and_then(|m| m.0.get(&key)) {
        return Ok(h.clone());
    }
    let img = map_image(make()?, kind, repeat);
    let h = world.resource_mut::<Assets<Image>>().add(img);
    world.get_resource_or_insert_with(Maps::default).0.insert(key, h.clone());
    Ok(h)
}

/// Whether a mesh row has any texture map (it then needs texture coordinates; a normal map needs tangents too).
pub(super) fn has_maps(vals: &BTreeMap<String, V>) -> bool {
    ["map", "normal_map", "rough_map", "metal_map", "mr_map", "ao_map"].iter().any(|k| vals.get(*k).is_some_and(V::truthy))
}

/// A mesh row's surface: the standard material from its columns.
pub(super) fn material(world: &mut World, t: &Tup, inserted: &[Tup], dir: &Path) -> Result<StandardMaterial, String> {
    let vals = &t.vals;
    let g = |k: &str, d: f32| num(vals, k, d);
    let mut base = colour(vals.get("color"), [0.8, 0.8, 0.8, 1.0]);
    if vals.contains_key("col_r") {
        base = Color::srgb(g("col_r", 0.8), g("col_g", 0.8), g("col_b", 0.8));
    }
    let opacity = g("opacity", 1.0).clamp(0.0, 1.0);
    base.set_alpha(opacity);
    let em = colour(vals.get("emissive"), [0.0, 0.0, 0.0, 1.0]).to_linear() * g("emissive_k", 1.0);
    let mut m = StandardMaterial {
        base_color: base,
        perceptual_roughness: g("rough", 0.45),
        metallic: g("metal", 0.0),
        emissive: LinearRgba::rgb(em.red, em.green, em.blue),
        unlit: flag(vals, "unlit", false),
        clearcoat: g("clearcoat", 0.0),
        clearcoat_perceptual_roughness: g("clearcoat_rough", 0.5),
        reflectance: g("reflectance", 0.5),
        // see-through only when asked (an opacity column, even at 1, may animate to less)
        alpha_mode: if vals.contains_key("opacity") { AlphaMode::Blend } else { AlphaMode::Opaque },
        ..default()
    };
    // texture maps (Poly Haven's diff, nor_gl, rough, arm, ao), all tiled by repeat_u/repeat_v
    let src = |k: &str| match vals.get(k) { Some(V::S(s)) if !s.is_empty() => Some(s.clone()), _ => None };
    let (ru, rv) = (g("repeat_u", 1.0), g("repeat_v", 1.0));
    let repeat = ru != 1.0 || rv != 1.0;
    if repeat {
        m.uv_transform = bevy::math::Affine2::from_scale(Vec2::new(ru, rv));
    }
    if let Some(s) = src("map") {
        m.base_color_texture = Some(cached_map(world, s.clone(), MapKind::Colour, repeat, || map_picture(t, inserted, dir, "map", &s))?);
    }
    if let Some(s) = src("normal_map") {
        m.normal_map_texture = Some(cached_map(world, s.clone(), MapKind::Data, repeat, || map_picture(t, inserted, dir, "normalMap", &s))?);
    }
    if let Some(s) = src("mr_map") {
        m.metallic_roughness_texture = Some(cached_map(world, s.clone(), MapKind::Data, repeat, || map_picture(t, inserted, dir, "metallicRoughnessMap", &s))?);
    } else if src("rough_map").is_some() || src("metal_map").is_some() {
        // separate greyscale maps packed into glTF's layout: green roughness, blue metalness (white where there's no
        // map, so the factor alone sets it)
        let (rs, ms) = (src("rough_map"), src("metal_map"));
        let key = format!("pack:{}:{}", rs.clone().unwrap_or_default(), ms.clone().unwrap_or_default());
        m.metallic_roughness_texture = Some(cached_map(world, key, MapKind::Data, repeat, || {
            let r = rs.as_ref().map(|s| map_picture(t, inserted, dir, "roughnessMap", s)).transpose()?;
            let mt = ms.as_ref().map(|s| map_picture(t, inserted, dir, "metalnessMap", s)).transpose()?;
            let (w, h) = r.as_ref().or(mt.as_ref()).map(|p| p.dimensions()).unwrap_or((1, 1));
            let at = |p: &Option<image::RgbaImage>, x: u32, y: u32| p.as_ref().map(|p| {
                let (pw, ph) = p.dimensions();
                p.get_pixel(x * pw / w, y * ph / h)[0]
            }).unwrap_or(255);
            Ok(image::RgbaImage::from_fn(w, h, |x, y| image::Rgba([255, at(&r, x, y), at(&mt, x, y), 255])))
        })?);
    }
    if let Some(s) = src("ao_map") {
        m.occlusion_texture = Some(cached_map(world, s.clone(), MapKind::Data, repeat, || map_picture(t, inserted, dir, "aoMap", &s))?);
    }
    Ok(m)
}

/// Whether a light casts shadows when it's lit (its row's `shadows`, or `volumetric`), kept apart from its
/// `shadow_maps_enabled`, which sync_lights clears while the light is dark.
#[derive(Component)]
pub(super) struct CastsShadows(pub bool);

/// A light row: directional (the default, as before), point or spot.
pub(super) fn light(world: &mut World, e: Entity, vals: &BTreeMap<String, V>) {
    let g = |k: &str, d: f32| num(vals, k, d);
    let color = colour(vals.get("color"), [1.0, 1.0, 1.0, 1.0]);
    let kind = match vals.get("type") { Some(V::S(s)) => s.as_str(), _ => "directional" };
    let pos = Vec3::new(g("x", 0.0), g("y", 0.0), g("z", 0.0));
    let dir = Vec3::new(g("dir_x", -0.5), g("dir_y", -1.0), g("dir_z", -0.4));
    // a light seen in the air (volumetric) is marched through its shadow map, so it casts shadows
    let volumetric = flag(vals, "volumetric", false) && !preview();
    // shadows by default for the sun, not for point or spot lights; sync_lights turns them off while the light is dark
    let directional = kind != "point" && kind != "spot";
    world.entity_mut(e).insert(CastsShadows((flag(vals, "shadows", directional) || volumetric) && !preview()));
    match kind {
        "point" => {
            world.entity_mut(e).insert((
                PointLight { color, intensity: g("lumens", 1.0e6), range: g("range", 20.0), radius: g("radius", 0.0), shadow_maps_enabled: (flag(vals, "shadows", false) || volumetric) && !preview(), ..default() },
                Transform::from_translation(pos),
            ));
        }
        "spot" => {
            let outer = g("outer", std::f32::consts::FRAC_PI_4);
            world.entity_mut(e).insert((
                SpotLight { color, intensity: g("lumens", 1.0e6), range: g("range", 20.0), radius: g("radius", 0.0), outer_angle: outer, inner_angle: g("inner", 0.0).min(outer), shadow_maps_enabled: (flag(vals, "shadows", false) || volumetric) && !preview(), ..default() },
                Transform::from_translation(pos).looking_to(dir, up_for(dir)),
            ));
        }
        _ => {
            // cascades fitted to a film's scale, not Bevy's kilometre: the first ends at 8 m, the last at 150 m, and
            // neighbours overlap by 0.3 so the seam between them doesn't show (a <Shadows> changes them)
            let cascades = bevy::light::CascadeShadowConfigBuilder {
                num_cascades: g("shadow_cascades", 4.0).clamp(1.0, 4.0) as usize,
                minimum_distance: 0.1,
                maximum_distance: g("shadow_dist", 150.0),
                first_cascade_far_bound: g("shadow_first", 8.0).min(g("shadow_dist", 150.0)),
                overlap_proportion: g("shadow_overlap", 0.3),
            }
            .build();
            world.entity_mut(e).insert((
                DirectionalLight { color, illuminance: g("lux", 8000.0), shadow_maps_enabled: (flag(vals, "shadows", true) || volumetric) && !preview(), ..default() },
                cascades,
                Transform::default().looking_to(dir, up_for(dir)),
            ));
        }
    }
    // a <Shadows>' biases, on every light (Bevy's defaults otherwise: 0.02 / 1.8 for the sun and spots, 0.08 / 0.6
    // for point lights)
    let (b, nb) = (vals.get("shadow_bias").map(|v| v.num() as f32), vals.get("shadow_nbias").map(|v| v.num() as f32));
    if b.is_some() || nb.is_some() {
        let mut em = world.entity_mut(e);
        if let Some(mut l) = em.get_mut::<DirectionalLight>() {
            l.shadow_depth_bias = b.unwrap_or(l.shadow_depth_bias);
            l.shadow_normal_bias = nb.unwrap_or(l.shadow_normal_bias);
        }
        if let Some(mut l) = em.get_mut::<SpotLight>() {
            l.shadow_depth_bias = b.unwrap_or(l.shadow_depth_bias);
            l.shadow_normal_bias = nb.unwrap_or(l.shadow_normal_bias);
        }
        if let Some(mut l) = em.get_mut::<PointLight>() {
            l.shadow_depth_bias = b.unwrap_or(l.shadow_depth_bias);
            l.shadow_normal_bias = nb.unwrap_or(l.shadow_normal_bias);
        }
    }
    // soft shadows (PCSS): a sun's size as seen from the scene, or a point or spot light's radius; the shadow's
    // penumbra widens with the distance from what casts it, as a real one's does
    if let Some(soft) = vals.get("soft").map(|v| v.num() as f32).filter(|s| *s > 0.0 && !preview()) {
        let mut em = world.entity_mut(e);
        if let Some(mut l) = em.get_mut::<DirectionalLight>() {
            l.soft_shadow_size = Some(soft);
        }
        if let Some(mut l) = em.get_mut::<SpotLight>() {
            l.radius = soft;
            l.soft_shadows_enabled = true;
        }
        if let Some(mut l) = em.get_mut::<PointLight>() {
            l.radius = soft;
            l.soft_shadows_enabled = true;
        }
    }
    if volumetric {
        world.entity_mut(e).insert(bevy::light::VolumetricLight);
    }
}

/// A fog volume row: a unit box of air (Bevy's FogVolume), placed and sized by its transform.
pub(super) fn fog_volume(world: &mut World, e: Entity, id: &str, vals: &BTreeMap<String, V>) {
    let mut fv = bevy::light::FogVolume::default();
    fog_values(&mut fv, vals);
    world.entity_mut(e).insert((fv, fog_transform(vals), FogRank(id.to_string()), FogThinning(num(vals, "falloff", 0.0).max(0.0))));
}

/// How fast a fog volume thins going up, per metre from its box's floor (`falloff`; 0: even). Only the froxel grid
/// draws it (froxel.rs); Bevy's march fills the box evenly.
#[derive(Component, Clone, Copy)]
pub(super) struct FogThinning(pub(super) f32);

/// Where a fog volume comes in the order Bevy composites them (its row's id). Overlapping volumes blend in the order
/// Bevy's render world iterates them, which is the order their render entities first got a FogVolume: a render of
/// frame 59 alone and a play that reached it from frame 0 got them in different orders (as did a load that spawned
/// one more resource or readback entity), so the overlap of two volumes differed between them. order_fog_volumes
/// re-inserts them in this order every frame, so it's the same whatever came before.
#[derive(Component, Clone)]
pub(super) struct FogRank(pub(super) String);

/// The render world's fog volumes, by rank (filled each extraction).
#[derive(Resource, Default)]
pub(super) struct FogOrder(Vec<(String, Entity)>);

pub(super) fn extract_fog_order(q: bevy::render::Extract<Query<(&bevy::render::sync_world::RenderEntity, &FogRank)>>, mut order: ResMut<FogOrder>) {
    order.0.clear();
    order.0.extend(q.iter().map(|(r, k)| (k.0.clone(), r.id())));
}

/// Takes every fog volume's FogVolume off its render entity and puts them back in rank order, so their table (the
/// order Bevy's prepare_volumetric_fog_uniforms draws them in) is in rank order. Runs before RenderSystems::Prepare.
pub(super) fn order_fog_volumes(world: &mut World) {
    let mut order = std::mem::take(&mut world.resource_mut::<FogOrder>().0);
    if order.len() < 2 {
        return;
    }
    order.sort();
    let mut taken = Vec::with_capacity(order.len());
    for (_, e) in &order {
        if let Ok(mut em) = world.get_entity_mut(*e) {
            if let Some(f) = em.take::<bevy::light::FogVolume>() {
                taken.push((*e, f));
            }
        }
    }
    for (e, f) in taken {
        world.entity_mut(e).insert(f);
    }
}

/// A fog volume's transform, shrunk to nothing while it's hidden: Bevy draws every FogVolume whatever its Visibility,
/// and lights the whole box with the camera's ambient whatever its density.
fn fog_transform(v: &BTreeMap<String, V>) -> Transform {
    let t = transform_of(v);
    if v.get("visible").is_none_or(V::truthy) { t } else { t.with_scale(Vec3::ZERO) }
}

/// A fog volume's surface values from its row: density (0 while hidden), colour, absorption, scattering, anisotropy.
fn fog_values(fv: &mut bevy::light::FogVolume, v: &BTreeMap<String, V>) {
    let shown = v.get("visible").is_none_or(V::truthy);
    fv.density_factor = if shown { num(v, "density", 0.1).max(0.0) } else { 0.0 };
    fv.fog_color = if v.contains_key("col_r") {
        Color::srgb(num(v, "col_r", 1.0), num(v, "col_g", 1.0), num(v, "col_b", 1.0))
    } else {
        colour(v.get("color"), [1.0, 1.0, 1.0, 1.0])
    };
    fv.absorption = num(v, "absorption", 0.3);
    fv.scattering = num(v, "scattering", 0.3);
    fv.scattering_asymmetry = num(v, "anisotropy", 0.5).clamp(-0.99, 0.99);
}

/// Fog volumes where the frame's rows put them, as thick and as coloured as they say.
pub(super) fn sync_fog_volumes(frame: Res<FrameRows>, mut q: Query<(Entity, &mut Transform, &mut bevy::light::FogVolume)>) {
    for (e, mut t, mut fv) in &mut q {
        let Some(v) = frame.vals(e) else { continue };
        let v = &v;
        let nt = fog_transform(v);
        if *t != nt {
            *t = nt;
        }
        let mut next = fv.clone();
        fog_values(&mut next, v);
        if next.density_factor != fv.density_factor || next.fog_color != fv.fog_color || next.absorption != fv.absorption || next.scattering != fv.scattering || next.scattering_asymmetry != fv.scattering_asymmetry {
            *fv = next;
        }
    }
}

/// An up vector that isn't along `dir` (a light pointing straight down).
fn up_for(dir: Vec3) -> Vec3 {
    if dir.normalize_or_zero().cross(Vec3::Y).length_squared() < 1e-6 { Vec3::Z } else { Vec3::Y }
}

/// The camera row's lens and post-processing.
pub(super) fn camera(world: &mut World, e: Entity, vals: &BTreeMap<String, V>) {
    let g = |k: &str, d: f32| num(vals, k, d);
    // shadow quality (a <Shadows>): the sun's and spots' maps are 4096 texels a side unless it says otherwise (Bevy's
    // 2048 speckles and blurs a film's shadows); point lights' cube maps half that, 1024 by default as Bevy's
    if !preview() {
        let size = match vals.get("shadow_map") { Some(V::N(n)) => *n as usize, _ => 4096 };
        if let Some(mut m) = world.get_resource_mut::<bevy::light::DirectionalLightShadowMap>() {
            m.size = size;
        }
        if let Some(mut m) = world.get_resource_mut::<bevy::light::PointLightShadowMap>() {
            m.size = if vals.contains_key("shadow_map") { size / 2 } else { 1024 };
        }
    }
    let filter = match vals.get("shadow_filter") { Some(V::S(s)) if s == "hardware" => bevy::light::ShadowFilteringMethod::Hardware2x2, _ => bevy::light::ShadowFilteringMethod::Gaussian };
    world.entity_mut(e).insert(filter);
    if vals.contains_key("near") || vals.contains_key("far") {
        let fov = g("fov", 45.0).to_radians();
        world.entity_mut(e).insert(Projection::Perspective(PerspectiveProjection { fov, near: g("near", 0.1), far: g("far", 1000.0), ..default() }));
    }
    let bloom = g("bloom", 0.0);
    if bloom > 0.0 {
        let mut b = bevy::post_process::bloom::Bloom::NATURAL;
        b.intensity = bloom;
        if let Some(th) = vals.get("bloom_threshold").map(V::num) {
            b.prefilter.threshold = th as f32;
            b.prefilter.threshold_softness = 0.2;
        }
        world.entity_mut(e).insert(b);
    }
    if vals.contains_key("fog_color") || vals.contains_key("fog_end") || vals.contains_key("fog_density") {
        let falloff = match vals.get("fog_density") {
            Some(d) => FogFalloff::Exponential { density: d.num() as f32 },
            None => FogFalloff::Linear { start: g("fog_start", 5.0), end: g("fog_end", 20.0) },
        };
        world.entity_mut(e).insert(DistanceFog { color: colour(vals.get("fog_color"), [0.5, 0.5, 0.55, 1.0]), falloff, ..default() });
    }
    if let Some(f) = vals.get("dof_aperture").map(V::num) {
        use bevy::post_process::dof::{DepthOfField, DepthOfFieldMode};
        let mode = if matches!(vals.get("dof_mode"), Some(V::S(m)) if m == "gaussian") { DepthOfFieldMode::Gaussian } else { DepthOfFieldMode::Bokeh };
        // the background counts as far as the camera sees, not infinitely far (its blur would be the largest)
        world.entity_mut(e).insert(DepthOfField { mode, focal_distance: g("focus", 5.0), aperture_f_stops: f as f32, max_circle_of_confusion_diameter: g("dof_max", 64.0), max_depth: g("far", 100.0), ..default() });
    }
    if let Some(V::S(tm)) = vals.get("tonemap") {
        use bevy::core_pipeline::tonemapping::Tonemapping as T;
        let t = match tm.as_str() {
            "none" => T::None,
            "reinhard" => T::Reinhard,
            "aces" => T::AcesFitted,
            "agx" => T::AgX,
            "neutral" | "blender" => T::BlenderFilmic,
            _ => T::TonyMcMapface,
        };
        world.entity_mut(e).insert(t);
    }
    if let Some(ev) = vals.get("exposure") {
        world.entity_mut(e).insert(bevy::camera::Exposure { ev100: ev.num() as f32 });
    }
    // a <Grade>: Bevy's colour grading (before tone mapping, but for the global saturation), vignette and fringes
    if vals.keys().any(|k| k.starts_with("grade_")) {
        use bevy::render::view::{ColorGrading, ColorGradingSection};
        let mut cg = ColorGrading::default();
        cg.global.temperature = g("grade_temperature", 0.0);
        cg.global.tint = g("grade_tint", 0.0);
        cg.global.post_saturation = g("grade_saturation", 1.0);
        for (pre, sec) in [("grade_s_", &mut cg.shadows), ("grade_m_", &mut cg.midtones), ("grade_h_", &mut cg.highlights)] {
            let d = ColorGradingSection::default();
            *sec = ColorGradingSection {
                saturation: g(&format!("{pre}saturation"), d.saturation),
                contrast: g(&format!("{pre}contrast"), d.contrast),
                gamma: g(&format!("{pre}gamma"), d.gamma),
                gain: g(&format!("{pre}gain"), d.gain),
                lift: g(&format!("{pre}lift"), d.lift),
            };
        }
        world.entity_mut(e).insert(cg);
    }
    if let Some(v) = vals.get("vignette").map(V::num) {
        use bevy::post_process::effect_stack::Vignette;
        let d = Vignette::default();
        world.entity_mut(e).insert(Vignette {
            intensity: v as f32,
            radius: g("vignette_radius", d.radius),
            smoothness: g("vignette_smooth", d.smoothness),
            color: colour(vals.get("vignette_color"), [0.0, 0.0, 0.0, 1.0]),
            ..d
        });
    }
    // ink outlines from the depth and normal prepasses (Msaa::Off, set where the camera is made)
    if vals.contains_key("outline") {
        let line = colour(vals.get("outline"), [0.0, 0.0, 0.0, 1.0]).to_linear().to_f32_array();
        world.entity_mut(e).insert((
            super::outline::InkOutline { color: Vec4::from_array(line), depth: g("outline_depth", 0.04), normal: g("outline_normal", 0.3), width: g("outline_width", 1.0).max(1.0) },
            bevy::core_pipeline::prepass::DepthPrepass,
            bevy::core_pipeline::prepass::NormalPrepass,
        ));
    }
    // ambient occlusion (Msaa::Off, set where the camera is made; its noise is a fixed pattern without TAA's jitter, so
    // a frame's occlusion is a function of its rows) and the anti-aliasing that replaces multisampling
    if let Some(V::S(q)) = vals.get("ssao").filter(|_| !preview()) {
        use bevy::pbr::{ScreenSpaceAmbientOcclusion, ScreenSpaceAmbientOcclusionQualityLevel as Q};
        let quality_level = match q.as_str() { "low" => Q::Low, "high" => Q::High, "ultra" => Q::Ultra, _ => Q::Medium };
        world.entity_mut(e).insert(ScreenSpaceAmbientOcclusion { quality_level, constant_object_thickness: g("ssao_thickness", 0.25) });
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
    // volumetric fog: light in the air inside each fog volume. Its jitter is raea's dither (install_fog_shader): 0
    // here, and with `vfog_dither` set each frame by sync_fog_dither from the film frame, so a frame's fog is a
    // function of the frame either way
    // (none when drawing a pass: passes.rs)
    if vals.get("vfog").is_some_and(V::truthy) && super::passes::wanted().is_none() {
        // RAEA_FOG_REFERENCE=1: the fog's ground truth (sdk/motion/bench/bevy/fog-score.py scores against it): Bevy's
        // march at 512 steps (RAEA_FOG_REFERENCE_STEPS), undithered, at full resolution, whatever the composition asks
        // for. Converged by then (the rome curia: 256 against 512 SSIM 0.99988); 1024 at 1080p runs past what Metal
        // lets one pass take and the fog pass is dropped without an error, the frame drawn with no fog at all
        let reference = fog_reference();
        let fog = bevy::light::VolumetricFog {
            step_count: if reference { std::env::var("RAEA_FOG_REFERENCE_STEPS").ok().and_then(|v| v.parse().ok()).unwrap_or(512) } else { g("vfog_steps", 64.0).round().max(1.0) as u32 },
            ambient_intensity: g("vfog_ambient", 0.0),
            ambient_color: colour(vals.get("vfog_ambient_color"), [1.0, 1.0, 1.0, 1.0]),
            jitter: 0.0,
        };
        // every volume in one grid along the view (froxel.rs), unless vfog_mode is march (or RAEA_FOG_MODE=march, for
        // comparing): Bevy's march, where vfog_resolution below 1 marches at that fraction of the frame, upsampled by
        // depth (fog_half.rs). The grid scored at or above the march on every scene fog-score.py judges (the
        // fixtures, the rome film's plot and curia, Mean Streets), in SSIM, PSNR, ms a frame and bitrate, so it's the
        // default; RAEA_FOG_MODE=froxel forces it over a march row
        let env_mode = std::env::var("RAEA_FOG_MODE");
        let march = vals.get("vfog_mode").is_some_and(|m| matches!(m, V::S(s) if s == "march")) && env_mode.as_deref() != Ok("froxel");
        let froxel = !reference && !march && env_mode.as_deref() != Ok("march");
        if froxel {
            let lin = fog.ambient_color.to_linear();
            super::froxel::insert(
                world,
                e,
                super::froxel::FroxelFog {
                    ambient: Vec3::new(lin.red, lin.green, lin.blue) * fog.ambient_intensity,
                    slices: g("vfog_slices", 64.0).round().clamp(32.0, 256.0) as u32,
                    tile: g("vfog_tile", 16.0).round().clamp(4.0, 16.0) as u32,
                    samples: g("vfog_samples", 1.0).round().clamp(1.0, 8.0) as u32,
                    // (the ground truth integrates every light itself)
                    hero: g("vfog_hero", 1.0) != 0.0 && std::env::var("RAEA_FOG_HERO").as_deref() != Ok("0") && super::froxel::reference_steps().is_none(),
                },
            );
        } else {
            super::fog_half::insert(world, e, fog, if reference { 1.0 } else { g("vfog_resolution", 1.0) });
            if !reference && vals.get("vfog_dither").is_some_and(V::truthy) {
                world.entity_mut(e).insert(FogDither);
            }
        }
    }
    if let Some(a) = vals.get("aberration").map(V::num) {
        use bevy::post_process::effect_stack::ChromaticAberration;
        world.entity_mut(e).insert(ChromaticAberration { intensity: a as f32, ..default() });
    }
    if super::lens::has_lens(vals) {
        // Fisheye / LensDistortion: its values come from each frame's row (sync_camera)
        world.entity_mut(e).insert(super::lens::Lens::default());
    }
}

/// Whether RAEA_FOG_REFERENCE asks for the fog's ground truth instead of what the composition asks for.
pub(super) fn fog_reference() -> bool {
    std::env::var("RAEA_FOG_REFERENCE").is_ok_and(|v| v == "1")
}

/// raea's volumetric fog shader (volumetric_fog.wgsl: Bevy 0.19.1's, its dither seeded from the film frame) over
/// Bevy's embedded one. The fog pipeline loads its shader when it is first made, after the plugins are built, so the
/// copy registered here is the one it gets.
pub(super) fn install_fog_shader(app: &mut App) {
    let reg = app.world_mut().resource_mut::<bevy::asset::io::embedded::EmbeddedAssetRegistry>();
    let mut bytes: &'static [u8] = include_bytes!("volumetric_fog.wgsl");
    // the reference marches the whole ray (Bevy's stops short off the view axis) and scatters the ambient light by
    // density, as the grid does (RAEA_FOG_REFERENCE_AMBIENT=flat: Bevy's flat ambient)
    if fog_reference() {
        let mut s = std::str::from_utf8(bytes).unwrap().replace("const RAY_LENGTH: bool = false;", "const RAY_LENGTH: bool = true;");
        if std::env::var("RAEA_FOG_REFERENCE_AMBIENT").as_deref() != Ok("flat") {
            s = s.replace("const SCATTERED_AMBIENT: bool = false;", "const SCATTERED_AMBIENT: bool = true;");
        }
        bytes = Box::leak(s.into_bytes().into_boxed_slice());
    }
    reg.insert_asset(std::path::PathBuf::from(file!()), Path::new("bevy_pbr/volumetric_fog/volumetric_fog.wgsl"), bytes);
}

/// A camera whose volumetric fog is dithered (`<VolumetricFog dither>`).
#[derive(Component)]
pub(super) struct FogDither;

/// The dither's seed for this frame: Bevy's `jitter`, which raea's shader reads as 1 + the film frame mod 64 (0 is no
/// dither). The film frame, not the engine's update count, so a frame rendered alone and the same frame reached in
/// play march the same samples.
pub(super) fn sync_fog_dither(
    clock: Res<super::Clock>,
    mut q: Query<&mut bevy::light::VolumetricFog, With<FogDither>>,
    mut half: Query<&mut super::fog_half::HalfFog, With<FogDither>>,
) {
    let seed = 1.0 + clock.0.rem_euclid(64) as f32;
    for mut f in &mut q {
        if f.jitter != seed {
            f.jitter = seed;
        }
    }
    for mut f in &mut half {
        if f.fog.jitter != seed {
            f.fog.jitter = seed;
        }
    }
}

/// A model row: its glTF asked for (the engine waits for it before drawing, `await_models`).
pub(super) fn model(world: &mut World, e: Entity, t: &Tup, inserted: &[Tup], dir: &Path) -> Result<(), String> {
    let src = match t.vals.get("src") { Some(V::S(s)) => s.clone(), _ => return Err(format!("model {}: needs src (a .glb or .gltf file)", t.id)) };
    let path = resolve_src(inserted, &src, dir)?;
    if !path.exists() {
        return Err(format!("model {}: {} doesn't exist", t.id, path.display()));
    }
    let gltf: Handle<Gltf> = world.resource::<AssetServer>().load(path.to_string_lossy().to_string());
    let anim = t.vals.get("anim").cloned().filter(|v| v.truthy() || matches!(v, V::N(_)));
    let toon = super::toon::ToonSpec::of(&t.vals);
    let surfaces = super::surface::of(world, t, inserted, dir)?;
    world.entity_mut(e).insert((Model3d { gltf, scene: num(&t.vals, "scene", 0.0) as usize, anim, state: ModelState::Loading, src, id: t.id.clone(), toon, toon_wait: 0, surfaces }, transform_of(&t.vals), Visibility::default()));
    Ok(())
}

/// Models along: loaded, then their scene spawned, then (a few updates later, once the scene's entities exist) their
/// clip found and set playing, paused: `sync_models` seeks it to each frame's time.
pub(super) fn model_setup(
    mut commands: Commands,
    server: Res<AssetServer>,
    gltfs: Res<Assets<Gltf>>,
    gmeshes: Res<Assets<bevy::gltf::GltfMesh>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut graphs: ResMut<Assets<AnimationGraph>>,
    mut models: Query<(Entity, &mut Model3d)>,
    kids: Query<&Children>,
    mut players: Query<&mut AnimationPlayer>,
    mut err: ResMut<Error>,
    mut images: ResMut<Assets<Image>>,
    mats: Res<Assets<bevy::gltf::GltfMaterial>>,
) {
    for (e, mut m) in &mut models {
        match m.state {
            ModelState::Loading => {
                if let Some(bevy::asset::LoadState::Failed(why)) = server.get_load_state(&m.gltf) {
                    err.0 = Some(format!("model {}: {why}", m.src));
                    m.state = ModelState::Ready { player: None };
                    continue;
                }
                // a texture that fails (a JPEG without the jpeg feature, a missing file) fails the model now, with
                // its reason, rather than leaving it loading until await_models gives up after 60 s
                if let Some(bevy::asset::RecursiveDependencyLoadState::Failed(why)) = server.get_recursive_dependency_load_state(&m.gltf) {
                    err.0 = Some(format!("model {}: {why}", m.src));
                    m.state = ModelState::Ready { player: None };
                    continue;
                }
                if !server.is_loaded_with_dependencies(&m.gltf) {
                    continue;
                }
                let Some(g) = gltfs.get(&m.gltf) else { continue };
                let Some(scene) = g.scenes.get(m.scene).or(g.default_scene.as_ref()).cloned() else {
                    err.0 = Some(format!("model {}: has no scene {}", m.src, m.scene));
                    m.state = ModelState::Ready { player: None };
                    continue;
                };
                normalise_joint_weights(g, &gmeshes, &mut meshes);
                mip_gltf_textures(g, &mats, &mut images);
                let child = commands.spawn((WorldAssetRoot(scene), Transform::default(), Visibility::Inherited)).id();
                commands.entity(e).add_child(child);
                m.state = ModelState::Spawned(0);
            }
            ModelState::Spawned(n) => {
                // the scene's entities, then the player among them (the glTF puts one on the animated root)
                let found = descendants(e, &kids).into_iter().find(|d| players.contains(*d));
                let Some(anim) = m.anim.clone() else {
                    if n > 2 {
                        m.state = ModelState::Ready { player: None };
                    } else {
                        m.state = ModelState::Spawned(n + 1);
                    }
                    continue;
                };
                let Some(p) = found else {
                    if n > 120 {
                        err.0 = Some(format!("model {}: asked for clip {anim:?}, but it has no animated parts", m.src));
                        m.state = ModelState::Ready { player: None };
                    } else {
                        m.state = ModelState::Spawned(n + 1);
                    }
                    continue;
                };
                let Some(g) = gltfs.get(&m.gltf) else { continue };
                let clip = match &anim {
                    V::S(name) => g.named_animations.get(name.as_str()).cloned().ok_or_else(|| {
                        let mut names: Vec<&str> = g.named_animations.keys().map(|k| k.as_ref()).collect();
                        names.sort();
                        format!("model {}: no clip \"{name}\" (it has {})", m.src, names.join(", "))
                    }),
                    V::N(i) => g.animations.get(*i as usize).cloned().ok_or_else(|| format!("model {}: no clip {i} (it has {})", m.src, g.animations.len())),
                };
                let clip = match clip {
                    Ok(c) => c,
                    Err(why) => {
                        err.0 = Some(why);
                        m.state = ModelState::Ready { player: None };
                        continue;
                    }
                };
                let (graph, node) = AnimationGraph::from_clip(clip.clone());
                commands.entity(p).insert(AnimationGraphHandle(graphs.add(graph)));
                if let Ok(mut pl) = players.get_mut(p) {
                    pl.play(node).repeat().pause();
                }
                m.state = ModelState::Ready { player: Some((p, node, clip)) };
            }
            ModelState::Ready { .. } => {}
        }
    }
}

/// Skin weights scaled to sum to 1, as three.js's GLTFLoader does (`normalizeSkinWeights`). glTF requires the sum to
/// be 1 and Bevy draws the weights as written, so a file exported with sums above 1 (RobotExpressive's hands reach
/// 1.75) stretches its skinned parts into spikes. Only meshes whose sums are off are touched.
fn normalise_joint_weights(g: &Gltf, gmeshes: &Assets<bevy::gltf::GltfMesh>, meshes: &mut Assets<Mesh>) {
    use bevy::mesh::VertexAttributeValues;
    let off = |w: &[f32; 4]| {
        let s: f32 = w.iter().sum();
        s > 0.0 && (s - 1.0).abs() > 1e-3
    };
    for gm in g.meshes.iter().filter_map(|h| gmeshes.get(h)) {
        for p in &gm.primitives {
            let needs = meshes.get(&p.mesh).and_then(|m| m.attribute(Mesh::ATTRIBUTE_JOINT_WEIGHT)).is_some_and(|v| matches!(v, VertexAttributeValues::Float32x4(w) if w.iter().any(off)));
            if !needs {
                continue;
            }
            let Some(mut m) = meshes.get_mut(&p.mesh) else { continue };
            if let Ok(Some(VertexAttributeValues::Float32x4(w))) = m.try_attribute_mut_option(Mesh::ATTRIBUTE_JOINT_WEIGHT) {
                for x in w.iter_mut().filter(|x| off(x)) {
                    let s: f32 = x.iter().sum();
                    x.iter_mut().for_each(|c| *c /= s);
                }
            }
        }
    }
}

pub(super) fn descendants(e: Entity, kids: &Query<&Children>) -> Vec<Entity> {
    let mut out = Vec::new();
    let mut stack = vec![e];
    while let Some(x) = stack.pop() {
        if let Ok(c) = kids.get(x) {
            for k in c.iter() {
                out.push(k);
                stack.push(k);
            }
        }
    }
    out
}

/// Models with a MeshToonMaterial: once ready, their meshes' materials swapped for toon ones (toon::apply_to_model),
/// and the spec cleared. A model whose scene shows no meshes for 30 updates after it's ready is left as it is.
pub(super) fn toon_models(world: &mut World) {
    // (and their surfaces re-covered first, surface.rs, in the same wait)
    let mut q = world.query::<(Entity, &mut Model3d)>();
    let todo: Vec<(Entity, Vec<super::surface::Surface>)> = q.iter_mut(world).filter(|(_, m)| !m.surfaces.is_empty() && matches!(m.state, ModelState::Ready { .. })).map(|(e, m)| (e, m.surfaces.clone())).collect();
    for (e, specs) in todo {
        let done = super::surface::apply_to_model(world, e, &specs);
        if let Some(mut m) = world.get_mut::<Model3d>(e) {
            m.toon_wait += 1;
            if done || m.toon_wait > 30 {
                m.surfaces.clear();
                m.toon_wait = 0;
            }
        }
    }
    let mut q = world.query::<(Entity, &mut Model3d)>();
    let todo: Vec<(Entity, Vec<super::toon::ToonSpec>)> = q.iter_mut(world).filter(|(_, m)| !m.toon.is_empty() && matches!(m.state, ModelState::Ready { .. })).map(|(e, m)| (e, m.toon.clone())).collect();
    for (e, specs) in todo {
        let done = super::toon::apply_to_model(world, e, &specs);
        if let Some(mut m) = world.get_mut::<Model3d>(e) {
            m.toon_wait += 1;
            if done || m.toon_wait > 30 {
                m.toon.clear();
            }
        }
    }
}

/// Every model ready (loaded, spawned, its clip set up, its toon surfaces on) before the first frame is drawn: a frame
/// never shows a model half-loaded. At most 60 s.
pub(super) fn await_models(app: &mut App) -> Result<(), String> {
    let t0 = std::time::Instant::now();
    loop {
        let world = app.world_mut();
        if let Some(e) = world.resource_mut::<Error>().0.take() {
            return Err(e);
        }
        toon_models(world);
        let mut q = world.query::<&Model3d>();
        let waiting = q.iter(world).filter(|m| !matches!(m.state, ModelState::Ready { .. }) || !m.toon.is_empty() || !m.surfaces.is_empty()).count();
        if waiting == 0 {
            return Ok(());
        }
        if t0.elapsed().as_secs_f64() > 60.0 {
            return Err(format!("{waiting} model(s) didn't load in 60 s"));
        }
        app.update();
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
}

/// Meshes and models where the frame's rows put them: transform, whether they show, and the material values a row
/// animates. Only columns a row has are applied, so rows written before these columns draw as they did.
#[allow(clippy::type_complexity)]
pub(super) fn sync_meshes(
    frame: Res<FrameRows>,
    mut mats: ResMut<Assets<StandardMaterial>>,
    mut glasses: ResMut<Assets<super::froxel_glass::GlassMaterial>>,
    mut q: Query<
        (
            Entity,
            &mut Transform,
            &mut Visibility,
            Option<&MeshMaterial3d<StandardMaterial>>,
            Option<(&super::froxel_glass::Fogged, &MeshMaterial3d<super::froxel_glass::GlassMaterial>)>,
            Has<super::plugins::Driven>,
            Option<&super::paint::Painted>,
            Option<&super::decal::DecalStd>,
        ),
        Or<(With<Mesh3d>, With<Model3d>)>,
    >,
) {
    for (e, mut t, mut vis, mat, fogged, driven, painted, decal) in &mut q {
        let Some(v) = frame.vals(e) else { continue };
        let v = &v;
        // a mesh a Bevy plugin moves (a rigid body) keeps the plugin's transform (engine/plugins.rs)
        let nt = transform_of(v);
        if *t != nt && !driven {
            *t = nt;
        }
        if let Some(on) = v.get("visible") {
            let want = if on.truthy() { Visibility::Inherited } else { Visibility::Hidden };
            if *vis != want {
                *vis = want;
            }
        }
        // a painted mesh's standard material is kept aside for this (paint.rs), and a see-through mesh drawn with its froxel
        // fog copy (froxel_glass.rs) follows its standard material
        // (and a decal's, decal.rs)
        let Some(h) = mat.map(|m| &m.0).or(painted.map(|p| &p.std)).or(fogged.map(|(f, _)| &f.std)).or(decal.map(|d| &d.0)) else { continue };
        let animated = ["col_r", "opacity", "emissive_k", "rough", "metal"].iter().any(|k| v.contains_key(*k));
        if !animated {
            continue;
        }
        let Some(cur) = mats.get(h) else { continue };
        let mut base = cur.base_color;
        if v.contains_key("col_r") {
            base = Color::srgb(num(v, "col_r", 0.8), num(v, "col_g", 0.8), num(v, "col_b", 0.8));
        }
        if v.contains_key("opacity") {
            base.set_alpha(num(v, "opacity", 1.0).clamp(0.0, 1.0));
        } else {
            base.set_alpha(cur.base_color.alpha());
        }
        let rough = if v.contains_key("rough") { num(v, "rough", 0.45) } else { cur.perceptual_roughness };
        let metal = if v.contains_key("metal") { num(v, "metal", 0.0) } else { cur.metallic };
        let emissive = if v.contains_key("emissive_k") {
            let em = colour(v.get("emissive"), [0.0, 0.0, 0.0, 1.0]).to_linear() * num(v, "emissive_k", 1.0);
            LinearRgba::rgb(em.red, em.green, em.blue)
        } else {
            cur.emissive
        };
        // a mesh made see-through after it was spawned (a live scene's row gains opacity mid-play) blends from then on,
        // as a render's, which knew from its load (material: an opacity column is a blended surface)
        let blend = v.contains_key("opacity") && cur.alpha_mode == AlphaMode::Opaque;
        if cur.base_color != base || cur.perceptual_roughness != rough || cur.metallic != metal || cur.emissive != emissive || blend {
            if let Some(mut m) = mats.get_mut(h) {
                m.base_color = base;
                m.perceptual_roughness = rough;
                m.metallic = metal;
                m.emissive = emissive;
                if blend {
                    m.alpha_mode = AlphaMode::Blend;
                }
                // and its fog copy, this frame (froxel_glass.rs's own follow is a frame behind)
                if let Some(mut g) = fogged.and_then(|(_, g)| glasses.get_mut(&g.0)) {
                    g.base = m.clone();
                }
            }
        }
    }
}

/// The camera where the frame's row puts it, looking where it says; its field of view and ambient light.
#[allow(clippy::type_complexity)]
pub(super) fn sync_camera(frame: Res<FrameRows>, edit: Option<Res<Edit>>, mut q: Query<(Entity, &mut Transform, Option<&mut Projection>, Option<&mut AmbientLight>, Option<&mut bevy::post_process::dof::DepthOfField>, Option<&mut super::lens::Lens>), With<Camera3d>>) {
    for (e, mut t, proj, amb, dof, lens) in &mut q {
        let Some(v) = frame.vals(e) else { continue };
        let v = &v;
        let pos = Vec3::new(num(v, "x", 0.0), num(v, "y", 2.0), num(v, "z", 9.0));
        let look = Vec3::new(num(v, "look_x", 0.0), num(v, "look_y", 1.0), num(v, "look_z", 0.0));
        if (look - pos).length_squared() > 1e-12 {
            let nt = Transform::from_translation(pos).looking_at(look, up_for(look - pos));
            if *t != nt {
                *t = nt;
            }
        }
        let mut seen = None;
        if let Some(mut p) = proj {
            if let Projection::Perspective(pp) = p.as_mut() {
                if let Some(fov) = v.get("fov") {
                    // a fisheye with its own fov widens the render by its strength (lens.rs)
                    let f = super::lens::render_fov(v, (fov.num() as f32).to_radians());
                    if pp.fov != f {
                        pp.fov = f;
                    }
                }
                // the target's own shape (the projection's aspect is only updated after this, in PostUpdate)
                let aspect = edit.as_ref().map(|e| e.prog.width as f32 / e.prog.height.max(1) as f32).unwrap_or(pp.aspect_ratio);
                seen = Some((pp.fov, aspect));
            }
        }
        if let (Some(mut l), Some((fov, aspect))) = (lens, seen) {
            let want = super::lens::lens_of(v, fov, aspect);
            if *l != want {
                *l = want;
            }
        }
        if let (Some(d), Some(mut dof)) = (v.get("focus"), dof) {
            let d = d.num() as f32;
            if dof.focal_distance != d {
                dof.focal_distance = d;
            }
        }
        if let (Some(b), Some(mut a)) = (v.get("ambient"), amb) {
            if a.brightness != b.num() as f32 {
                a.brightness = b.num() as f32;
            }
        }
    }
}

/// Lights where the frame's rows put them, as bright as they say.
pub(super) fn sync_lights(frame: Res<FrameRows>, mut q: Query<(Entity, &mut Transform, Option<&mut DirectionalLight>, Option<&mut PointLight>, Option<&mut SpotLight>, Option<&CastsShadows>)>) {
    // a dark light (a frame it isn't in: its power is 0) has its shadow maps off, so no shadow pass is drawn for it. Lit or dark,
    // every light stays shown: hiding the dark ones (so Bevy doesn't extract them) put the lit volumetric spots' shafts
    // out, as Bevy 0.19's prepare_lights indexes its spot lights past a count of point lights that the hidden ones
    // threw off. Dark with its maps on, a film's every shadow-casting light drew them on every frame of every scene
    // (the rome film: 20 lights, 4096-texel maps; 2026-10-20)
    for (e, mut t, d, p, s, casts) in &mut q {
        if d.is_none() && p.is_none() && s.is_none() {
            continue;
        }
        let Some(v) = frame.vals(e) else { continue };
        let v = &v;
        let pos = Vec3::new(num(v, "x", 0.0), num(v, "y", 0.0), num(v, "z", 0.0));
        let dir = Vec3::new(num(v, "dir_x", -0.5), num(v, "dir_y", -1.0), num(v, "dir_z", -0.4));
        let nt = if p.is_some() {
            Transform::from_translation(pos)
        } else if s.is_some() {
            Transform::from_translation(pos).looking_to(dir, up_for(dir))
        } else {
            Transform::default().looking_to(dir, up_for(dir))
        };
        if *t != nt {
            *t = nt;
        }
        let shadows = |power: f32, now: bool| casts.map_or(now, |c| c.0 && power > 0.0);
        if let Some(mut d) = d {
            let lux = num(v, "lux", 8000.0);
            if d.illuminance != lux {
                d.illuminance = lux;
            }
            let sh = shadows(lux, d.shadow_maps_enabled);
            if d.shadow_maps_enabled != sh {
                d.shadow_maps_enabled = sh;
            }
        }
        if let Some(mut p) = p {
            let lm = num(v, "lumens", 1.0e6);
            if p.intensity != lm {
                p.intensity = lm;
            }
            let sh = shadows(lm, p.shadow_maps_enabled);
            if p.shadow_maps_enabled != sh {
                p.shadow_maps_enabled = sh;
            }
        }
        if let Some(mut s) = s {
            let lm = num(v, "lumens", 1.0e6);
            if s.intensity != lm {
                s.intensity = lm;
            }
            let sh = shadows(lm, s.shadow_maps_enabled);
            if s.shadow_maps_enabled != sh {
                s.shadow_maps_enabled = sh;
            }
        }
    }
}

/// A model attached to another's bone (attach_to: the other's row id, attach_bone: the bone's node name): once both
/// are ready, made a child of the bone, so it follows the bone's animated pose and its own placement is the bone's
/// local space (a hat on a walking man's head). Set once.
#[derive(Component)]
pub(super) struct Attached;

pub(super) fn sync_attach(
    mut commands: Commands,
    frame: Res<FrameRows>,
    models: Query<(Entity, &Model3d, Has<Attached>)>,
    names: Query<&Name>,
    kids: Query<&Children>,
    mut err: ResMut<Error>,
) {
    for (e, m, done) in &models {
        if done || !matches!(m.state, ModelState::Ready { .. }) {
            continue;
        }
        let Some(v) = frame.vals(e) else { continue };
        let Some(V::S(to)) = v.get("attach_to") else { continue };
        let Some((target, tm, _)) = models.iter().find(|(_, o, _)| &o.id == to) else {
            err.0 = Some(format!("model {}: attached to {to}, which isn't a model", m.src));
            continue;
        };
        if !matches!(tm.state, ModelState::Ready { .. }) {
            continue;
        }
        let bone = match v.get("attach_bone") { Some(V::S(b)) => b.clone(), _ => String::new() };
        match descendants(target, &kids).into_iter().find(|k| names.get(*k).is_ok_and(|n| n.as_str() == bone)) {
            Some(b) => {
                commands.entity(e).insert((ChildOf(b), Attached));
            }
            None => err.0 = Some(format!("model {}: {} has no node named {bone:?}", m.src, tm.src)),
        }
    }
}

/// A skinned model's joints, found once it's loaded: each joint's entity and the row whose placement it takes.
#[derive(Component)]
pub(super) struct Bones(Vec<(Entity, Entity)>);

/// A skinned model's joints placed where other rows are (bone_joints, bone_rows: <Model bones={{ joint: name }}>): each
/// frame each joint takes its row's placement (translation and rotation; the row can be hidden), so a ragdoll's bodies
/// move one skin that bends at the joints. Its meshes aren't culled by their bind pose's bounds, which the skin leaves
/// behind as the joints travel.
pub(super) fn sync_bones(
    mut commands: Commands,
    frame: Res<FrameRows>,
    models: Query<(Entity, &Model3d, Option<&Bones>)>,
    names: Query<&Name>,
    kids: Query<&Children>,
    meshes: Query<(), With<Mesh3d>>,
    mut tf: Query<&mut Transform>,
    mut err: ResMut<Error>,
) {
    for (e, m, bones) in &models {
        if !matches!(m.state, ModelState::Ready { .. }) {
            continue;
        }
        let pairs = match bones {
            Some(b) => b.0.clone(),
            None => {
                let Some(v) = frame.vals(e) else { continue };
                let (Some(V::S(joints)), Some(V::S(rows))) = (v.get("bone_joints"), v.get("bone_rows")) else { continue };
                let tree = descendants(e, &kids);
                let mut pairs = Vec::new();
                for (joint, row) in joints.split(',').zip(rows.split(',')).filter(|(j, _)| !j.is_empty()) {
                    let Some(je) = tree.iter().copied().find(|k| names.get(*k).is_ok_and(|n| n.as_str() == joint)) else {
                        err.0 = Some(format!("model {}: has no joint named {joint:?}", m.src));
                        continue;
                    };
                    let Some((re, _, _)) = models.iter().find(|(_, o, _)| o.id == row) else {
                        err.0 = Some(format!("model {}: joint {joint} follows {row}, which isn't a model", m.src));
                        continue;
                    };
                    pairs.push((je, re));
                }
                for k in tree.into_iter().filter(|k| meshes.contains(*k)) {
                    commands.entity(k).insert(bevy::camera::visibility::NoFrustumCulling);
                }
                commands.entity(e).insert(Bones(pairs.clone()));
                pairs
            }
        };
        for (joint, row) in pairs {
            let Ok(at) = tf.get(row).copied() else { continue };
            if let Ok(mut t) = tf.get_mut(joint) {
                if t.translation != at.translation || t.rotation != at.rotation {
                    t.translation = at.translation;
                    t.rotation = at.rotation;
                    t.scale = Vec3::ONE;
                }
            }
        }
    }
}

/// Each model's clip at the frame's time (anim_t seconds): seeked, never advanced by the clock, so the pose is a
/// function of the frame.
pub(super) fn sync_models(frame: Res<FrameRows>, clips: Res<Assets<bevy::animation::AnimationClip>>, models: Query<(Entity, &Model3d)>, mut players: Query<&mut AnimationPlayer>) {
    for (e, m) in &models {
        let ModelState::Ready { player: Some((p, node, ref clip)) } = m.state else { continue };
        let Some(v) = frame.vals(e) else { continue };
        let mut at = num(&v, "anim_t", 0.0).max(0.0);
        // looped (anim_loop, the default) or held on its last pose
        let len = clips.get(clip).map(|c| c.duration()).unwrap_or(0.0);
        if len > 0.0 {
            at = if flag(&v, "anim_loop", true) { at % len } else { at.min(len) };
        }
        if let Ok(mut pl) = players.get_mut(p) {
            if let Some(a) = pl.animation_mut(node) {
                if a.seek_time() != at || !a.is_paused() {
                    a.seek_to(at).pause();
                }
            }
        }
    }
}

#[cfg(all(test, feature = "native"))]
mod data_url_tests {
    /// data: URL pictures decode in memory: an SVG (utf8, percent-escaped) drawn at least 512 px wide with its fill, and
    /// a base64 PNG to its own pixels
    #[test]
    fn data_pictures_decode() {
        let svg = super::data_picture("data:image/svg+xml;utf8,<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"64\" height=\"32\"><rect width=\"64\" height=\"32\" fill=\"%23ff0000\"/></svg>").unwrap();
        assert_eq!(svg.dimensions(), (512, 256));
        assert_eq!(svg.get_pixel(100, 100).0, [255, 0, 0, 255]);
        let mut png = Vec::new();
        image::RgbaImage::from_pixel(3, 2, image::Rgba([0, 128, 255, 255])).write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png).unwrap();
        use base64::Engine as _;
        let url = format!("data:image/png;base64,{}", base64::engine::general_purpose::STANDARD.encode(&png));
        let pic = super::data_picture(&url).unwrap();
        assert_eq!((pic.dimensions(), pic.get_pixel(2, 1).0), ((3, 2), [0, 128, 255, 255]));
        assert!(super::data_picture("data:image/png;base64,!!!").is_err());
    }
}
