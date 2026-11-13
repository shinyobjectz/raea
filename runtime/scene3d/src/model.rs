use super::*;

/// A model row: its glTF asked for (the engine waits for it before drawing, `await_models`).
pub fn model(world: &mut World, e: Entity, t: &Tup, inserted: &[Tup], dir: &Path) -> Result<(), String> {
    let src = match t.vals.get("src") { Some(V::S(s)) => s.clone(), _ => return Err(format!("model {}: needs src (a .glb or .gltf file)", t.id)) };
    let path = resolve_src(inserted, &src, dir)?;
    // in the browser (raea-web) a file is fetched by the asset server, so there's nothing on disk to look for
    if cfg!(not(target_arch = "wasm32")) && !path.exists() {
        return Err(format!("model {}: {} doesn't exist", t.id, path.display()));
    }
    let gltf: Handle<Gltf> = world.resource::<AssetServer>().load(path.to_string_lossy().to_string());
    let anim = t.vals.get("anim").cloned().filter(|v| v.truthy() || matches!(v, V::N(_)));
    let toon = super::toon::ToonSpec::of(&t.vals);
    let surfaces = super::surface::of(world, t, inserted, dir)?;
    let shaders = super::model_shaders(t)?;
    let sampler = raea_sys_sampler::Sampling::col(&t.vals, "sampler").map_err(|e| format!("model {}: {e}", t.id))?;
    let inst = model_placements(t)?;
    // placements that change over the shot: moved by sync_instances once the scene is copied to them
    if inst.is_some() {
        world.entity_mut(e).insert(Placements::of(t));
    }
    // `shadow={false}`, `receiveShadow={false}`: on the model, then on each of its meshes once spawned (model_shadow_markers)
    if t.vals.get("shadow").is_some_and(|v| !v.truthy()) {
        world.entity_mut(e).insert(bevy::light::NotShadowCaster);
    }
    if t.vals.get("receive").is_some_and(|v| !v.truthy()) {
        world.entity_mut(e).insert(bevy::light::NotShadowReceiver);
    }
    // a model that keeps its own surfaces (drawn flat, in a look, or toon) isn't given a scene's <SurfaceShader>
    if ["flat", "look", "toon"].iter().any(|k| t.vals.get(*k).is_some_and(V::truthy)) {
        world.entity_mut(e).insert(raea_sys_shader::OwnSurface);
    }
    world.entity_mut(e).insert((Model3d { gltf, scene: num(&t.vals, "scene", 0.0) as usize, anim, state: ModelState::Loading, src, id: t.id.clone(), toon, toon_wait: 0, surfaces, shaders, sampler, mix: names_col(&t.vals, "mix"), mixer: vec![], morphs: names_col(&t.vals, "morph_names"), graph: graph_col(&t.vals).map_err(|e| format!("model {}: {e}", t.id))?, built: None, inst }, transform_of(&t.vals), Visibility::default()));
    Ok(())
}

