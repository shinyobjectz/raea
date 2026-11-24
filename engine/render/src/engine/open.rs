//! An Engine made: its Bevy App, plugins, target and readback, built for a program (`build`).

use super::*;

/// An Engine half made: its App's plugins added, the renderer not yet known to be up. Natively it always is; in a
/// browser the GPU device comes asynchronously, so the page asks `ready` each animation frame before `finish`.
pub struct Opening {
    app: App,
    prog: Program,
    inserted: Vec<Tup>,
    dir: PathBuf,
    timeline: Option<std::sync::Arc<raea_core_program::timeline::Timeline>>,
    live: bool,
}

impl Opening {
    /// The App's plugins are ready to finish (the render device made).
    pub fn ready(&mut self) -> bool {
        self.app.plugins_state() == bevy::app::PluginsState::Ready
    }

    /// The App's World before it's finished (raea-web adds its canvas window here).
    pub fn world_mut(&mut self) -> &mut World {
        self.app.world_mut()
    }

    /// The rest of the Engine made: plugins finished, target, readback, the program loaded.
    pub fn finish(self) -> Result<Engine, String> {
        let Opening { app, prog, inserted, dir, timeline, live } = self;
        Engine::finish_build(app, &prog, &inserted, &dir, timeline, live)
    }
}

impl Engine {
    pub(crate) fn build(prog: &Program, inserted: &[Tup], dir: &Path, timeline: Option<std::sync::Arc<raea_core_program::timeline::Timeline>>, live: bool) -> Result<Engine, String> {
        Engine::begin(prog, inserted, dir, timeline, live)?.finish()
    }

