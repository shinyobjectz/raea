//! Matchmove's engine side: footage behind the 3D (`plate` rows) and a camera that sees a window of a larger view (the
//! camera row's `view_*` columns), so 3D solved into a clip's world (raea-motion's <Matchmove>) stands in it.
//!
//!   plate: src (a video), t (the source time each frame, seconds), k1 (the lens's radial distortion, SIMPLE_RADIAL's),
//!          visible. A grid on the camera's far side, as big as its whole view, unlit, its texture the clip's frame at t,
//!          its texture coordinates bent by k1, so the footage's straight lines are straight where the 3D's are.
//!   camera up_x up_y up_z: which way is up for the camera (its roll), where it looks as scene3d::sync_camera puts it.
//!   camera view_w view_h view_x view_y: the frame is the window at (view_x, view_y), the frame's size, of a view
//!          view_w x view_h frame pixels whose vertical field is `fov` (an off-axis projection, `OffAxis`). A 9:16
//!          crop of 16:9 footage keeps the lens's centre where the footage had it, so the 3D stays locked to it.
//!
//! The plate is drawn as the scene's own picture, so it goes through tone mapping like everything else: <Matchmove>
//! puts `<ToneMapping mode="none">` in its scene, which leaves an unlit texture's colours as they are.

use super::*;
use bevy::camera::{CameraProjection, SubCameraView};
use bevy::math::Vec3A;

/// A perspective that draws the window (offset, size) of a larger view `full` (frame pixels), whose vertical field of
/// view is `persp.fov`. The frustum (culling, shadow cascades) is the window's own, not the whole view's.
#[derive(Debug, Clone)]
pub(super) struct OffAxis {
    pub persp: PerspectiveProjection,
    pub full: Vec2,
    pub offset: Vec2,
    pub size: Vec2,
}

impl OffAxis {
    /// The window's left, right, bottom and top at the near plane.
    fn bounds(&self) -> (f32, f32, f32, f32) {
        let n = self.persp.near;
        let top = n * (0.5 * self.persp.fov).tan();
        let right = top * self.full.x / self.full.y;
        // y grows downward in pixels, upward in the view
        let oy = self.full.y - (self.offset.y + self.size.y);
        (
            -right + 2.0 * right * self.offset.x / self.full.x,
            -right + 2.0 * right * (self.offset.x + self.size.x) / self.full.x,
            -top + 2.0 * top * oy / self.full.y,
            -top + 2.0 * top * (oy + self.size.y) / self.full.y,
        )
    }
}

impl CameraProjection for OffAxis {
    fn get_clip_from_view(&self) -> Mat4 {
        let (l, r, b, t) = self.bounds();
        let n = self.persp.near;
        // Bevy's infinite reverse-z, off axis (PerspectiveProjection::get_clip_from_view_for_sub's matrix)
        Mat4::from_cols(
            Vec4::new(2.0 * n / (r - l), 0.0, 0.0, 0.0),
            Vec4::new(0.0, 2.0 * n / (t - b), 0.0, 0.0),
            Vec4::new((r + l) / (r - l), (t + b) / (t - b), 0.0, -1.0),
            Vec4::new(0.0, 0.0, n, 0.0),
        )
    }

    fn get_clip_from_view_for_sub(&self, sub: &SubCameraView) -> Mat4 {
        self.persp.get_clip_from_view_for_sub(sub)
    }

    /// The window's size is the row's (the frame's); a look's cells target, smaller, draws the same window.
    fn update(&mut self, width: f32, height: f32) {
        self.persp.update(width, height);
    }

    fn far(&self) -> f32 {
        self.persp.far
    }

    fn get_frustum_corners(&self, z_near: f32, z_far: f32) -> [Vec3A; 8] {
        let (l, r, b, t) = self.bounds();
        let n = self.persp.near;
        let at = |z: f32| {
            let s = z.abs() / n;
            [Vec3A::new(r * s, b * s, z), Vec3A::new(r * s, t * s, z), Vec3A::new(l * s, t * s, z), Vec3A::new(l * s, b * s, z)]
        };
        let (a, f) = (at(z_near), at(z_far));
        [a[0], a[1], a[2], a[3], f[0], f[1], f[2], f[3]]
    }
}