/// Models along: loaded, then their scene spawned, then (a few updates later, once the scene's entities exist) their
/// clip found and set playing, paused: `sync_models` seeks it to each frame's time.
pub fn model_setup(
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
    targets: Query<(Option<&Name>, Option<&bevy::animation::AnimationTargetId>)>,
    spawned: Query<(), With<SceneSpawned>>,
) {
    for (e, mut m) in &mut models {
        let was = m.ready();
        // every way out of a step (a model failed, still loading, or ready) comes to ModelReady below
        'step: {
            match m.state {
                ModelState::Loading => {
                    if let Some(bevy::asset::LoadState::Failed(why)) = server.get_load_state(&m.gltf) {
                        err.0 = Some(format!("model {}: {why}", m.src));
                        m.state = ModelState::Ready { player: None };
                        break 'step;
                    }
                    // a texture that fails (a JPEG without the jpeg feature, a missing file) fails the model now, with
                    // its reason, rather than leaving it loading until await_models gives up after 60 s
                    if let Some(bevy::asset::RecursiveDependencyLoadState::Failed(why)) = server.get_recursive_dependency_load_state(&m.gltf) {
                        err.0 = Some(format!("model {}: {why}", m.src));
                        m.state = ModelState::Ready { player: None };
                        break 'step;
                    }
                    if !server.is_loaded_with_dependencies(&m.gltf) {
                        break 'step;
                    }
                    let Some(g) = gltfs.get(&m.gltf) else { break 'step };
                    let Some(scene) = g.scenes.get(m.scene).or(g.default_scene.as_ref()).cloned() else {
                        err.0 = Some(format!("model {}: has no scene {}", m.src, m.scene));
                        m.state = ModelState::Ready { player: None };
                        break 'step;
                    };
                    normalise_joint_weights(g, &gmeshes, &mut meshes);
                    mip_gltf_textures(g, &mats, &mut images);
                    // Bevy says when the scene's whole hierarchy exists (WorldInstanceReady), rather than raea counting frames
                    let child = commands
                        .spawn((WorldAssetRoot(scene), Transform::default(), Visibility::Inherited))
                        .observe(|ready: On<bevy::world_serialization::WorldInstanceReady>, mut c: Commands| {
                            c.entity(ready.event_target()).insert(SceneSpawned);
                        })
                        .id();
                    commands.entity(e).add_child(child);
                    m.state = ModelState::Spawned(child);
                }
                ModelState::Spawned(root) => {
                    if !spawned.contains(root) {
                        break 'step;
                    }
                    // the scene's entities, then the player among them (the glTF puts one on the animated root)
                    let found = descendants(e, &kids).into_iter().find(|d| players.contains(*d));
                    let first_clip = m.graph.iter().find_map(|g| g.clip.clone());
                    let Some(anim) = m.anim.clone().or_else(|| m.mix.first().cloned()).or(first_clip) else {
                        m.state = ModelState::Ready { player: None };
                        break 'step;
                    };
                    let Some(p) = found else {
                        err.0 = Some(format!("model {}: asked for clip {anim:?}, but it has no animated parts", m.src));
                        m.state = ModelState::Ready { player: None };
                        break 'step;
                    };
                    let Some(g) = gltfs.get(&m.gltf) else { break 'step };
                    let find = |anim: &V| match anim {
                        V::S(name) => g.named_animations.get(name.as_str()).cloned().ok_or_else(|| {
                            let mut names: Vec<&str> = g.named_animations.keys().map(|k| k.as_ref()).collect();
                            names.sort();
                            format!("model {}: no clip \"{name}\" (it has {})", m.src, names.join(", "))
                        }),
                        V::N(i) => g.animations.get(*i as usize).cloned().ok_or_else(|| format!("model {}: no clip {i} (it has {})", m.src, g.animations.len())),
                    };
                    // an authored graph (<Model graph>): blend, additive and clip nodes as written, each masked off the
                    // subtrees of the bones it names (one mask group per masked node)
                    if !m.graph.is_empty() {
                        let mut graph = AnimationGraph::new();
                        let mut made: Vec<(AnimationNodeIndex, Option<Handle<bevy::animation::AnimationClip>>)> = vec![];
                        let mut group = 0u32;
                        let mut failed = None;
                        let all = descendants(e, &kids);
                        for (i, part) in m.graph.clone().iter().enumerate() {
                            let parent = part.parent.map(|p| made[p].0).unwrap_or(graph.root);
                            let mut mask = 0u64;
                            if !part.mask.is_empty() {
                                if group >= 64 {
                                    failed = Some(format!("model {}: more than 64 masked graph nodes", m.src));
                                    break;
                                }
                                for bone in &part.mask {
                                    let Some(b) = all.iter().copied().find(|d| targets.get(*d).ok().and_then(|t| t.0).is_some_and(|n| n.as_str() == bone)) else {
                                        failed = Some(format!("model {}: graph node {i}'s mask names {bone:?}, and no node of the model has that name", m.src));
                                        break;
                                    };
                                    for d in std::iter::once(b).chain(descendants(b, &kids)) {
                                        if let Ok((_, Some(id))) = targets.get(d) {
                                            graph.add_target_to_mask_group(*id, group);
                                        }
                                    }
                                }
                                mask = 1u64 << group;
                                group += 1;
                            }
                            let node = match part.kind.as_str() {
                                "clip" => match find(part.clip.as_ref().unwrap()) {
                                    Ok(c) => {
                                        let n = graph.add_clip_with_mask(c.clone(), mask, 1.0, parent);
                                        made.push((n, Some(c)));
                                        continue;
                                    }
                                    Err(why) => {
                                        failed = Some(why);
                                        break;
                                    }
                                },
                                "blend" => graph.add_blend_with_mask(mask, 1.0, parent),
                                _ => graph.add_additive_blend_with_mask(mask, 1.0, parent),
                            };
                            made.push((node, None));
                        }
                        if let Some(why) = failed {
                            err.0 = Some(why);
                            m.state = ModelState::Ready { player: None };
                            break 'step;
                        }
                        let h = graphs.add(graph);
                        commands.entity(p).insert(AnimationGraphHandle(h.clone()));
                        if let Ok(mut pl) = players.get_mut(p) {
                            for (n, c) in &made {
                                if c.is_some() {
                                    pl.play(*n).repeat().pause();
                                }
                            }
                        }
                        let (n0, c0) = made.iter().find(|(_, c)| c.is_some()).cloned().unwrap();
                        m.built = Some((h, made));
                        m.state = ModelState::Ready { player: Some((p, n0, c0.unwrap())) };
                        break 'step;
                    }
                    // several clips blended (<Model animations>): one graph with a node each under its blend root, all
                    // playing paused; sync_models seeks and weights each a frame
                    if m.anim.is_none() && !m.mix.is_empty() {
                        let clips = match m.mix.iter().map(&find).collect::<Result<Vec<_>, _>>() {
                            Ok(c) => c,
                            Err(why) => {
                                err.0 = Some(why);
                                m.state = ModelState::Ready { player: None };
                                break 'step;
                            }
                        };
                        let (graph, nodes) = AnimationGraph::from_clips(clips.iter().cloned());
                        commands.entity(p).insert(AnimationGraphHandle(graphs.add(graph)));
                        if let Ok(mut pl) = players.get_mut(p) {
                            for &n in &nodes {
                                pl.play(n).repeat().pause();
                            }
                        }
                        m.mixer = nodes.iter().copied().zip(clips.iter().cloned()).collect();
                        m.state = ModelState::Ready { player: Some((p, nodes[0], clips[0].clone())) };
                        break 'step;
                    }
                    let clip = match find(&anim) {
                        Ok(c) => c,
                        Err(why) => {
                            err.0 = Some(why);
                            m.state = ModelState::Ready { player: None };
                            break 'step;
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
        if !was && m.ready() {
            commands.entity(e).insert(ModelReady);
        }
    }
}

/// Skin weights scaled to sum to 1, as three.js's GLTFLoader does (`normalizeSkinWeights`). glTF requires the sum to
/// be 1 and Bevy draws the weights as written, so a file exported with sums above 1 (RobotExpressive's hands reach
/// 1.75) stretches its skinned parts into spikes. Only meshes whose sums are off are touched.
pub fn normalise_joint_weights(g: &Gltf, gmeshes: &Assets<bevy::gltf::GltfMesh>, meshes: &mut Assets<Mesh>) {
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

/// Models with a MeshToonMaterial: once ready, their meshes' materials swapped for toon ones (toon::apply_to_model),
/// and the spec cleared. A model whose scene shows no meshes for 30 updates after it's ready is left as it is.
pub fn toon_models(world: &mut World) {
    // a <Model sampler> over its own glTF textures, before anything re-covers them (raea-sys-sampler sets it once loaded)
    let mut q = world.query::<(Entity, &mut Model3d)>();
    let todo: Vec<(Entity, raea_sys_sampler::Sampling, String)> = q.iter_mut(world).filter(|(_, m)| m.sampler.is_some() && m.ready()).map(|(e, mut m)| (e, m.sampler.take().unwrap(), m.src.clone())).collect();
    for (e, s, src) in todo {
        let mut stack = vec![e];
        let mut texs: Vec<Handle<Image>> = Vec::new();
        while let Some(x) = stack.pop() {
            if let Some(c) = world.get::<Children>(x) {
                stack.extend(c.iter());
            }
            let Some(h) = world.get::<MeshMaterial3d<StandardMaterial>>(x).map(|m| m.0.clone()) else { continue };
            let Some(m) = world.resource::<Assets<StandardMaterial>>().get(&h) else { continue };
            for t in [&m.base_color_texture, &m.emissive_texture, &m.metallic_roughness_texture, &m.normal_map_texture, &m.occlusion_texture].into_iter().flatten() {
                if !texs.contains(t) {
                    texs.push(t.clone());
                }
            }
        }
        for t in texs {
            raea_sys_sampler::PendingSamplers::give(world, t, s.clone(), format!("model {src}"));
        }
    }
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
                m.toon_wait = 0;
            }
        }
    }
    // then their shaders over what's there (shaded.rs), and a scene's <SurfaceShader> once every model is settled
    let mut q = world.query::<(Entity, &mut Model3d)>();
    let todo: Vec<(Entity, Vec<(Option<String>, serde_json::Value)>)> = q.iter_mut(world).filter(|(_, m)| !m.shaders.is_empty() && m.toon.is_empty() && m.surfaces.is_empty() && m.ready()).map(|(e, m)| (e, m.shaders.clone())).collect();
    for (e, specs) in todo {
        let done = match super::shade_model(world, e, &specs) {
            Ok(d) => d,
            Err(err) => {
                let id = world.get::<Model3d>(e).map(|m| m.id.clone()).unwrap_or_default();
                world.resource_mut::<Error>().0.get_or_insert(format!("model {id}: shader: {err}"));
                true
            }
        };
        if let Some(mut m) = world.get_mut::<Model3d>(e) {
            m.toon_wait += 1;
            if done || m.toon_wait > 30 {
                m.shaders.clear();
                m.toon_wait = 0;
            }
        }
    }
    super::shade_scene(world);
}

/// A browser loads models on its own event loop, between frames: a live frame there draws the models that are ready.
#[cfg(target_arch = "wasm32")]
pub fn await_models(_app: &mut App) -> Result<(), String> {
    Ok(())
}

/// Every model ready (loaded, spawned, its clip set up, its toon surfaces on) before the first frame is drawn: a frame
/// never shows a model half-loaded. It gives up when no model has moved a stage (loaded, spawned, ready, surfaces on) for
/// 60 s, not after 60 s in all: prechorus's 477 models load for about a minute and then turn ready together, so at load
/// 13-20 a count of ready models alone refused it half the time (10-26).
#[cfg(not(target_arch = "wasm32"))]
pub fn await_models(app: &mut App) -> Result<(), String> {
    let mut t0 = bevy::platform::time::Instant::now();
    let t_all = t0;
    let mut most = 0usize;
    // the cameras off while models load, so the draws are binned once, whole (#48): raea-sys-post's views.rs
    let mut off: Option<Vec<Entity>> = None;
    // RAEA_MODELS_TRACE=1: each wait that ran an update says how many and for which models (a render's frame time)
    let trace = std::env::var("RAEA_MODELS_TRACE").as_deref() == Ok("1");
    let mut pumped = 0usize;
    loop {
        let world = app.world_mut();
        if let Some(e) = world.resource_mut::<Error>().0.take() {
            return Err(e);
        }
        toon_models(world);
        let mut q = world.query::<&Model3d>();
        // each model's stage: loading 0, spawned 1, ready 2, ready with its toon and surfaces on 3
        let (mut waiting, mut done, mut stages) = (0usize, 0usize, [0usize; 3]);
        for m in q.iter(world) {
            let s = match m.state {
                ModelState::Loading => 0,
                ModelState::Spawned(_) => 1,
                ModelState::Ready { .. } if m.settled() => 3,
                ModelState::Ready { .. } => 2,
            };
            if s < 3 {
                waiting += 1;
                stages[s] += 1;
            }
            done += s;
        }
        if waiting == 0 {
            if let Some(cams) = off { raea_sys_post::views::on(app, cams); }
            if trace && pumped > 0 {
                eprintln!("raea models: waited {pumped} update(s), {:.1} ms", t_all.elapsed().as_secs_f64() * 1e3);
            }
            return Ok(());
        }
        if trace && pumped == 0 {
            let mut q = app.world_mut().query::<&Model3d>();
            let who: Vec<String> = q.iter(app.world()).filter(|m| !(matches!(m.state, ModelState::Ready { .. }) && m.settled())).map(|m| format!("{} ({}; surfaces {}, toon {}, shaders {})", m.id, match m.state { ModelState::Loading => "loading", ModelState::Spawned(_) => "spawned", ModelState::Ready { .. } => "ready" }, m.surfaces.len(), m.toon.len(), m.shaders.len())).take(4).collect();
            eprintln!("raea models: waiting on {waiting}: {}", who.join("; "));
        }
        pumped += 1;
        if done > most {
            most = done;
            t0 = bevy::platform::time::Instant::now();
        }
        if t0.elapsed().as_secs_f64() > 60.0 {
            let [l, s, r] = stages;
            return Err(format!("{waiting} model(s) didn't load: none moved a stage in 60 s ({l} loading, {s} spawning, {r} ready without their surfaces)"));
        }
        if off.is_none() { off = Some(raea_sys_post::views::off(app.world_mut())); }
        app.update();
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
}