    /// An Engine started, its plugins added: finished by `Opening::finish` once `ready` (a browser's GPU device comes
    /// asynchronously). `live` as `new_live`'s (raea-web passes no timeline and true).
    pub fn begin(prog: &Program, inserted: &[Tup], dir: &Path, timeline: Option<std::sync::Arc<raea_core_program::timeline::Timeline>>, live: bool) -> Result<Opening, String> {
        mem_mark("engine: new");
        let mut app = App::new();
        let group = DefaultPlugins
            .set(WindowPlugin { primary_window: None, exit_condition: ExitCondition::DontExit, close_when_requested: false, ..default() })
            // quiet on a good run: bevy_log's "Could not set global logger" (raea_io_trace owns `log` in a play or a render, and its
            // records go there, as meant), a pass-timing readback that missed a frame (RenderDiagnosticsPlugin's "Failed to download render
            // statistics buffer": one second's bevy_passes lost) and bevy_time's "did not receive the time from the render world" (raea steps Time by frame)
            .set(bevy::log::LogPlugin { level: bevy::log::Level::WARN, filter: format!("{}bevy_log=off,bevy_asset::io::web=error,bevy_asset::io::source=error,fontdb=error,bevy_render::diagnostic::internal=error,bevy_time=error", bevy::log::DEFAULT_FILTER), ..default() })
            // glTF models (scene3d) load from the paths their rows name, which are the edit's files wherever they are.
            // Hot reload (Bevy's file_watcher, in the bevy-all build) only when asked, RAEA_BEVY_WATCH=1: a render or a
            // baked play reloading an asset mid-run would draw frames that a render alone doesn't
            .set(bevy::asset::AssetPlugin { unapproved_path_mode: bevy::asset::UnapprovedPathMode::Allow, meta_check: bevy::asset::AssetMetaCheck::Never, watch_for_changes_override: Some(std::env::var_os("RAEA_BEVY_WATCH").is_some()), ..default() })
            // pipelines compile on the render thread as a frame queues them, as Bevy always compiles them on macOS. On
            // its task pool (Windows and Linux only) wait_pipelines held the first frame for pipelines the pool left
            // unstarted, its threads idle: 3-43 s stalls on Windows, and opens that never finished (`raea play`'s hang
            // in v0.1.0's Windows live play; 32 of 32 opened in under 0.7 s compiled here, cuda-box 11-09).
            // RAEA_BEVY_ASYNC_PIPELINES=1 compiles them on the task pool (lightspeed F5, measured when macOS had it)
            .set(bevy::render::RenderPlugin { synchronous_pipeline_compilation: std::env::var_os("RAEA_BEVY_ASYNC_PIPELINES").is_none(), render_creation: render_creation(), ..default() });
        // no render thread: a frame is drawn by the update that asks for it (wasm32 has no pipelined rendering to turn off)
        #[cfg(not(target_arch = "wasm32"))]
        let group = group.disable::<bevy::render::pipelined_rendering::PipelinedRenderingPlugin>();
        // the registered plugins' changes (of the catalog's, those the project chose: nobuild NB4a), no WinitPlugin (no event loop, #182)
        { let chosen = plugins::choose(prog, dir); app.add_plugins(plugins::group_chosen(group, &chosen)).insert_resource(chosen); }
        #[cfg(feature = "raytracing")]
        if raea_render_raytrace::wanted(prog) {
            raea_render_raytrace::add(&mut app);
            app.add_systems(PostUpdate, solari_cut.after(raea_sys_post::cut::DetectCuts));
        }
        app
        .init_resource::<Clock>()
        .init_resource::<Out>()
        .init_resource::<Error>()
        .init_resource::<Gen>()
        .init_resource::<PreStep>()
        .init_resource::<Stages>()
        .insert_resource(bevy::render::error_handler::RenderErrorHandler(raea_sys_frame::gpu::gpu_error));
        // the frame in named sets (FrameSystems), in this order
        #[cfg(feature = "native")]
        {
            use FrameSystems as F;
            app.init_resource::<OverlayKey>()
                .configure_sets(Update, (F::Start, F::Rows, F::Scene3d, F::Clips, F::Overlay, F::Tag, F::End).chain())
                .add_systems(
                    Update,
                    (
                        mark_main_start.in_set(F::Start),
                        relational.in_set(F::Rows),
                        raea_plugins::scene3d_systems().in_set(Scene3dSystems).in_set(F::Scene3d),
                        (clips.run_if(on_ui), clips_gpu.run_if(on_gpu)).chain().in_set(F::Clips),
                        (overlay_key, (mark_overlay_start, overlay_rows, mark_overlay_end).chain().run_if(overlay_dirty)).chain().in_set(F::Overlay),
                        tag.in_set(F::Tag),
                        mark_main_end.in_set(F::End),
                    ),
                );
        }
        // a camera's roll and window on a larger view after the camera is placed and before the looks copy it, then
        // footage behind the 3D where the camera now is: matchmove
        app.add_systems(Update, matchmove::sync_view.after(scene3d::sync_camera).before(look::sync_looks));
        #[cfg(feature = "native")]
        app.add_systems(Update, matchmove::sync_plates.after(matchmove::sync_view).before(clips_gpu));
        // without native the same 3D systems in the same order, and no clips or page overlay (a browser's page is its
        // own DOM)
        #[cfg(not(feature = "native"))]
        {
            use FrameSystems as F;
            app.configure_sets(Update, (F::Start, F::Rows, F::Scene3d, F::Tag, F::End).chain()).add_systems(
                Update,
                (
                    mark_main_start.in_set(F::Start),
                    relational.in_set(F::Rows),
                    raea_plugins::scene3d_systems().in_set(Scene3dSystems).in_set(F::Scene3d),
                    tag.in_set(F::Tag),
                    mark_main_end.in_set(F::End),
                ),
            );
        }
        // each render pass's time (CPU side on Metal, where wgpu has no timestamps inside passes), into the run's trace
        // once a second as `bevy_passes`: a slow frame shows which of Bevy's passes it was. The stat channel bevy_passes only: its
        // statistics buffer is read back every frame, and play and a render's page path read nothing back (docs/blitz-
        // bevy.feature B1d)
        #[cfg(feature = "native")]
        if raea_io_trace::stat("RAEA_BEVY_PASSES") {
            app.add_plugins(bevy::render::diagnostic::RenderDiagnosticsPlugin).add_systems(Last, forward_render_diagnostics);
        }
        // raea's modules as Bevy plugins, in the order they were installed one by one (raea_plugins.rs), then the
        // registered Bevy plugins and the frame clock they step by (plugins.rs; nothing when none are)
        app.add_plugins((raea_plugins::RaeaPlugins, raea_plugins::RegisteredPlugins { fps: prog.fps }));
        app.sub_app_mut(bevy::render::RenderApp).add_systems(
            bevy::render::Render,
            wait_pipelines.in_set(bevy::render::RenderSystems::Render).before(bevy::render::renderer::render_system),
        );
        // overlapping fog volumes composited in one order whatever frames came before (raea-sys-fog's FogRank)
        app.sub_app_mut(bevy::render::RenderApp)
            .init_resource::<scene3d::FogOrder>()
            .add_systems(bevy::render::ExtractSchedule, scene3d::extract_fog_order)
            .add_systems(bevy::render::Render, scene3d::order_fog_volumes.after(bevy::render::RenderSystems::ExtractCommands).before(bevy::render::RenderSystems::PrepareAssets));
        // the clips' composite and the NV12 readback: GPU kernels (raea-gpu-device), native only
        #[cfg(feature = "native")]
        {
                // a render's stills composite on the GPU too (sRGB-encoded blending, as Chrome's; bevy_ui's overlay blends
                // in linear light), then come back as RGBA rather than NV12; a live engine's stills (the page over its 3D)
                // keep bevy_ui's composite, and RAEA_READBACK=rgba reads the target as bevy_ui drew it
                let stills = STILLS_RGBA.load(std::sync::atomic::Ordering::Relaxed);
                let composite = (!stills || !live) && std::env::var("RAEA_READBACK").map(|v| v != "rgba").unwrap_or(true) && std::env::var("RAEA_COMPOSITE").map(|v| v != "ui").unwrap_or(true);
                app.insert_resource(Composite(composite));
                app.add_plugins(ExtractResourcePlugin::<Nv12Job>::default());
                if composite {
                    app.init_resource::<GpuFrame>().init_resource::<PoolBook>().add_plugins(ExtractResourcePlugin::<GpuFrame>::default());
                    app.sub_app_mut(bevy::render::RenderApp).init_resource::<PoolGpu>().add_systems(
                        bevy::render::renderer::RenderGraph,
                        gpu_frame_pass.in_set(bevy::render::renderer::RenderGraphSystems::Render).after(bevy::core_pipeline::schedule::camera_driver),
                    );
                } else {
                    app.sub_app_mut(bevy::render::RenderApp).init_resource::<Nv12Gpu>().add_systems(
                        bevy::render::renderer::RenderGraph,
                        nv12_pass.in_set(bevy::render::renderer::RenderGraphSystems::Render).after(bevy::core_pipeline::schedule::camera_driver),
                    );
                }
        }
        // RAEA_MEM_MARKS: the render world's sets, marked through the first updates (where start-up memory goes)
        if super::stat("RAEA_MEM_MARKS") {
            use bevy::render::{Render, RenderSystems as RS};
            fn at(name: &'static str) -> impl FnMut() {
                let mut n = 0u32;
                move || {
                    if n < 3 {
                        mem_mark(&format!("render world: {name} (update {n})"));
                    }
                    n += 1;
                }
            }
            app.sub_app_mut(bevy::render::RenderApp).add_systems(
                Render,
                (
                    at("start").before(RS::ExtractCommands),
                    at("assets prepared").after(RS::PrepareAssets).before(RS::CreateViews),
                    at("views made").after(RS::CreateViews).before(RS::Specialize),
                    at("specialized").after(RS::Specialize).before(RS::PrepareViews),
                    at("views prepared").after(RS::PrepareViews).before(RS::Queue),
                    at("queued").after(RS::QueueSweep).before(RS::PhaseSort),
                    at("resources prepared").after(RS::PrepareResourcesFlush).before(RS::PrepareBindGroups),
                    at("bind groups").after(RS::PrepareBindGroups).before(RS::Render),
                    at("rendered").after(RS::Render),
                ),
            );
        }
        #[cfg(feature = "native")]
        {
                let backend = raea_io_media::backend(&media_backend())?;
                app.insert_non_send(Media { backend, decoders: HashMap::new(), dir: dir.to_path_buf() });
        }
        raea_sys_hold::install(&mut app, live); // a part held by a hand, and wireframe (docs/scene-feed.feature SF4c, SF3e)
        mem_mark("engine: plugins built (DefaultPlugins, the render device)");
        Ok(Opening { app, prog: prog.clone(), inserted: inserted.to_vec(), dir: dir.to_path_buf(), timeline, live })
    }

