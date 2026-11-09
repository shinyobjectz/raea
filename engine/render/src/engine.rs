//! The runtime: one long-lived Bevy app whose World mirrors the edit's tables. 3D kinds render with Bevy; 2D kinds
//! are raea rows drawn by Blitz (real CSS) in the same World. `load` re-mirrors the tables after a move without
//! restarting the app; `frame` makes one frame: the inserted rows, then keys, then every rule (the timeline),
//! written onto the entities, then rendered. Offline rendering has no pipelining, and a readback is only taken when
//! its tag row carries the frame, so a frame is a function of the tables in any order.

use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};

use bevy::camera::RenderTarget;
use bevy::prelude::*;
#[cfg(feature = "native")]
use bevy::render::gpu_readback::{Readback, ReadbackComplete};
use bevy::render::render_resource::{TextureFormat, TextureUsages};
use bevy::window::ExitCondition;
#[cfg(feature = "native")]
use bevy::render::extract_resource::ExtractResourcePlugin;
use bevy::render::render_asset::RenderAssets;
#[cfg(feature = "native")]
use bevy::render::storage::ShaderBuffer;

use raea_core_program::eval::Tup;
use raea_sys_frame::store;
use raea_core_program::ir::{Program, V};

// clips pooled on the GPU, the Blitz overlay and the NV12 readback (raea-runtime-gpuframe)
#[cfg(feature = "native")]
use raea_runtime_gpuframe::{nv12::*, overlay::*, pool::*};
mod spawn;
pub use spawn::*;
pub use open::Opening;
// the 2D rows as the page reads them, and the paint device as Bevy's renderer (raea-runtime-view)
#[cfg(feature = "native")]
use raea_runtime_view::rows::*;
#[cfg(feature = "native")]
use raea_runtime_view::render_creation;

mod target;
pub(crate) use target::*;

mod open;
mod load;
mod output;
// RT1: each frame's motion vectors and depth, for a neural or denoising pass after the render
mod aov;
pub use aov::Aov;

// a frame's rows, the edit, the clock and the row helpers every pass reads (raea-sys-frame)
use raea_sys_frame::*;
use raea_sys_frame::bevy_api;
use raea_sys_environment as environment;
use raea_runtime_scene3d as scene3d;
use raea_sys_look::look;
use raea_sys_post::outline;
// composition passes: which pass drew each pixel, and the frame-wide effects held back from some
use raea_sys_compose::compose;
use raea_sys_fog_half as fog_half;
use raea_sys_fog::froxel;
use raea_sys_fog::froxel_glass;
use raea_sys_fog::fire;
// text in the 3D world, a glyph an entity (raea-sys-text3d)
use raea_sys_text3d as text3d;
// Gaussian splats (raea-sys-splat)
use raea_sys_splat as splat;
// 2D over the 3D, <Sprite> and <Shape2D> (raea-sys-sprite)
use raea_sys_sprite as sprite2d;
// traced ambient occlusion (raea-sys-rt)
use raea_sys_rt::traced_ao;
use raea_sys_grid as grid;
// footage behind the 3D and a camera's window on a larger view (<Matchmove>)
use raea_sys_matchmove as matchmove;
use raea_sys_look::paint;
use raea_sys_material::decal;
use raea_sys_compose::passes;
use raea_sys_screen as screen;
// the Engine's screens
#[cfg(feature = "native")]
mod screens;
use raea_sys_volume as volume;
// meshes that cast shadows without being drawn (<Mesh shadow="only">)
use raea_sys_material::shadow_only;
use raea_sys_material::shadow_catch;
// baked simulations played back by the frame (<MeshSequence>, <Goo>) and glitches on any part (physics.tsx)
use raea_sys_meshseq as meshseq;
// toon (cel) surfaces and their ink outlines (<MeshToonMaterial>)
use raea_sys_material::toon;
use raea_sys_shader as shader_ext;
// the camera's lens: Fisheye and LensDistortion, a pass after tone mapping
use raea_sys_post::lens;
// the camera's motion blur (<MotionBlur>), a function of the frame
use raea_sys_post::motion_blur;
// Bevy's depth of field, patched to keep the frame's alpha (the paint look's sky)
use raea_sys_post::dof;
use raea_sys_post::effect_stack;
// Bevy plugins compiled in by a crate that registers them, driven by generic rows and stepped by the frame index
use raea_runtime_plugins as plugins;
/// raea's modules as Bevy plugins, gathered in RaeaPlugins (docs/sdk.feature S6a)
pub mod raea_plugins;