/// The camera's projection for a frame's row: an OffAxis window when it has view_* columns.
fn off_axis(v: &BTreeMap<String, V>, frame: Vec2, near: f32, far: f32) -> Option<OffAxis> {
    let (w, h) = (num(v, "view_w", 0.0), num(v, "view_h", 0.0));
    if !(w > 0.0 && h > 0.0) {
        return None;
    }
    let persp = PerspectiveProjection { fov: num(v, "fov", 45.0).to_radians(), near, far, aspect_ratio: frame.x / frame.y, ..default() };
    Some(OffAxis { persp, full: Vec2::new(w, h), offset: Vec2::new(num(v, "view_x", 0.0), num(v, "view_y", 0.0)), size: frame })
}

/// The camera's place for a row with an `up`: where it is, looking where it says, rolled to that up.
fn rolled(v: &BTreeMap<String, V>) -> Option<Transform> {
    if !v.contains_key("up_x") {
        return None;
    }
    let pos = Vec3::new(num(v, "x", 0.0), num(v, "y", 2.0), num(v, "z", 9.0));
    let look = Vec3::new(num(v, "look_x", 0.0), num(v, "look_y", 1.0), num(v, "look_z", 0.0));
    let up = Vec3::new(num(v, "up_x", 0.0), num(v, "up_y", 1.0), num(v, "up_z", 0.0));
    ((look - pos).length_squared() > 1e-12 && up.cross(look - pos).length_squared() > 1e-12).then(|| Transform::from_translation(pos).looking_at(look, up))
}

/// At load, after the camera's lens (scene3d::camera): its window and roll, if the row has them, so a look's camera
/// (look.rs), which copies the projection when it's made, sees the same window.
pub(super) fn camera_view(world: &mut World, e: Entity, vals: &BTreeMap<String, V>, frame: Vec2) {
    let (near, far) = (num(vals, "near", 0.1), num(vals, "far", 1000.0));
    if let Some(p) = off_axis(vals, frame, near, far) {
        world.entity_mut(e).insert(Projection::custom(p));
    }
    if let Some(t) = rolled(vals) {
        world.entity_mut(e).insert(t);
    }
}

/// The camera's roll, window and lens each frame: after scene3d::sync_camera (which places it with +Y up and leaves a
/// custom projection alone) and before the looks copy its place (look::sync_looks). A look's camera (no row of its
/// own) copied the window when it was made; it gets each new one here, so a model drawn in a look stays locked as a
/// crop moves.
pub(super) fn sync_view(frame: Res<FrameRows>, edit: Res<Edit>, mut q: Query<(Entity, &mut Projection, &mut Transform), With<Camera3d>>) {
    let size = Vec2::new(edit.prog.width as f32, edit.prog.height as f32);
    let mut window: Option<OffAxis> = None;
    for (e, mut p, mut t) in &mut q {
        let Some(v) = frame.vals(e) else { continue };
        if let Some(nt) = rolled(&v) {
            if *t != nt {
                *t = nt;
            }
        }
        let Projection::Custom(c) = &*p else { continue };
        let Some(old) = c.get::<OffAxis>() else { continue };
        let Some(new) = off_axis(&v, size, old.persp.near, old.persp.far) else { continue };
        if new.persp.fov != old.persp.fov || new.full != old.full || new.offset != old.offset {
            *p = Projection::custom(new.clone());
        }
        window = Some(new);
    }
    let Some(w) = window else { return };
    for (e, mut p, _) in &mut q {
        if frame.vals(e).is_some() {
            continue;
        }
        let Projection::Custom(c) = &*p else { continue };
        let Some(old) = c.get::<OffAxis>() else { continue };
        if w.persp.fov != old.persp.fov || w.full != old.full || w.offset != old.offset {
            *p = Projection::custom(w.clone());
        }
    }
}

#[cfg(feature = "native")]
/// A plate's state: the frame's texture, and the grid's tan(fov/2) and k1 it was built for.
#[derive(Component)]
pub(super) struct Plate {
    src: String,
    slot: usize,
    image: Handle<Image>,
    aspect: f32,
    k1: f32,
    built: f32,
    last: Option<usize>,
}

