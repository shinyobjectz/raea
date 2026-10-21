//! A model's look: the model drawn at one texel a cell and laid back over the frame cell by cell (look.wgsl), so a
//! character can be pixel art or raea.io's dithered grain while everything else in the scene draws as it is.
//!
//! Columns on a model row (all optional; without `look` the model draws as before):
//!   look (0 solid, 1 pixel, 2 site; keyed, so a model changes look mid-piece), look_px (a cell's size in the frame's
//!   pixels, 6 by default: the site's), look_edge (the pixel look's outline colour, '' for none), look_theme ('dark',
//!   the default, or 'light': the site look's greys), look_lift look_gain (the site look: ink = lift + light * gain, on light greys lift + (1 - light) * gain),
//!   flat (1: the model unlit, its shape and colours alone, in every look but site)
//!
//! Each looked model gets its own render layer, which its meshes move onto while it isn't solid, and a camera on that
//! layer that follows the scene's camera into a target a cell per texel, cleared see-through. One composite camera
//! after the scene's draws every looked model's cells over the frame (a full-frame quad each, nearest the camera
//! last), before the page goes on top. The lights shine on every layer, so a looked model is lit as it was and still
//! casts its shadow on the scene. Parts of the scene in front of a looked model don't hide it.

use super::*;
use bevy::camera::visibility::RenderLayers;
use bevy::core_pipeline::tonemapping::{DebandDither, Tonemapping};
use bevy::pbr::{Material, MaterialPlugin, MeshMaterial3d};
use bevy::render::render_resource::{AsBindGroup, ShaderType};
use bevy::shader::ShaderRef;

/// The composite camera's layer, past any look's.
const COMPOSITE: usize = 31;

pub(super) fn install(app: &mut App) {
    bevy::asset::embedded_asset!(app, "look.wgsl");
    app.add_plugins(MaterialPlugin::<LookMaterial>::default());
}

#[derive(Asset, TypePath, AsBindGroup, Clone)]
pub(super) struct LookMaterial {
    #[uniform(0)]
    p: LookParams,
    #[texture(1)]
    #[sampler(2)]
    cells: Handle<Image>,
}

#[derive(ShaderType, Clone, Copy, Default, PartialEq)]
struct LookParams {
    mode: u32,
    dark: u32,
    size: Vec2,
    edge: Vec4,
    lift: f32,
    gain: f32,
    /// the frame look: how much of a cell's hue its grey keeps
    color: f32,
    /// the ink look's paper (its ink is `edge`) and how big a step in light draws a line (0: none)
    paper: Vec4,
    line: f32,
    /// the ink look's split between ink and paper, and how wide a band of light either side of it is dithered
    threshold: f32,
    band: f32,
    /// the paint look (mode 6): paper grain (0 none), how far edges wander in pixels and the size of their wander,
    /// the wander's seed (a boiling line's changes on its beat), the sky where nothing's drawn (top and bottom; alpha
    /// 0, none: the scene's own background), and a palette every pixel snaps to (`npal` colours, 0 for none)
    grain: f32,
    rag: f32,
    rag_scale: f32,
    seed: f32,
    sky_top: Vec4,
    sky_bottom: Vec4,
    npal: u32,
    pal: [Vec4; 16],
}

impl Material for LookMaterial {
    fn vertex_shader() -> ShaderRef {
        "embedded://rdb/engine/look.wgsl".into()
    }
    fn fragment_shader() -> ShaderRef {
        "embedded://rdb/engine/look.wgsl".into()
    }
    fn alpha_mode(&self) -> AlphaMode {
        // the frame's own cells (modes 3 and 4) cover it; a model's are laid over them
        if self.p.mode >= 3 { AlphaMode::Opaque } else { AlphaMode::Blend }
    }
    fn enable_prepass() -> bool {
        false
    }
    fn enable_shadows() -> bool {
        false
    }
}

/// A looked model's layer, its cell camera and its quad on the composite.
#[derive(Component)]
pub(super) struct Look {
    layer: usize,
    cam: Entity,
    quad: Entity,
    mat: Handle<LookMaterial>,
}

/// A camera this file made (a model's cells, or the composite): not the scene's.
#[derive(Component)]
pub(super) struct LookCam;