pub struct Engine {
    app: App,
    target: Handle<Image>,
    /// the target's size: the edit's, times ss
    w: u32,
    h: u32,
    /// RAEA_SS: supersampling, the frame drawn at ss x its size and each ss x ss block averaged (in linear light)
    /// back down as it's read back. Lossless readback only (RAEA_READBACK=rgba).
    ss: u32,
    dir: PathBuf,
    /// a frame has come back since the last load (the GPU holds the load's assets)
    #[cfg(feature = "native")]
    warm: bool,
    /// RAEA_SERIAL: wait for the GPU after every update, to time its un-overlapped work
    #[cfg(feature = "native")]
    serial: bool,
    /// frames come back as NV12 (the default) or as RGBA (RAEA_READBACK=rgba, the reference path)
    pub nv12: bool,
    /// RGBA frames from the GPU composite (stills): w x h words then the tag word, no row padding
    pub packed: bool,
    /// with a Bevy plugin installed, the last load (a frame before the last stepped one replays it from frame 0 in a
    /// new App), and whether this Engine draws live
    replay: Option<(Program, Vec<Tup>)>,
    live: bool,
    /// the edit has MotionBlur: each update of frame f is preceded by one of frame f - 1 (its readback ignored), so the
    /// previous poses Bevy blurs by are always frame f - 1's, however many updates a frame takes (motion_blur.rs)
    prestep: bool,
    /// its updates pipelined or not (raea_sys_frame::pipe)
    pipe: raea_sys_frame::pipe::Pipe,
}

/// RAEA_MEM_MARKS: one line on stderr per start-up phase, "mem_mark <ms since the first mark> <phys_footprint MB>
/// <phase>", to line up with a memory timeline (the footprint is the process's, as the OS counts it).
pub fn mem_mark(what: &str) {
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    static T0: std::sync::OnceLock<bevy::platform::time::Instant> = std::sync::OnceLock::new();
    if !*ON.get_or_init(|| std::env::var_os("RAEA_MEM_MARKS").is_some()) {
        return;
    }
    let t = T0.get_or_init(bevy::platform::time::Instant::now).elapsed().as_secs_f64() * 1e3;
    let mb = phys_footprint().map(|b| format!("{:.1}", b as f64 / 1048576.0)).unwrap_or_else(|| "?".into());
    eprintln!("mem_mark {t:.1} {mb} {what}");
}

/// The edit has 3D rows (a light or a mesh): its camera draws through the 3D pipeline, and the Apple encoder runs
/// real-time for it.
pub fn has_3d(rows: &[Tup]) -> bool {
    rows.iter().any(|t| matches!(t.kind.as_str(), "light" | "cube" | "sphere" | "plane" | "panel" | "decal" | "mesh" | "model" | "fogvolume" | "fire" | "grass" | "plate" | "splat" | "meshseq" | "entity" | "text" | "glyph"))
}

/// RAEA_MEM_MARKS_EVERY: frames between render marks (default 100).
pub fn mark_every() -> usize {
    std::env::var("RAEA_MEM_MARKS_EVERY").ok().and_then(|v| v.parse().ok()).filter(|n: &usize| *n > 0).unwrap_or(100)
}

/// The process's physical footprint (task_info TASK_VM_INFO), on Apple platforms.
#[cfg(target_vendor = "apple")]
pub fn phys_footprint() -> Option<u64> {
    unsafe extern "C" {
        static mach_task_self_: u32;
        fn task_info(task: u32, flavor: u32, info: *mut i32, count: *mut u32) -> i32;
    }
    // task_vm_info_data_t: phys_footprint is the 64-bit field at byte 144
    let mut buf = [0u64; 64];
    let mut count: u32 = (std::mem::size_of_val(&buf) / 4) as u32;
    let r = unsafe { task_info(mach_task_self_, 22, buf.as_mut_ptr() as *mut i32, &mut count) };
    (r == 0 && count as usize * 4 >= 152).then(|| buf[144 / 8])
}

#[cfg(not(target_vendor = "apple"))]
pub fn phys_footprint() -> Option<u64> {
    None
}

/// Before a frame renders, every pipeline that's compiling is waited for (they compile in parallel on the task pool),
/// so the frame draws everything it has, as synchronous compilation did. A pipeline whose shader hasn't loaded yet is
/// retried by Bevy as before; only those being compiled are waited on.
fn wait_pipelines(mut cache: ResMut<bevy::render::render_resource::PipelineCache>) {
    use bevy::render::render_resource::CachedPipelineState;
    // a browser compiles them asynchronously, on its own event loop: a live frame there draws what's made
    #[cfg(target_arch = "wasm32")]
    {
        cache.process_queue();
        return;
    }
    #[cfg(not(target_arch = "wasm32"))]
    loop {
        cache.process_queue();
        if !cache.pipelines().any(|p| matches!(p.state, CachedPipelineState::Creating(_))) {
            return;
        }
        std::thread::sleep(std::time::Duration::from_micros(200));
    }
}

