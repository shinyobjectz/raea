//! Rows into the World: stage timings, cameras, the 2D and 3D rows applied and removed, and a 3D row spawned by kind.

use super::*;

/// Clips and the overlay composite on the GPU (the default with NV12), or as Bevy UI nodes (RAEA_COMPOSITE=ui).
#[cfg(feature = "native")]
#[derive(Resource, Clone, Copy)]
pub(crate) struct Composite(pub(crate) bool);

#[cfg(feature = "native")]
pub(crate) fn on_gpu(c: Res<Composite>) -> bool {
    c.0
}

#[cfg(feature = "native")]
pub(crate) fn on_ui(c: Res<Composite>) -> bool {
    !c.0
}

/// The image rows' overlay (rows2d.rs, bevy_ui) above every clip.
#[cfg(feature = "native")]
pub(crate) const OVERLAY_Z: i32 = 1000;

/// The frame's state onto the entities: one relational step, pure.
pub(crate) fn relational(edit: Res<Edit>, clock: Res<Clock>, mut err: ResMut<Error>, mut st: ResMut<Stages>, mut frame: ResMut<FrameRows>) {
    // a live scene's rows are its frame's values already (Engine::set_rows)
    if frame.live.is_some() {
        return;
    }
    let t0 = bevy::platform::time::Instant::now();
    frame.set_cols(edit.timeline.cols(clock.0));
    if !frame.timeline.as_ref().is_some_and(|t| std::sync::Arc::ptr_eq(t, &edit.timeline)) {
        frame.timeline = Some(edit.timeline.clone());
    }
    frame.parse_looks();
    st.rules += t0.elapsed().as_nanos() as u64;
    let _ = &mut err;
}

/// The kinds a row spawns as a thing in the 3D scene (spawn_3d).
pub(crate) fn is_3d_spawn(kind: &str) -> bool {
    matches!(kind, "light" | "model" | "cube" | "sphere" | "plane" | "mesh" | "panel" | "decal" | "fogvolume" | "fire" | "grass" | "plate" | "splat" | "meshseq" | "entity" | "text" | "glyph" | "screenpage" | "sprite" | "shape2d")
}

/// A 3D row's entity made what it draws (a light, a model, a mesh with its surface, a panel): what a load does for each
/// one, and what a live scene does for a row first seen mid-play (Engine::set_rows).
/// A camera row's entity made the 3D camera, drawing into `target` (the edit's frame in an Engine; a plain App's own,
/// SceneTarget): its lens and post-processing, motion blur's prepasses, the environment and a window on a larger view.
#[allow(clippy::too_many_arguments)]
pub(crate) fn spawn_camera(world: &mut World, e: Entity, vals: &BTreeMap<String, V>, inserted: &[Tup], dir: &Path, size: (u32, u32), target: RenderTarget, has_3d: bool) -> Result<(), String> {
    let g = |k: &str, d: f32| num(vals, k, d);
                world.entity_mut(e).insert((
                    Camera3d::default(),
                    // the row's `bg` when it has one (a live scene's clear, a page export's), else Bevy's ClearColor resource, which
                    // <BevyResource type="bevy_camera::clear_color::ClearColor"> sets (docs/control conventions C17)
                    Camera { clear_color: vals.get("bg").map_or(ClearColorConfig::Default, |b| ClearColorConfig::Custom(colour(Some(b), [0.0, 0.0, 0.0, 1.0]))), ..default() },
                    target,
                    // ambient occlusion can't multisample, and SMAA or FXAA replace it (scene3d); composition passes read a
                    // single-sampled prepass (compose.rs)
                    if has_3d && !preview() && !vals.contains_key("ssao") && !vals.contains_key("aa") && !vals.contains_key("outline") && !vals.contains_key("frame_looks") && !vals.contains_key("passes") { Msaa::default() } else { Msaa::Off },
                    Transform::from_xyz(g("x", 0.0), g("y", 2.0), g("z", 9.0)).looking_at(Vec3::new(g("look_x", 0.0), g("look_y", 1.0), g("look_z", 0.0)), Vec3::Y),
                ));
                // three.js-style scenes (raea's 3D): a vertical field of view in degrees, and an ambient
                // light on the camera (brightness in cd/m²); without them the camera is as it always was
                if let Some(fov) = vals.get("fov").map(V::num) {
                    world.entity_mut(e).insert(Projection::Perspective(PerspectiveProjection { fov: (fov as f32).to_radians(), ..default() }));
                }
                if let Some(b) = vals.get("ambient").map(V::num) {
                    world.entity_mut(e).insert(AmbientLight { color: colour(vals.get("ambient_color"), [1.0, 1.0, 1.0, 1.0]), brightness: b as f32, affects_lightmapped_meshes: true });
                }
                // the lens and post-processing (bloom, fog, tonemapping, exposure): scene3d
                scene3d::camera(world, e, vals);
                // the prepasses motion blur reads (motion_blur.rs)
                motion_blur::load(world, e, vals);
                // an <Environment>'s panoramas, light and sky: environment
                environment::environment(world, e, vals, inserted, dir)?;
                // a window on a larger view (view_*): matchmove
                matchmove::camera_view(world, e, vals, Vec2::new(size.0 as f32, size.1 as f32));
    Ok(())
}