#[cfg(feature = "native")]
/// Footage behind the 3D: its decoder opened (to know its size), an empty texture and unlit material; the grid is made
/// by sync_plates once the camera's lens is known.
pub(super) fn spawn_plate(world: &mut World, e: Entity, t: &Tup, inserted: &[Tup], dir: &Path) -> Result<(), String> {
    let vals = &t.vals;
    let src = match vals.get("src") {
        Some(V::S(s)) if !s.is_empty() => s.clone(),
        _ => return Err(format!("plate {}: needs src (a video)", t.id)),
    };
    let path = resolve_src(inserted, &src, dir)?;
    let slot = 1000 + world.query::<&Plate>().iter(world).count();
    let (w, h) = {
        let mut m = world.non_send_mut::<Media>();
        let dec = m.backend.open_video(&path).map_err(|x| format!("plate {}: {}: {x}", t.id, path.display()))?;
        let (w, h) = (dec.info().width, dec.info().height);
        m.decoders.insert((src.clone(), slot), dec);
        (w, h)
    };
    let image = world.resource_mut::<Assets<Image>>().add(Image::new_fill(
        bevy::render::render_resource::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
        bevy::render::render_resource::TextureDimension::D2,
        &[0, 0, 0, 255],
        TextureFormat::Rgba8UnormSrgb,
        bevy::asset::RenderAssetUsages::default(),
    ));
    let mat = world.resource_mut::<Assets<StandardMaterial>>().add(StandardMaterial {
        base_color_texture: Some(image.clone()),
        unlit: true,
        fog_enabled: false,
        cull_mode: None,
        ..default()
    });
    let mesh = world.resource_mut::<Assets<Mesh>>().add(Mesh::from(Rectangle::new(1.0, 1.0)));
    world.entity_mut(e).insert((
        Mesh3d(mesh),
        MeshMaterial3d(mat),
        Transform::default(),
        Visibility::Hidden,
        bevy::light::NotShadowCaster,
        bevy::light::NotShadowReceiver,
        bevy::camera::visibility::NoFrustumCulling,
        Plate { src, slot, image, aspect: w as f32 / h.max(1) as f32, k1: num(vals, "k1", 0.0), built: 0.0, last: None },
    ));
    Ok(())
}

#[cfg(feature = "native")]
/// The grid a plate is drawn on: the view's normalised image plane (x in [-aspect, aspect] * th, y in [-th, th], at
/// z = -1), each vertex's texture coordinate where SIMPLE_RADIAL's k1 puts that undistorted ray in the footage.
pub(super) fn plate_grid(aspect: f32, th: f32, k1: f32) -> Mesh {
    let (nx, ny) = (64u32, 36u32);
    let mut pos = Vec::new();
    let mut uv = Vec::new();
    for j in 0..=ny {
        for i in 0..=nx {
            let (u, v) = (i as f32 / nx as f32, j as f32 / ny as f32);
            let (x, y) = ((2.0 * u - 1.0) * aspect * th, (1.0 - 2.0 * v) * th);
            let d = 1.0 + k1 * (x * x + y * y);
            pos.push([x, y, -1.0]);
            uv.push([(x * d / (aspect * th) + 1.0) / 2.0, (1.0 - y * d / th) / 2.0]);
        }
    }
    let mut idx = Vec::new();
    for j in 0..ny {
        for i in 0..nx {
            let a = j * (nx + 1) + i;
            idx.extend([a, a + nx + 1, a + 1, a + 1, a + nx + 1, a + nx + 2]);
        }
    }
    let mut m = Mesh::new(bevy::render::render_resource::PrimitiveTopology::TriangleList, bevy::asset::RenderAssetUsages::default());
    m.insert_attribute(Mesh::ATTRIBUTE_POSITION, pos.clone());
    m.insert_attribute(Mesh::ATTRIBUTE_NORMAL, vec![[0.0f32, 0.0, 1.0]; pos.len()]);
    m.insert_attribute(Mesh::ATTRIBUTE_UV_0, uv);
    m.insert_indices(bevy::mesh::Indices::U32(idx));
    m
}