fn mode_of(v: &BTreeMap<String, V>) -> u32 {
    match v.get("look") {
        Some(V::S(s)) => match s.as_str() {
            "pixel" => 1,
            "site" | "dither" => 2,
            _ => 0,
        },
        Some(x) => x.num().clamp(0.0, 2.0) as u32,
        None => 0,
    }
}

/// The whole frame's look (a <Look> in the scene: frame_look on the camera row): the scene's camera draws into a target
/// a cell per texel instead of the frame, its bloom, fog and depth of field with it, and the composite lays those cells
/// over the frame, opaque, under any model's own look. Set up once, from the first frame's row.
#[allow(clippy::type_complexity)]
pub(super) fn sync_frame_look(
    mut commands: Commands,
    frame: Res<FrameRows>,
    edit: Res<Edit>,
    mut images: ResMut<Assets<Image>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut mats: ResMut<Assets<LookMaterial>>,
    mut main: Query<(Entity, &mut RenderTarget, &mut Camera, &Msaa, Has<bevy::camera::Hdr>), (With<Camera3d>, Without<LookCam>)>,
    composite: Query<Entity, (With<LookCam>, With<Camera3d>, Without<Mesh3d>)>,
    mut ui: Query<&mut bevy::ui::UiTargetCamera>,
    mut done: Local<Option<Handle<LookMaterial>>>,
) {
    if let Some(h) = done.as_ref() {
        // the paint look's keyed values (grain, the edges' wander and its seed), each frame they change
        let Ok((cam, ..)) = main.single() else { return };
        let Some(v) = frame.vals(cam) else { return };
        let Some(m) = mats.get(h) else { return };
        if m.p.mode != 6 {
            return;
        }
        let (grain, rag, seed) = (num(&v, "paint_grain", m.p.grain), num(&v, "paint_rag", m.p.rag), num(&v, "paint_seed", m.p.seed));
        if (m.p.grain, m.p.rag, m.p.seed) != (grain, rag, seed) {
            if let Some(mut m) = mats.get_mut(h) {
                (m.p.grain, m.p.rag, m.p.seed) = (grain, rag, seed);
            }
        }
        return;
    }
    let Ok((cam, mut target, mut camera, msaa, hdr)) = main.single_mut() else { return };
    let Some(v) = frame.vals(cam) else { return };
    let v = &v;
    let Some(V::S(mode)) = v.get("frame_look") else { return };
    let paint = mode == "paint";
    let sky = |k: &str| match v.get(k) {
        Some(V::S(s)) if !s.is_empty() => Vec4::from_array(colour(v.get(k), [0.0; 4]).to_linear().to_f32_array()),
        _ => Vec4::ZERO,
    };
    let (sky_top, sky_bottom) = (sky("paint_sky_top"), sky("paint_sky_bottom"));
    if paint {
        // the paint's colours are chosen, not exposed: no tone curve between them and the frame; and with a sky, the
        // scene clears see-through so the composite knows where nothing's drawn
        commands.entity(cam).insert(Tonemapping::None);
        if sky_top.w > 0.0 {
            camera.clear_color = ClearColorConfig::Custom(Color::NONE);
        }
    }
    let mut pal = [Vec4::ZERO; 16];
    let mut npal = 0;
    if let Some(V::S(list)) = v.get("paint_palette") {
        for h in list.split(',').filter(|h| !h.is_empty()).take(16) {
            pal[npal] = Vec4::from_array(colour(Some(&V::S(h.to_string())), [0.0; 4]).to_linear().to_f32_array());
            npal += 1;
        }
    }
    let px = num(v, "frame_look_px", 6.0).max(1.0);
    let (w, h) = (edit.prog.width, edit.prog.height);
    let (cw, ch) = (((w as f32) / px).ceil().max(1.0) as u32, ((h as f32) / px).ceil().max(1.0) as u32);
    let mut img = Image::new_target_texture(cw, ch, TextureFormat::Rgba8UnormSrgb, None);
    img.sampler = bevy::image::ImageSampler::nearest();
    let cells = images.add(img);
    let frame_target = std::mem::replace(&mut *target, RenderTarget::Image(cells.clone().into()));
    let mat = mats.add(LookMaterial {
        p: LookParams {
            mode: match mode.as_str() {
                "pixel" => 4,
                "ink" => 5,
                "paint" => 6,
                _ => 3,
            },
            dark: !matches!(v.get("frame_look_theme"), Some(V::S(s)) if s == "light") as u32,
            size: Vec2::new(cw as f32, ch as f32),
            edge: Vec4::from_array(colour(v.get("frame_look_ink"), [0.13, 0.12, 0.10, 1.0]).to_linear().to_f32_array()),
            paper: Vec4::from_array(colour(v.get("frame_look_paper"), [0.90, 0.88, 0.80, 1.0]).to_linear().to_f32_array()),
            line: num(v, "frame_look_line", 0.0),
            threshold: num(v, "frame_look_threshold", 0.5),
            band: num(v, "frame_look_band", 1.0),
            lift: num(v, "frame_look_lift", 0.0),
            gain: num(v, "frame_look_gain", 1.0),
            color: num(v, "frame_look_color", 1.0),
            grain: num(v, "paint_grain", 0.3),
            rag: num(v, "paint_rag", 2.0),
            rag_scale: num(v, "paint_rag_scale", 24.0).max(1.0),
            seed: num(v, "paint_seed", 0.0),
            sky_top,
            sky_bottom: if sky_bottom.w > 0.0 { sky_bottom } else { sky_top },
            npal: npal as u32,
            pal,
        },
        cells,
    });
    *done = Some(mat.clone());
    commands.spawn((Mesh3d(meshes.add(Rectangle::new(1.0, 1.0))), MeshMaterial3d(mat), RenderLayers::layer(COMPOSITE), Transform::default(), bevy::camera::visibility::NoFrustumCulling, LookCam));
    if composite.is_empty() {
        let mut c = commands.spawn((
            Camera3d::default(),
            Camera { order: 1, clear_color: ClearColorConfig::None, ..default() },
            frame_target,
            *msaa,
            RenderLayers::layer(COMPOSITE),
            Tonemapping::None,
            DebandDither::Disabled,
            Transform::default(),
            LookCam,
        ));
        if hdr {
            c.insert(bevy::camera::Hdr);
        }
        // the page goes over the frame, not into its cells
        c.insert(bevy::ui::IsDefaultUiCamera);
        // and so does every page part made for the scene's camera (a render's clips and pages, drawn as UI on it):
        // left on a camera that now draws into the cells, they'd be laid out at the cells' size and drawn into them
        let c = c.id();
        for mut t in &mut ui {
            if t.0 == cam {
                t.0 = c;
            }
        }
    }
}