/// A live scene's rows applied to a World (Engine::set_rows, or a plain App's: docs/sdk.feature S6a): each row's values
/// merged into its latest ones, a row not seen before spawned as a load spawns it, and the component and resource rows
/// set through the registry. True when a glTF model was spawned, which the caller loads before the frame that shows it.
pub fn apply_rows(world: &mut World, dir: &Path, rows: &[Tup]) -> Result<bool, String> {
    if world.resource::<FrameRows>().live.is_none() {
        // the loaded rows' values as the timeline has them at the clock, in the store's order
        let clock = world.resource::<Clock>().0;
        let base = world.resource::<Edit>().timeline.cols(clock).to_tups().into_iter().map(|t| t.vals).collect();
        world.resource_mut::<FrameRows>().live = Some(base);
    }
    let media = world.resource::<Edit>().media.clone();
    let mut spawned = false;
    // component and resource rows a live frame sets: applied through the registry once their values are in
    let mut reflected: Vec<(String, String)> = Vec::new();
    for t in rows {
        if t.kind == "component" || t.kind == "resource" {
            reflected.push((t.kind.clone(), t.id.clone()));
        }
        match store::find(world, &t.kind, &t.id) {
            Some(e) => {
                let mut fr = world.resource_mut::<FrameRows>();
                let r = fr.ix.get(&e).copied();
                if let Some(row) = r.and_then(|r| fr.live.as_mut().and_then(|l| l.get_mut(r))) {
                    for (k, v) in &t.vals {
                        match row.get_mut(k) {
                            Some(x) => *x = v.clone(),
                            None => {
                                row.insert(k.clone(), v.clone());
                            }
                        }
                    }
                }
            }
            None => {
                let e = store::spawn(world, t);
                // a camera first seen here is a plain App's (SceneTarget); an Engine's comes with its load
                match world.get_resource::<SceneTarget>().map(|s| (s.target.clone(), s.size)) {
                    Some((target, size)) if t.kind == "camera" => spawn_camera(world, e, &t.vals, rows, dir, size, target, true)?,
                    _ => spawn_3d(world, e, t, &media, dir)?,
                }
                let r = store::rows(world).len() - 1;
                let mut fr = world.resource_mut::<FrameRows>();
                fr.ix.insert(e, r);
                if let Some(l) = fr.live.as_mut() {
                    l.push(t.vals.clone());
                }
                spawned |= t.kind == "model";
            }
        }
    }
    // a <BevyComponent> or <BevyResource> whose value a live frame changed (or a new one): set on its part
    plugins::apply_live_rows(world, &reflected)?;
    Ok(spawned)
}

/// Each screen's image by name, with its size (<Screen>, screen.rs): where a host drawing in the same World copies a
/// screen's picture each frame (sdk/react's WorldPagePlugin, a plain App's page), as Engine::copy_screen does in live
/// play.
pub fn screen_images(world: &World) -> Vec<(String, Handle<Image>, u32, u32)> {
    world.get_resource::<screen::ScreenImages>().map(|s| s.0.iter().map(|(n, (hd, w, h))| (n.clone(), hd.clone(), *w, *h)).collect()).unwrap_or_default()
}