/// Without native there is no paint device to share: Bevy makes its own (in a browser, on the page's WebGPU).
#[cfg(not(feature = "native"))]
fn render_creation() -> bevy::render::settings::RenderCreation {
    default()
}

impl Engine {
    pub fn new(prog: &Program, inserted: &[Tup], dir: &Path) -> Result<Engine, String> {
        Engine::new_shared(prog, inserted, dir, None)
    }

    /// As new, with the edit's timeline already evaluated (shared with the sound or a preview).
    pub fn new_shared(prog: &Program, inserted: &[Tup], dir: &Path, timeline: Option<std::sync::Arc<raea_core_program::timeline::Timeline>>) -> Result<Engine, String> {
        Engine::build(prog, inserted, dir, timeline, false)
    }

    /// For live play: the program drawn on the paint device into its target and handed over as a texture (`draw`,
    /// `texture`), nothing read back. A live 3D scene draws through this, the same rows and code as its render.
    pub fn new_live(prog: &Program, inserted: &[Tup], dir: &Path) -> Result<Engine, String> {
        Engine::build(prog, inserted, dir, None, true)
    }

    /// Frame `f` drawn into the target (one update: the pipeline is synchronous, so it's on the GPU's queue when this
    /// returns, ahead of anything the paint device is given next).
    pub fn draw(&mut self, f: i64) {
        // with a Bevy plugin, a frame drawn again is left as it was: some plugins change state on every update, not
        // by time (hanabi re-seeds its particles each update), so a frame is exactly one update in every path
        if plugins::active(self.app.world()) && self.app.world().resource::<plugins::PluginClock>().stepped == f && self.app.world().resource::<plugins::PluginClock>().steps > 0 {
            return;
        }
        if let Err(e) = self.catch_up(f) {
            self.app.world_mut().resource_mut::<Error>().0 = Some(e);
        }
        self.app.world_mut().resource_mut::<Clock>().0 = f;
        self.pre_step();
        plugins::arm(self.app.world_mut());
        self.pipe.flush(&mut self.app);
        self.app.update();
        self.capture_aov(f);
    }

    /// With MotionBlur, the frame before the clock's drawn first (its readback ignored): Bevy takes each mesh's,
    /// skin's and the camera's previous pose from the update before, so this makes it frame f - 1's on every update of
    /// frame f, in a render (which may update a frame several times), a seek and play alike. It costs a second draw.
    fn pre_step(&mut self) {
        let f = self.app.world().resource::<Clock>().0;
        if !self.prestep || f <= 0 || plugins::active(self.app.world()) {
            return;
        }
        let world = self.app.world_mut();
        world.resource_mut::<Clock>().0 = f - 1;
        world.resource_mut::<PreStep>().0 = true;
        self.pipe.update(&mut self.app);
        let world = self.app.world_mut();
        world.resource_mut::<PreStep>().0 = false;
        world.resource_mut::<Clock>().0 = f;
        // its readback, tagged with generation 0, is left for `frame` and `frames` to pass over: other frames'
        // readbacks may have come back in the same update
    }

    /// A live scene (a game's 3D, made from its play's state each frame): `rows` are values for the rows they name, by
    /// kind and id, merged into each row's latest ones; a row the engine doesn't have yet is spawned as a load spawns it
    /// (its mesh built and its surface made from these values, so its first values are all of them). From the first
    /// call on, every frame draws the rows' latest values instead of the timeline's. Nothing is rebuilt per frame: only
    /// the values given change, and the sync systems write only what changed onto the entities.
    pub fn set_rows(&mut self, rows: &[Tup]) -> Result<(), String> {
        let spawned = apply_rows(self.app.world_mut(), &self.dir, rows)?;
        if spawned {
            scene3d::shade_rows(self.app.world_mut(), rows)?;
        }
        // a camera's frame looks, as the timeline's are each frame (relational), so a live game's look turns on too
        self.app.world_mut().resource_mut::<FrameRows>().parse_looks();
        // a glTF model, and a panorama a live frame showed first (#38), set up before the frame that shows it draws
        if spawned { scene3d::await_models(&mut self.app)?; }
        raea_sys_post::msaa_for_3d(self.app.world_mut(), has_3d(rows) || raea_sys_sprite::has_2d(rows));
        environment::live_rows(&mut self.app, rows, &self.dir)
    }