    fn finish_build(mut app: App, prog: &Program, inserted: &[Tup], dir: &Path, timeline: Option<std::sync::Arc<raea_core_program::timeline::Timeline>>, live: bool) -> Result<Engine, String> {
        let ss = supersampling(live);
        let (w, h) = (prog.width * ss, prog.height * ss);
        // every plugin has embedded its shaders by now: a project's engine-shaders/ and RAEA_ENGINE_SHADERS replace them
        raea_sys_engine_shaders::shadow(&mut app, dir, prog);
        app.finish();
        mem_mark("engine: plugins finished");
        // RT4: MetalFX's denoiser goes in after Solari's lighting, which Solari's plugin adds as it finishes
        #[cfg(feature = "raytracing")]
        if raea_render_raytrace::wanted(prog) {
            raea_render_raytrace::add_denoiser(&mut app);
        }
        // lights clustered on the CPU, not by Bevy 0.19's GPU clustering: on a fresh engine's first frames the GPU's
        // cluster lists left a spot light out (its fog shaft drawn, its light on no surface), for as many updates as its
        // readback took, so a render (a fresh engine a frame) missed it where play, warm, had it: volumetric-grid's
        // render against its play 0.964 -> 1.000; volumetric-froxel's and the rome film's stills dropped it in bursts
        // under load (2026-10-20). RAEA_GPU_CLUSTERING=1 brings Bevy's back
        if std::env::var("RAEA_GPU_CLUSTERING").as_deref() != Ok("1") {
            app.world_mut().resource_mut::<bevy::light::cluster::GlobalClusterSettings>().gpu_clustering = None;
        }
        app.cleanup();
        mem_mark("engine: plugins cleaned up");
        // supersampled, the UI (the page's nodes, the frame tag) laid out at the edit's size and scaled up with the frame
        app.insert_resource(bevy::ui::UiScale(ss as f32));
        let mut image = Image::new_target_texture(w, h + 1, TextureFormat::Rgba8UnormSrgb, None);
        image.texture_descriptor.usage |= TextureUsages::COPY_SRC | TextureUsages::TEXTURE_BINDING;
        // a Unorm view lets the NV12 kernel read the stored bytes (an sRGB view would decode them to linear)
        image.texture_descriptor.view_formats = &[TextureFormat::Rgba8Unorm];
        let target = app.world_mut().resource_mut::<Assets<Image>>().add(image);
        app.insert_resource(TagRowTarget(target.clone())).init_resource::<ViewBox>().add_systems(PostUpdate, tag_row_view.before(bevy::camera::CameraUpdateSystems));
        // a free camera over the scene (docs/preview.feature PV1c): off until a preview asks for one
        app.init_resource::<raea_sys_frame::view::FreeView>().add_systems(PostUpdate, raea_sys_frame::view::apply.before(bevy::transform::TransformSystems::Propagate));
        // without native nothing ever comes back (frames are read back only to be encoded or analysed)
        let nv12 = !STILLS_RGBA.load(std::sync::atomic::Ordering::Relaxed) && std::env::var("RAEA_READBACK").map(|v| v != "rgba").unwrap_or(true);
        // stills through the GPU composite: its RGBA words and the tag word, unpadded
        #[cfg(feature = "native")]
        let packed = !nv12 && app.world().get_resource::<Composite>().is_some_and(|c| c.0);
        #[cfg(not(feature = "native"))]
        let packed = false;
        #[cfg(feature = "native")]
        if live {
            // nothing comes back: the target is sampled where it is (Engine::texture)
        } else if nv12 || packed {
            let words = if nv12 { nv12_words(w, h) } else { packed_tag(w, h) / 4 + 1 };
            let out = app
                .world_mut()
                .resource_mut::<Assets<ShaderBuffer>>()
                .add(ShaderBuffer::with_size((words * 4) as u64, bevy::asset::RenderAssetUsages::RENDER_WORLD));
            app.world_mut().insert_resource(Nv12Job { target: target.clone(), out: out.clone(), w, h, rgba: packed });
            app.world_mut().spawn(Readback::buffer(out)).observe(|ev: On<ReadbackComplete>, mut out: ResMut<Out>| out.0.push(ev.data.clone()));
        } else {
            app.world_mut().spawn(Readback::texture(target.clone())).observe(|ev: On<ReadbackComplete>, mut out: ResMut<Out>| out.0.push(ev.data.clone()));
        }
        let mut e = Engine { app, target, w, h, ss, dir: dir.to_path_buf(), #[cfg(feature = "native")] warm: false, #[cfg(feature = "native")] serial: std::env::var_os("RAEA_SERIAL").is_some(), nv12, packed, replay: None, live, prestep: false, pipe: Default::default(), drawn: None, warm_next: false, kept: Default::default() };
        mem_mark("engine: target and readback made");
        e.load_shared(prog, inserted, timeline)?;
        mem_mark("engine: edit loaded into the World");
        for i in 0..8 {
            e.app.update();
            mem_mark(&format!("engine: warm-up update {i}"));
        }
        // the warm-up drew the clock's frame: a first draw across a cut from it warms up again (Engine::draw)
        e.drawn = Some(e.app.world().resource::<Clock>().0);
        // updates pipelined from here on (raea_sys_frame::pipe), but for a live draw's own
        e.pipe = raea_sys_frame::pipe::Pipe::new();
        // a live game's frame on one thread (lightspeed F2): its scene is small, and the multi-threaded executor's task
        // hand-offs cost more than its systems (arena3d's frame work p50 2.7-3.4 -> 1.9-2.6 ms, p99 3.9-4.6 -> 2.7-3.5,
        // paired). RAEA_BEVY_EXECUTOR=single|multi picks for every engine; renders stay multi-threaded otherwise
        let single = match std::env::var("RAEA_BEVY_EXECUTOR").as_deref() {
            Ok("single") => true,
            Ok("multi") => false,
            _ => live,
        };
        if single {
            use bevy::ecs::schedule::{SingleThreadedExecutor, Schedules};
            for (_, s) in e.app.world_mut().resource_mut::<Schedules>().iter_mut() {
                s.set_executor(SingleThreadedExecutor::new());
            }
            let rw = e.app.sub_app_mut(bevy::render::RenderApp).world_mut();
            for (_, s) in rw.resource_mut::<Schedules>().iter_mut() {
                s.set_executor(SingleThreadedExecutor::new());
            }
        }
        Ok(e)
    }
}

/// A camera that jumped (raea-sys-post cut.rs, RT31): Solari's history starts again, and with it MetalFX's denoiser's,
/// its scaler's and emit_taa's, which follow Solari's reset.
#[cfg(feature = "raytracing")]
fn solari_cut(mut cams: Query<(&raea_sys_post::cut::CameraCut, &mut bevy::solari::prelude::SolariLighting)>) {
    for (cut, mut s) in &mut cams {
        if cut.0 {
            s.reset = true;
        }
    }
}