/// A live scene's rows dropped from a World, by kind and id (Engine::remove_rows, or a plain App's): their entities go,
/// and the rows after them take their places.
pub fn remove_rows_from(world: &mut World, rows: &[(String, String)]) {
    // each dropped row's place as the rows stand now, then the places dropped from the last back, so a drop never
    // moves a place still to come. Before, each place was dropped as it was found: a second row dropped in the
    // same call took the place it had before the first moved it up, dropping its neighbour's values instead, and
    // every row after was drawn with the values of the row before it (The Hero's Journey's 375 words shown once
    // to make their meshes, dropped together on its third frame: its body gone and other parts drawn huge,
    // 2026-10-20)
    let mut places = Vec::new();
    for (kind, id) in rows {
        let Some(e) = store::find(world, kind, id) else { continue };
        let Some(r) = world.resource::<FrameRows>().ix.get(&e).copied() else { continue };
        store::despawn(world, e);
        places.push(r);
    }
    let any = !places.is_empty();
    if let Some(l) = world.resource_mut::<FrameRows>().live.as_mut() {
        drop_places(l, places);
    }
    if any {
        // the rows after a dropped one moved up: their places again, from the store's order
        let ix: bevy::ecs::entity::EntityHashMap<usize> = store::rows(world).iter().enumerate().map(|(r, &e)| (e, r)).collect();
        world.resource_mut::<FrameRows>().ix = ix;
    }
}

/// Where a plain App's 3D camera draws when its camera row arrives through apply_rows (an Engine's draws into its frame).
#[derive(Resource, Clone)]
pub struct SceneTarget {
    pub target: RenderTarget,
    pub size: (u32, u32),
}