/// Looks set up for models that ask for one (once each, when the model is ready), then each frame: the model's
/// meshes on its layer or the scene's, its camera where the scene's is, its composite's mode.
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
pub(super) fn sync_looks(
    mut commands: Commands,
    frame: Res<FrameRows>,
    edit: Res<Edit>,
    mut images: ResMut<Assets<Image>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut mats: ResMut<Assets<LookMaterial>>,
    models: Query<(Entity, &scene3d::Model3d, Option<&Look>)>,
    kids: Query<&Children>,
    layered: Query<(Entity, Option<&RenderLayers>), With<Mesh3d>>,
    lights: Query<Entity, Or<(With<DirectionalLight>, With<PointLight>, With<SpotLight>)>>,
    main: Query<(Entity, &Transform, Option<&Projection>, Option<&AmbientLight>, Option<&Tonemapping>, Option<&bevy::camera::Exposure>, &RenderTarget, &Msaa, Has<bevy::camera::Hdr>), (With<Camera3d>, Without<LookCam>)>,
    mut cams: Query<(&mut Camera, &mut Transform, Option<&mut Projection>), (With<LookCam>, Without<Look>)>,
    mut quads: Query<&mut Visibility, (With<Mesh3d>, With<LookCam>)>,
    composite: Query<(Entity, Has<bevy::camera::Hdr>), (With<LookCam>, With<Camera3d>, Without<Mesh3d>)>,
    mut next: Local<usize>,
) {
    let Ok((scene_cam, ct, proj, amb, tm, ev, target, msaa, hdr)) = main.single() else { return };
    let (w, h) = (edit.prog.width, edit.prog.height);
    let mut made = false;
    for (e, m, look) in &models {
        if !m.ready() {
            continue;
        }
        let Some(v) = frame.vals(e) else { continue };
        let v = &v;
        if look.is_none() {
            if !v.contains_key("look") {
                continue;
            }
            // a cell's size in the frame's pixels; the cells' target is the frame's size over it, rounded up
            let px = num(v, "look_px", 6.0).max(1.0);
            let (cw, ch) = (((w as f32) / px).ceil().max(1.0) as u32, ((h as f32) / px).ceil().max(1.0) as u32);
            let mut img = Image::new_target_texture(cw, ch, TextureFormat::Rgba8UnormSrgb, None);
            img.sampler = bevy::image::ImageSampler::nearest();
            let cells = images.add(img);
            *next += 1;
            let layer = *next;
            if layer >= COMPOSITE {
                continue; // thirty looks at most
            }
            let mut cam = commands.spawn((
                Camera3d::default(),
                Camera { order: -1 - layer as isize, clear_color: ClearColorConfig::Custom(Color::NONE), ..default() },
                RenderTarget::Image(cells.clone().into()),
                Msaa::Off,
                RenderLayers::layer(layer),
                *ct,
                tm.copied().unwrap_or_default(),
                LookCam,
            ));
            if let Some(p) = proj {
                cam.insert(p.clone());
            }
            if let Some(a) = amb {
                cam.insert(a.clone());
            }
            if let Some(x) = ev {
                cam.insert(*x);
            }
            let cam = cam.id();
            let edge = match v.get("look_edge") {
                Some(V::S(s)) if s.is_empty() || s == "none" => Vec4::ZERO,
                c => Vec4::from_array(colour(c, [0.0, 0.0, 0.0, 1.0]).to_linear().to_f32_array()),
            };
            let theme_dark = !matches!(v.get("look_theme"), Some(V::S(s)) if s == "light");
            let mat = mats.add(LookMaterial {
                p: LookParams {
                    mode: 0,
                    dark: theme_dark as u32,
                    size: Vec2::new(cw as f32, ch as f32),
                    edge,
                    // on the light greys ink is darkness, and a dark model needs more of the range to show its light
                    lift: num(v, "look_lift", if theme_dark { 0.28 } else { 0.35 }),
                    gain: num(v, "look_gain", if theme_dark { 0.9 } else { 0.75 }),
                    color: 0.0,
                    paper: Vec4::ZERO,
                    line: 0.0,
                    threshold: 0.5,
                    band: 1.0,
                    ..default()
                },
                cells,
            });
            let quad = commands
                .spawn((Mesh3d(meshes.add(Rectangle::new(1.0, 1.0))), MeshMaterial3d(mat.clone()), RenderLayers::layer(COMPOSITE), Transform::default(), Visibility::Hidden, bevy::camera::visibility::NoFrustumCulling, LookCam))
                .id();
            commands.entity(e).insert(Look { layer, cam, quad, mat });
            made = true;
        }
    }
    if made {
        // every light on every look's layer as well as the scene's
        let mut all: Vec<usize> = vec![0];
        all.extend(1..=*next);
        for l in &lights {
            commands.entity(l).insert(RenderLayers::from_layers(&all));
        }
        if composite.is_empty() {
            let mut c = commands.spawn((
                Camera3d::default(),
                Camera { order: 1, clear_color: ClearColorConfig::None, ..default() },
                target.clone(),
                *msaa,
                RenderLayers::layer(COMPOSITE),
                Tonemapping::None,
                DebandDither::Disabled,
                Transform::default(),
                LookCam,
            ));
            if hdr {
                c.insert(bevy::camera::Hdr);
            }
        }
        // the page's UI stays on the scene's camera (with a frame look, which draws into cells, on the composite)
        if !matches!(frame.vals(scene_cam).as_ref().and_then(|v| v.get("frame_look")), Some(V::S(_))) {
            commands.entity(scene_cam).insert(bevy::ui::IsDefaultUiCamera);
        }
    }
    // the composite draws over the scene's own frame only if it's as HDR as the scene's camera (Bloom makes it so,
    // perhaps a few updates in): otherwise it gets a frame of its own and covers the scene with it
    for (c, has) in &composite {
        if has != hdr {
            if hdr {
                commands.entity(c).insert(bevy::camera::Hdr);
            } else {
                commands.entity(c).remove::<bevy::camera::Hdr>();
            }
        }
    }
    for (e, _, look) in &models {
        let Some(look) = look else { continue };
        let Some(v) = frame.vals(e) else { continue };
        let mode = mode_of(&v);
        let on = mode != 0;
        // the model's meshes (a scene can spawn more of them a few updates after it's ready) on its layer or the scene's
        let want = RenderLayers::layer(if on { look.layer } else { 0 });
        for d in scene3d::descendants(e, &kids) {
            if let Ok((m, have)) = layered.get(d) {
                if have != Some(&want) {
                    commands.entity(m).insert(want.clone());
                }
            }
        }
        if let Ok((mut c, mut t, p)) = cams.get_mut(look.cam) {
            if c.is_active != on {
                c.is_active = on;
            }
            if *t != *ct {
                *t = *ct;
            }
            if let (Some(mut p), Some(sp)) = (p, proj) {
                if let (Projection::Perspective(a), Projection::Perspective(b)) = (p.as_mut(), sp) {
                    if a.fov != b.fov {
                        a.fov = b.fov;
                    }
                }
            }
        }
        if let Ok(mut vis) = quads.get_mut(look.quad) {
            let want = if on { Visibility::Inherited } else { Visibility::Hidden };
            if *vis != want {
                *vis = want;
            }
        }
        if let Some(m) = mats.get(&look.mat) {
            if m.p.mode != mode {
                if let Some(mut m) = mats.get_mut(&look.mat) {
                    m.p.mode = mode;
                }
            }
        }
    }
}