    /// A live scene's rows dropped, by kind and id: their entities go (and the meshes and surfaces only they held).
    pub fn remove_rows(&mut self, rows: &[(String, String)]) {
        remove_rows_from(self.app.world_mut(), rows);
    }

    /// The registered Bevy plugins' frame clock: the frames before f stepped through (one update each, their readbacks
    /// dropped), from a new App made from the load when f is before the last frame stepped. Nothing without plugins.
    fn catch_up(&mut self, f: i64) -> Result<(), String> {
        if !plugins::active(self.app.world()) {
            return Ok(());
        }
        if f < self.app.world().resource::<plugins::PluginClock>().stepped {
            // a new App, not a reload into this one: a plugin's state can live outside the rows' entities (hanabi's
            // particles in GPU buffers its render world keeps), and only a fresh App is sure to start from nothing
            let (prog, rows) = self.replay.clone().ok_or("a plugin's frame clock went back with no load to replay")?;
            let dir = self.dir.clone();
            // the old App's GPU work finished first, so a buffer map it asked for (Bevy's clustering readback maps a
            // staging buffer on submit) calls back while its buffer still exists, not on the new App's next poll after
            // the old App dropped it ("Buffer ... has been destroyed" from get_mapped_range, a panic)
            #[cfg(feature = "native")]
            raea_sys_frame::pipe::wait_gpu(&self.app);
            *self = Engine::build(&prog, &rows, &dir, None, self.live)?;
        }
        loop {
            let s = self.app.world().resource::<plugins::PluginClock>().stepped;
            if s + 1 >= f {
                break;
            }
            self.app.world_mut().resource_mut::<Clock>().0 = s + 1;
            plugins::arm(self.app.world_mut());
            self.pipe.update(&mut self.app);
            self.app.world_mut().resource_mut::<Out>().0.clear();
        }
        Ok(())
    }

    /// The gamepads Bevy sees (bevy_gilrs reads the real ones into Gamepad entities each update), as the live wire's
    /// marked keys a game's useInput() reads as `gamepads` (games.ts wireKeys): "Pad0:South" for each button held,
    /// "Pad0:LeftStickX=0.42" for each axis off its rest. Pads are numbered in the order Bevy made their entities.
    pub fn gamepad_keys(&mut self) -> Vec<String> {
        use bevy::input::gamepad::{Gamepad, GamepadInput};
        let world = self.app.world_mut();
        let Some(mut q) = world.try_query::<(Entity, &Gamepad)>() else { return Vec::new() };
        let mut pads: Vec<(Entity, &Gamepad)> = q.iter(world).collect();
        pads.sort_by_key(|(e, _)| *e);
        let mut out = Vec::new();
        for (i, (_, pad)) in pads.into_iter().enumerate() {
            for b in pad.get_pressed() {
                out.push(format!("Pad{i}:{b:?}"));
            }
            for input in pad.get_analog_axes() {
                if let GamepadInput::Axis(a) = input {
                    if let Some(v) = pad.get(*a).filter(|v| v.abs() > 1e-3) {
                        out.push(format!("Pad{i}:{a:?}={}", (v * 1e4).round() / 1e4));
                    }
                }
            }
        }
        out
    }

    /// The App's World (for tools that read its TypeRegistry: bevy-plugins' generator).
    pub fn world(&self) -> &World {
        self.app.world()
    }

    /// The App's World, to change (raea-web: its canvas window and the camera that shows the target on it).
    pub fn world_mut(&mut self) -> &mut World {
        self.app.world_mut()
    }

    /// The image every frame is drawn into (w x h, and the tag row under it).
    pub fn target_image(&self) -> Handle<Image> {
        self.target.clone()
    }

    /// What went wrong drawing a live frame (a model that failed to load, a row that didn't spawn), taken: a live
    /// engine's `draw` keeps going without it, so its caller says it.
    pub fn take_error(&mut self) -> Option<String> {
        self.app.world_mut().resource_mut::<Error>().0.take()
    }

    /// The target as a texture view on the paint device (its stored bytes, w x h and one tag row under them).
    pub fn texture(&self) -> Option<wgpu::TextureView> {
        let rw = self.app.get_sub_app(bevy::render::RenderApp)?.world();
        let images = rw.get_resource::<RenderAssets<bevy::render::texture::GpuImage>>()?;
        let gi = images.get(&self.target)?;
        let t: &wgpu::Texture = &gi.texture;
        Some(t.create_view(&wgpu::TextureViewDescriptor { format: Some(wgpu::TextureFormat::Rgba8Unorm), ..Default::default() }))
    }