pub(crate) fn spawn_3d(world: &mut World, e: Entity, t: &Tup, inserted: &[Tup], dir: &Path) -> Result<(), String> {
    let vals = &t.vals;
    let g = |k: &str, d: f32| num(vals, k, d);
    // its SDK name as Bevy's Name too: a BRP query, an observer, a saved world or a compute's mesh finds it by it
    if let Some(V::S(n)) = vals.get("name") {
        world.entity_mut(e).insert(Name::new(n.clone()));
    }
    match t.kind.as_str() {
        // directional (as before), point or spot: scene3d
        "light" => scene3d::light(world, e, vals),
        // and its cloth, when it has one (meshseq/cloth.rs)
        "model" => {
            scene3d::model(world, e, t, inserted, dir)?;
            meshseq::cloth::spawn(world, e, t, inserted, dir)?
        }
        // a picture pressed onto a surface (<Decal>): decal.rs
        "decal" => decal::spawn(world, e, t, inserted, dir)?,
        // a box of air a <VolumetricFog> draws: scene3d
        "fogvolume" => scene3d::fog_volume(world, e, &t.id, vals),
        // a flame drawn as a volume (<Fire>): fire.rs
        "fire" => fire::spawn(world, e, vals),
        // a field of grass (<Grass>): systems/grass
        "grass" => raea_sys_grass::spawn(world, e, vals)?,
        // gaussian splats (<Splat>): splat.rs
        "splat" => splat::spawn(world, e, t, inserted, dir)?,
        // a bare Bevy entity (<Entity>): placed, and given Bevy's own components by its component rows (plugins.rs)
        "entity" => {
            world.entity_mut(e).insert((transform_of(vals), Visibility::default(), BareEntity));
        }
        // text in the 3D world (<RichText>): its block laid out, then each glyph an entity under it (text3d.rs)
        "text" => text3d::spawn_text(world, e, vals)?,
        "glyph" => text3d::spawn_glyph(world, e, &t.id, vals)?,
        // a baked simulation, a mesh a frame (<MeshSequence>, <Goo>): meshseq.rs
        "meshseq" => meshseq::spawn(world, e, t, inserted, dir)?,
        // footage behind the 3D (<Plate>, <Matchmove>): matchmove.rs
        #[cfg(feature = "native")]
        "plate" => matchmove::spawn_plate(world, e, t, inserted, dir)?,
        "cube" | "sphere" | "plane" => {
            let size = g("size", 1.0);
            // shared as a mesh row's are (share.rs): one Mesh per kind and size, one material per value
            let mesh = scene3d::shared_mesh(world, &format!("{}\0{size}", t.kind), || {
                Ok(match t.kind.as_str() {
                    "cube" => Mesh::from(Cuboid::from_length(size)),
                    "sphere" => Mesh::from(Sphere::new(size / 2.0).mesh().ico(5).unwrap()),
                    _ => Mesh::from(Plane3d::default().mesh().size(size, size)),
                })
            })?;
            let (mat, shared) = scene3d::shared_material(world, StandardMaterial {
                base_color: colour(vals.get("color"), [0.8, 0.8, 0.8, 1.0]),
                perceptual_roughness: g("rough", 0.45),
                metallic: g("metal", 0.0),
                ..default()
            });
            world.entity_mut(e).insert((Mesh3d(mesh), MeshMaterial3d(mat), Transform::from_xyz(g("x", 0.0), g("y", 0.0), g("z", 0.0))));
            if shared {
                world.entity_mut(e).insert(scene3d::SharedMaterial);
            }
        }
        "mesh" => {
            // a shape as a value (docs/media.feature M2): its `shape` column, a CSG expression, built by Manifold
            // once per expression (shape::cached), flat-shaded
            let expr = match vals.get("shape") { Some(V::S(s)) => s.clone(), _ => return Err(format!("mesh {}: needs shape, as box(1, 1, 1) or minus(rounded_box(2, 1, 0.2, 0.1), text3d(\"SALE\", 0.45, 0.3))", t.id)) };
            // a picture map needs UVs, which Manifold doesn't make (scene3d); a normal map tangents (mikktspace), as a grid's outline
            let grid = grid::grid_of(vals);
            let uvs = scene3d::has_maps(vals) || grid.is_some();
            let tangents = raea_sys_material::maps::needs_tangents(vals);
            let attrs = match vals.get("attributes") { Some(V::S(a)) => a.as_str(), _ => "" };
            // made once per shape and what it's given (share.rs): equal parts hold one Mesh, so Bevy batches them
            let mesh = scene3d::shared_mesh(world, &format!("{expr}\0{uvs}\0{tangents}\0{attrs}"), || raea_sys_material::mesh::shape_mesh(&t.id, &expr, uvs, tangents).and_then(|m| raea_sys_material::mesh::with_attributes(m, attrs).map_err(|e| format!("mesh {}: {e}", t.id))))?;
            // a three.js shader (raea-sys-three-shader, docs/r3f.feature R3F11): its own material, and the mesh given
            // what it reads; its pictures' files found as a model's are (the edit's folder, its media)
            let three = match vals.get("three") {
                Some(V::S(spec)) => raea_sys_three_shader::of_row(world, &t.id, spec, |w, x| match x.as_str() {
                    Some(p) if !p.starts_with("render:") => plugins::image(w, &serde_json::Value::String(resolve_src(inserted, p, dir)?.to_string_lossy().into_owned())),
                    _ => plugins::image(w, x),
                }),
                _ => None,
            };
            let mesh = match &three {
                Some((_, own)) => {
                    let m = raea_sys_three_shader::fitted(world.resource::<Assets<Mesh>>().get(&mesh), &t.id, expr.strip_prefix("buffer:"), own)?;
                    scene3d::shared_mesh(world, &format!("{expr}\0{uvs}\0{tangents}\0three:{}", own.join(",")), || Ok(m))?
                }
                None => mesh,
            };
            // the surface (emissive, opacity, unlit, clearcoat, a picture map...) and transform: scene3d
            let mat = scene3d::material(world, t, inserted, dir)?;
            // rot_x/y/z in radians and scale (or scale_x/y/z), as sync_meshes moves every mesh frame by frame
            if let Some((m, _)) = three {
                grid::put(world, e, mesh, m, vals);
            } else if vals.get("shadow_only").is_some_and(V::truthy) {
                // `shadow: "only"`: in the shadow maps, never drawn (shadow_only.rs)
                grid::put(world, e, mesh, shadow_only::ShadowOnlyMaterial::default(), vals);
            } else if vals.get("shadow_catch").is_some_and(V::truthy) {
                // `shadow: "catch"`: only the shadows falling on it, in its colour and opacity (shadow_catch.rs)
                grid::put(world, e, mesh, shadow_catch::ShadowCatchMaterial::of(&mat), vals);
            } else if vals.contains_key("toon") {
                // a MeshToonMaterial: the surface lit in bands over the standard one, and its ink outline (toon.rs)
                world.entity_mut(e).insert((Mesh3d(mesh.clone()), transform_of(vals)));
                toon::spawn_mesh(world, e, vals, &mesh, mat);
            } else if let Some(extension) = grid {
                // graph paper over it (grid.rs): its colours fixed, so it isn't re-tinted frame by frame
                grid::put(world, e, mesh, grid::GridMaterial { base: mat, extension }, vals);
            } else if let Some(extension) = raea_sys_procmat::of(vals).filter(|_| !scene3d::clay::on()) {
                // a procedural surface (systems/procmat): its pattern, bump and use made on the GPU; clay draws it plain
                grid::put(world, e, mesh, raea_sys_procmat::ProcMaterial { base: mat, extension }, vals);
            } else if let Some(extension) = raea_sys_wear::of(vals).filter(|_| !scene3d::clay::on()) {
                // worn paint (systems/wear): chipped edges, grime and rust, fixed in the part's own space
                grid::put(world, e, mesh, raea_sys_wear::WearMaterial { base: mat, extension }, vals);
            } else if vals.get("shader").is_none() {
                // a plain surface, see-through too: one material per value, shared by the parts that have it (share.rs;
                // froxel_glass's fog copy is per material, so equal panes share it; mesh_ray_cast's six walls are one)
                let (mat, shared) = scene3d::shared_material(world, mat);
                world.entity_mut(e).insert((Mesh3d(mesh), MeshMaterial3d(mat), transform_of(vals)));
                if shared {
                    world.entity_mut(e).insert(scene3d::SharedMaterial);
                }
            } else {
                grid::put(world, e, mesh, mat, vals);
                // a shader over the lit surface (shader_ext.rs): put on once the rows are applied (scene3d's shade_rows)
                if let Some(V::S(sh)) = vals.get("shader") {
                    let spec = serde_json::from_str(sh).map_err(|err| format!("mesh {}: shader: {err}", t.id))?;
                    world.entity_mut(e).insert((scene3d::MeshShader { id: t.id.clone(), spec }, shader_ext::OwnSurface));
                }
            }
            // `shadow: false`: it casts none (a window's glass: a blended surface casts one otherwise)
            if vals.get("shadow").is_some_and(|v| !v.truthy()) {
                world.entity_mut(e).insert(bevy::light::NotShadowCaster);
            }
            // `receive: 0` (receiveShadow={false}): no shadow falls on it
            if vals.get("receive").is_some_and(|v| !v.truthy()) {
                world.entity_mut(e).insert(bevy::light::NotShadowReceiver);
            }
            layers(world, e, vals);
            // <Instances>: its mesh and material on one child a copy (scene3d's instance.rs)
            scene3d::instance_mesh(world, e, t)?;
        }
        // 2D over the 3D (<Sprite>, <Shape2D>): raea-sys-sprite; a sprite's picture found and decoded as a panel's
        "sprite" => {
            let src = match vals.get("src") { Some(V::S(s)) => s.clone(), _ => return Err(format!("sprite {}: needs src (a picture)", t.id)) };
            let pic = |s: &str| scene3d::panel_picture(s, inserted, dir).map(|p| (p.dimensions(), p.into_raw()));
            let (image, size) = sprite2d::source(world, &src, pic, plugins::image).map_err(|e| format!("sprite {}: {e}", t.id))?;
            sprite2d::spawn_sprite(world, e, t, image, size)?;
        }
        "shape2d" => {
            // its ColorMaterial's texture, found and decoded as a sprite's picture
            let tex = match vals.get("texture") {
                Some(V::S(s)) => { let pic = scene3d::panel_picture(s, inserted, dir).map_err(|e| format!("shape2d {}: {e}", t.id))?; let (pw, ph) = pic.dimensions(); Some(world.resource_mut::<Assets<Image>>().add(sprite2d::shape_image(pw, ph, pic.into_raw(), vals).map_err(|e| format!("shape2d {}: texture's {e}", t.id))?)) }
                _ => None,
            };
            sprite2d::spawn_shape(world, e, t, tex)?
        }
        // a live picture on a quad (<Screen>): screen.rs
        "panel" if vals.contains_key("screen") => screen::spawn_panel(world, e, t)?,
        // a screen's picture over a run of frames (raea's bevy_ui page): screen_page.rs
        "screenpage" => screen::screen_page::spawn(world, e, t, dir)?,
        "panel" => {
            // DOM on a surface (docs/bevy.feature B4): a picture painted once (src: a PNG, such as the
            // page a React DOM subtree painted) on a w x h quad in the scene. The GPU moves it, so its text is
            // never re-rasterised under a changing transform (no wobble); unlit, so it reads as painted.
            let src = match vals.get("src") { Some(V::S(s)) => s.clone(), _ => return Err(format!("panel {}: needs src (a picture)", t.id)) };
            // a picture that changes over the run (`srcs`, one a line, and the keyed `src_k`): scene3d::sync_pictures
            // swaps them; it starts on the one the row's first frame shows
            let pictures = match vals.get("srcs") {
                Some(V::S(list)) => {
                    let paths = list.lines().map(|s| if s.starts_with("data:") { Ok(PathBuf::from(s)) } else { resolve_src(inserted, s, dir) }).collect::<Result<Vec<_>, _>>()?;
                    Some(scene3d::Pictures { paths, shown: g("src_k", 0.0).round().max(0.0) as usize })
                }
                _ => None,
            };
            let src = match &pictures {
                Some(p) => p.paths.get(p.shown).map(|p| p.to_string_lossy().to_string()).unwrap_or(src),
                None => src,
            };
            let pic = scene3d::panel_picture(&src, inserted, dir).map_err(|e| format!("panel {}: {e}", t.id))?;
            let (pw, ph) = pic.dimensions();
            let tex = world.resource_mut::<Assets<Image>>().add(scene3d::panel_image(pic));
            if let Some(p) = pictures {
                world.entity_mut(e).insert(p);
            }
            // a sprite sheet shows one cell: its shape is a cell's (sync_meshes picks the cell, scene3d::sheet_uv)
            let (cols, rows) = (g("sheet_cols", 1.0).max(1.0), g("sheet_rows", 1.0).max(1.0));
            let (w, h) = (g("w", 1.0), g("h", g("w", 1.0) * (ph as f32 / rows) / (pw.max(1) as f32 / cols)));
            let mesh = world.resource_mut::<Assets<Mesh>>().add(Mesh::from(Rectangle::new(w, h)));
            let mat = world.resource_mut::<Assets<StandardMaterial>>().add(StandardMaterial {
                base_color_texture: Some(tex),
                unlit: true,
                // premultiplied (mip_chain): interpolating straight colour and alpha apart and multiplying them
                // after isn't linear in the sample position, so a moving edge's centroid wobbled (0.10 px). A cut-out
                // card (`cutout`) is no surface where its picture is clear, and shadows in its picture's shape
                alpha_mode: if g("cutout", 0.0) > 0.0 { AlphaMode::Mask(0.5) } else { AlphaMode::Premultiplied },
                cull_mode: None,
                double_sided: true,
                uv_transform: scene3d::sheet_uv(g("sheet_frame", 0.0), cols, rows),
                ..default()
            });
            world.entity_mut(e).insert((Mesh3d(mesh), MeshMaterial3d(mat), Transform::from_xyz(g("x", 0.0), g("y", 0.0), g("z", 0.0))));
            // `shadow: false`: a card that casts none (a cursor or a UI floating in the scene)
            if vals.get("shadow").is_some_and(|v| !v.truthy()) {
                world.entity_mut(e).insert(bevy::light::NotShadowCaster);
            }
        }
        _ => {}
    }
    Ok(())
}