#[cfg(feature = "native")]
/// Each plate's frame at its row's time, on the camera's far side, as big as its whole view.
#[allow(clippy::too_many_arguments)]
pub(super) fn sync_plates(
    frame: Res<FrameRows>,
    mut media: NonSendMut<Media>,
    mut err: ResMut<Error>,
    mut images: ResMut<Assets<Image>>,
    mut meshes: ResMut<Assets<Mesh>>,
    cams: Query<(Entity, &Transform, &Projection), (With<Camera3d>, Without<Plate>)>,
    mut q: Query<(Entity, &mut Plate, &mut Transform, &mut Visibility, &mut Mesh3d)>,
) {
    if q.is_empty() {
        return;
    }
    // the scene's camera: the one with a row (a look's cameras have none)
    let Some((_, ct, proj)) = cams.iter().find(|(e, _, _)| frame.vals(*e).is_some()) else { return };
    let (fov, far) = match proj {
        Projection::Perspective(p) => (p.fov, p.far),
        Projection::Custom(c) => match c.get::<OffAxis>() {
            Some(o) => (o.persp.fov, o.persp.far),
            None => return,
        },
        _ => return,
    };
    let th = (0.5 * fov).tan();
    for (e, mut plate, mut t, mut vis, mut mesh) in &mut q {
        let Some(v) = frame.vals(e) else { continue };
        let on = num(&v, "visible", 1.0) > 0.5;
        let want = if on { Visibility::Inherited } else { Visibility::Hidden };
        if *vis != want {
            *vis = want;
        }
        if !on {
            continue;
        }
        if plate.built != th {
            mesh.0 = meshes.add(plate_grid(plate.aspect, th, plate.k1));
            plate.built = th;
        }
        // the grid is the image plane at z = -1 in the camera's space: scaled to half the far plane away (inside the
        // frustum whatever the depth range), it is the same picture, behind everything nearer
        let nt = Transform { translation: ct.translation, rotation: ct.rotation, scale: Vec3::splat(0.5 * far) };
        if *t != nt {
            *t = nt;
        }
        let at = num(&v, "t", 0.0) as f64;
        let key = (plate.src.clone(), plate.slot);
        let Some(dec) = media.decoders.get_mut(&key) else { continue };
        let idx = dec.index_at(at);
        if idx.is_some() && idx == plate.last {
            continue;
        }
        match dec.frame_at(at) {
            Ok(f) => {
                if let Some(mut img) = images.get_mut(&plate.image) {
                    if img.width() == f.width && img.height() == f.height {
                        img.data = Some(f.rgba);
                    }
                }
                plate.last = idx;
            }
            Err(x) => {
                err.0 = Some(format!("plate {} at {at:.3}s: {x}", plate.src));
                return;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_centred_window_is_the_plain_perspective() {
        let persp = PerspectiveProjection { fov: 0.6, near: 0.1, far: 100.0, aspect_ratio: 16.0 / 9.0, ..default() };
        let o = OffAxis { persp: persp.clone(), full: Vec2::new(1920.0, 1080.0), offset: Vec2::ZERO, size: Vec2::new(1920.0, 1080.0) };
        assert!(o.get_clip_from_view().abs_diff_eq(persp.get_clip_from_view(), 1e-5));
    }

    #[test]
    fn a_window_sees_its_part_of_the_view() {
        // a 9:16 window at x 1000 of a 3413 x 1920 view: a point that lands at pixel (1500, 600) of the whole view
        // lands at (500, 600) of the window
        let fov = 0.6f32;
        let persp = PerspectiveProjection { fov, near: 0.1, far: 100.0, ..default() };
        let o = OffAxis { persp, full: Vec2::new(3413.0, 1920.0), offset: Vec2::new(1000.0, 0.0), size: Vec2::new(1080.0, 1920.0) };
        let f = 960.0 / (fov / 2.0).tan();
        let p = Vec3::new((1500.0 - 3413.0 / 2.0) / f, -(600.0 - 960.0) / f, -1.0) * 7.0;
        let c = o.get_clip_from_view() * p.extend(1.0);
        let (x, y) = ((c.x / c.w + 1.0) / 2.0 * 1080.0, (1.0 - c.y / c.w) / 2.0 * 1920.0);
        assert!((x - 500.0).abs() < 0.01 && (y - 600.0).abs() < 0.01, "{x} {y}");
        // the frustum's corners are the window's
        let k = o.get_frustum_corners(-1.0, -10.0);
        let l = (1000.0 - 3413.0 / 2.0) / f;
        assert!((k[2].x - l).abs() < 1e-4 && (k[6].x - 10.0 * l).abs() < 1e-3, "{k:?}");
    }

    #[test]
    fn the_grid_bends_by_k1_and_not_without_it() {
        let m = plate_grid(16.0 / 9.0, 0.3, 0.0);
        let Some(bevy::mesh::VertexAttributeValues::Float32x2(uv)) = m.attribute(Mesh::ATTRIBUTE_UV_0) else { panic!() };
        assert_eq!(uv[0], [0.0, 0.0]);
        let m = plate_grid(16.0 / 9.0, 0.3, -0.05);
        let Some(bevy::mesh::VertexAttributeValues::Float32x2(uv)) = m.attribute(Mesh::ATTRIBUTE_UV_0) else { panic!() };
        // barrel: the corner's ray lands inside the footage's corner; the centre doesn't move
        assert!(uv[0][0] > 0.0 && uv[0][1] > 0.0);
        let c = 18 * 65 + 32;
        assert!((uv[c][0] - 0.5).abs() < 1e-6 && (uv[c][1] - 0.5).abs() < 1e-6);
    }
}