    /// The target's texture on the paint device (`Rgba8UnormSrgb`, premultiplied: w x h, and one tag row under them):
    /// what live play lays its bevy_ui page over or puts in its layered slot, copied on the GPU.
    pub fn target_texture(&self) -> Option<wgpu::Texture> {
        let rw = self.app.get_sub_app(bevy::render::RenderApp)?.world();
        let images = rw.get_resource::<RenderAssets<bevy::render::texture::GpuImage>>()?;
        images.get(&self.target).map(|gi| (*gi.texture).clone())
    }

    /// A live engine's target after `draw`, read back: its stored bytes, w x h (the tag row left out). What the bevy_ui
    /// page lays its page over for a still (raea), where the fed render samples `texture` on the device instead.
    #[cfg(feature = "native")]
    /// Whether the 3D's GPU still runs work (raea-page's gpucheck `canary`).
    pub fn canary(&self) -> Result<(), String> {
        let rw = self.app.get_sub_app(bevy::render::RenderApp).ok_or("no render app")?.world();
        let device = rw.resource::<bevy::render::renderer::RenderDevice>().wgpu_device();
        raea_gpu_device::check::canary(device, rw.resource::<bevy::render::renderer::RenderQueue>())
    }

    pub fn read_target(&self) -> Result<Vec<u8>, String> {
        let rw = self.app.get_sub_app(bevy::render::RenderApp).ok_or("no render app")?.world();
        let gi = rw.resource::<RenderAssets<bevy::render::texture::GpuImage>>().get(&self.target).ok_or("the 3D drew no target")?;
        let device = rw.resource::<bevy::render::renderer::RenderDevice>().wgpu_device().clone();
        let queue = rw.resource::<bevy::render::renderer::RenderQueue>();
        let (w, h) = (self.w, self.h);
        let row = (w * 4).div_ceil(256) * 256;
        let buf = device.create_buffer(&wgpu::BufferDescriptor { label: Some("3D target readback"), size: (row * h) as u64, usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ, mapped_at_creation: false });
        let mut enc = device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("3D target readback") });
        enc.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo { texture: &gi.texture, mip_level: 0, origin: wgpu::Origin3d::ZERO, aspect: wgpu::TextureAspect::All },
            wgpu::TexelCopyBufferInfo { buffer: &buf, layout: wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(row), rows_per_image: Some(h) } },
            wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
        );
        queue.submit([enc.finish()]);
        let (tx, rx) = std::sync::mpsc::channel();
        buf.slice(..).map_async(wgpu::MapMode::Read, move |r| {
            let _ = tx.send(r);
        });
        device.poll(wgpu::PollType::Wait { submission_index: None, timeout: Some(std::time::Duration::from_secs(60)) }).map_err(|e| format!("the 3D's readback: {e}"))?;
        rx.recv().map_err(|e| e.to_string())?.map_err(|e| format!("the 3D's readback: {e}"))?;
        let data = buf.slice(..).get_mapped_range().map_err(|e| format!("the 3D's readback: {e:?}"))?;
        let mut out = Vec::with_capacity((w * h * 4) as usize);
        for y in 0..h as usize {
            out.extend_from_slice(&data[y * row as usize..y * row as usize + w as usize * 4]);
        }
        drop(data);
        buf.unmap();
        Ok(out)
    }

    /// Overlay redraws and reuses so far.
    #[cfg(feature = "native")]
    pub fn overlay_stats(&self) -> (u64, u64) {
        let k = self.app.world().resource::<OverlayKey>();
        (k.drawn, k.reused)
    }

    pub fn size(&self) -> (u32, u32) {
        (self.w / self.ss, self.h / self.ss)
    }

    /// The camera's view drawn into `b` ([x, y, w, h] in frame px) of the frame instead of the whole of it: a camera
    /// inside a page element (docs/sdk.feature S3a), its own view at the box's aspect, the rest of the frame whatever
    /// the wider frustum sees (the page crops it). None: the whole frame, as always.
    pub fn set_view_box(&mut self, b: Option<[u32; 4]>) {
        let ss = self.ss;
        let b = b.map(|[x, y, w, h]| [x * ss, y * ss, w.max(1) * ss, h.max(1) * ss]);
        if let Some(mut v) = self.app.world_mut().get_resource_mut::<ViewBox>() {
            if v.0 != b {
                v.0 = b;
            }
        }
    }

}