/// A flat model's surfaces (its row's `flat`): shape and colour with no light on them, each of its meshes' materials
/// copied unlit, except under the site look, whose grain is made of the light, where the lit originals come back.
#[derive(Component, Default)]
pub(super) struct Flat(Vec<(Entity, Handle<StandardMaterial>, Handle<StandardMaterial>)>);

pub(super) fn sync_flat(
    mut commands: Commands,
    frame: Res<FrameRows>,
    mut models: Query<(Entity, &scene3d::Model3d, Option<&mut Flat>)>,
    kids: Query<&Children>,
    mut surfaces: Query<&mut MeshMaterial3d<StandardMaterial>>,
    // a see-through mesh drawn with its froxel fog copy keeps its standard material here
    mut fogged: Query<&mut super::froxel_glass::Fogged>,
    mut std: ResMut<Assets<StandardMaterial>>,
) {
    for (e, m, flat) in &mut models {
        if !m.ready() {
            continue;
        }
        let Some(v) = frame.vals(e) else { continue };
        if !v.get("flat").is_some_and(V::truthy) {
            continue;
        }
        let Some(mut flat) = flat else {
            commands.entity(e).insert(Flat::default());
            continue;
        };
        // the copies made once per mesh (the model's own, not the file's, which other copies of it share); a scene
        // can spawn more meshes a few updates after it's ready
        for d in scene3d::descendants(e, &kids) {
            if flat.0.iter().any(|(x, ..)| *x == d) {
                continue;
            }
            let Some(lit) = surfaces.get(d).map(|s| s.0.clone()).ok().or_else(|| fogged.get(d).ok().map(|f| f.std.clone())) else { continue };
            let Some(mut copy) = std.get(&lit).cloned() else { continue };
            copy.unlit = true;
            flat.0.push((d, lit, std.add(copy)));
        }
        let unlit = mode_of(&v) != 2;
        for (d, lit, un) in &flat.0 {
            let want = if unlit { un } else { lit };
            if let Ok(mut s) = surfaces.get_mut(*d) {
                if s.0 != *want {
                    s.0 = want.clone();
                }
            } else if let Ok(mut f) = fogged.get_mut(*d) {
                if f.std != *want {
                    f.std = want.clone();
                }
            }
        }
    }
}
