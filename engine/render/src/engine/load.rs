//! A program loaded into a running Engine: its rows spawned, its clips and media opened, its overlay keyed.

use super::*;

impl Engine {
    /// Re-mirror the tables into the World (after a move). The size must not change (make a new Engine for that).
    pub fn load(&mut self, prog: &Program, inserted: &[Tup]) -> Result<(), String> {
        self.load_shared(prog, inserted, None)
    }

    /// As load; a timeline passed in must be this edit's (its rows in the edit's order), and is used as it is.
    pub fn load_shared(&mut self, prog: &Program, inserted: &[Tup], timeline: Option<std::sync::Arc<raea_core_program::timeline::Timeline>>) -> Result<(), String> {
        if (prog.width, prog.height) != self.size() {
            return Err("the edit's size changed: make a new Engine".into());
        }
        if plugins::active(self.app.world()) {
            self.replay = Some((prog.clone(), inserted.to_vec()));
        }
        let world = self.app.world_mut();
        let g = world.resource::<Gen>().0;
        world.insert_resource(Gen(if g == 255 { 1 } else { g + 1 }));
        world.resource_mut::<Out>().0.clear();
        #[cfg(feature = "native")]
        world.insert_resource(OverlayKey::default());
        // the last load's material maps go with its meshes (scene3d); made only by a load that uses one
        if let Some(mut maps) = world.get_resource_mut::<scene3d::Maps>() {
            maps.0.clear();
        }
        #[cfg(feature = "native")]
        {
                self.warm = false;
        }
        // the rows go through the store; what else the last load spawned (overlay, tag, raea's leftovers) goes too
        store::clear(world);
        world.despawn_all::<With<Mirror>>();
        let ents = store::load(world, inserted);
        #[cfg(feature = "native")]
        let mut rows2d: Vec<(String, serde_json::Value, Entity)> = Vec::new();
        let mut camera = None;
        // an edit with no 3D or 2D mesh rows draws nothing through a mesh pipeline: its camera only clears, so it gets no
        // MSAA (4x targets at full size were most of the start-up memory); a 2D one's shares the 2D camera's (sprite)
        let has_3d = has_3d(inserted) || raea_sys_sprite::has_2d(inserted);
        // cameras first, so UI can target them; otherwise insertion order
        let mut order: Vec<(&Tup, Entity)> = inserted.iter().zip(ents.iter().copied()).collect();
        order.sort_by_key(|(t, _)| t.kind != "camera");
        for (t, e) in order {
            let vals = &t.vals;
            match t.kind.as_str() {
                "camera" => {
                    spawn_camera(world, e, vals, inserted, &self.dir, (prog.width, prog.height), RenderTarget::Image(self.target.clone().into()), has_3d)?;
                    camera.get_or_insert(e); // (the first camera: a second draws over it, two_passes)
                }
                k if is_3d_spawn(k) => {
                    spawn_3d(world, e, t, inserted, &self.dir)?;
                }
                // without native a clip or a 2D row is a row only (a browser's page draws its own video and text)
                #[cfg(feature = "native")]
                "clip" if world.resource::<Composite>().0 => {} // a row only: clips_gpu reads it
                #[cfg(feature = "native")]
                "clip" => {
                    if raea_runtime_view::masks::mask_of(vals).is_some() {
                        return Err(format!("clip {}: a mask needs the composite (kgpu) path, and this render draws clips as UI images", t.id));
                    }
                    let image = world.resource_mut::<Assets<Image>>().add(Image::new_fill(
                        bevy::render::render_resource::Extent3d { width: 1, height: 1, depth_or_array_layers: 1 },
                        bevy::render::render_resource::TextureDimension::D2,
                        &[0, 0, 0, 0],
                        TextureFormat::Rgba8UnormSrgb,
                        bevy::asset::RenderAssetUsages::default(),
                    ));
                    let Some(cam) = camera else { return Err("the edit needs a camera before its clips".into()) };
                    world.entity_mut(e).insert((
                        Node { position_type: PositionType::Absolute, ..default() },
                        ImageNode::new(image.clone()),
                        Clip { image, last: None },
                        Visibility::Hidden,
                        GlobalZIndex(0),
                        UiTargetCamera(cam),
                    ));
                }
                #[cfg(feature = "native")]
                k if KINDS_2D.contains(&k) => {
                    let (id, row) = row2d(t, inserted);
                    rows2d.push((id, row, e));
                }
                _ => {} // data kinds (face, cut, ...): rows only, read by rules
            }
        }
        // MotionBlur's previous poses: a pre-step before every update (motion_blur.rs)
        self.prestep = inserted.iter().any(|t| t.kind == "camera" && t.vals.contains_key("mb_on"));
        // plugin component and resource rows through the TypeRegistry, and the frame clock back at 0 (plugins.rs)
        plugins::apply_rows(world, inserted, &ents, &self.dir)?;
        // shaders over lit surfaces (docs/shaders.feature SH1-SH4), once their textures' "render:NAME" images exist
        world.remove_resource::<scene3d::SceneShader>();
        scene3d::shade_rows(world, inserted)?;
        // a camera on a bare <Entity> (a second view, a viewport) draws into the edit's frame unless its rows give it a
        // RenderTarget of its own (a "render:NAME" image)
        for (t, &e) in inserted.iter().zip(ents.iter()) {
            if t.kind != "entity" || world.get::<Camera>(e).is_none() {
                continue;
            }
            let own = inserted.iter().any(|c| c.kind == "component" && c.vals.get("target").is_some_and(|v| matches!(v, V::S(x) if *x == t.id)) && c.vals.get("type").is_some_and(|v| matches!(v, V::S(x) if x.ends_with("RenderTarget"))));
            if !own {
                world.entity_mut(e).insert(RenderTarget::Image(self.target.clone().into()));
            }
        }
        // the 2D rows drawn by bevy_ui over the 3D (rows2d.rs); each row's entity keeps its id and constants. None, no
        // page: an empty one was drawn and read back anyway (the fed render's 3D, whose 2D is raea's page; B1d)
        #[cfg(feature = "native")]
        if !rows2d.is_empty() {
            let instances: Vec<serde_json::Value> = rows2d.iter().map(|(_, r, _)| r.clone()).collect();
            let rows = raea_runtime_view::rows2d::Rows::new(&instances, prog.width, prog.height, prog.frames.max(1), &self.dir)?;
            for (id, _, e) in rows2d {
                let base = Base2d(rows.base(&id).cloned().unwrap_or_default());
                world.entity_mut(e).insert((base, Row2d(id)));
            }
            world.insert_non_send(Rows2d(rows));
        } else {
            world.remove_non_send::<Rows2d>();
        }
        let base = store::gather(world);
        let ix: bevy::ecs::entity::EntityHashMap<usize> = store::rows(world).iter().enumerate().map(|(r, &e)| (e, r)).collect();
        let media = inserted.iter().filter(|t| t.kind == "media").cloned().collect();
        let timeline = match timeline {
            Some(t) => t,
            None => std::sync::Arc::new(raea_core_program::timeline::Timeline::from_cols(prog, base)?),
        };
        world.insert_resource(Edit { prog: prog.clone(), timeline, media });
        world.insert_resource(FrameRows::new(ix));
        let Some(cam) = camera else { return Err("the edit needs a camera: INSERT INTO camera (id, ...) VALUES (...)".into()) };
        let models = world.query::<&scene3d::Model3d>().iter(world).count();
        let h = prog.height as f32;
        // the page overlay's texture, and a UI node showing it over the 3D when clips draw as UI (native only)
        #[cfg(feature = "native")]
        {
                let w = prog.width as f32;
                let mut img = Image::new_target_texture(prog.width, prog.height, TextureFormat::Rgba8UnormSrgb, None);
                img.texture_descriptor.usage |= TextureUsages::COPY_DST;
                let overlay = world.resource_mut::<Assets<Image>>().add(img);
                world.insert_resource(Overlay(overlay.clone()));
                if !world.resource::<Composite>().0 {
                    world.spawn((
                        Node { position_type: PositionType::Absolute, left: Val::Px(0.0), top: Val::Px(0.0), width: Val::Px(w), height: Val::Px(h), ..default() },
                        ImageNode::new(overlay),
                        GlobalZIndex(OVERLAY_Z),
                        UiTargetCamera(cam),
                        Mirror,
                    ));
                }
        }
        raea_sys_frame::spawn_tag(world, self.target.clone(), h);
        // glTF models loaded and set up before any frame draws
        if models > 0 {
            scene3d::await_models(&mut self.app)?;
        }
        // and the assets a component row's handle fields loaded by path (plugins.rs)
        plugins::await_loads(&mut self.app)?;
        Ok(())
    }
}
