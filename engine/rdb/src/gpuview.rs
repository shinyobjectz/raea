//! GPU preview (G8): the edit's frames from the GPU pool, without the Bevy frame loop. A frame is its rows (the
//! rules, on the CPU, cached), its clips as NV12 planes in the pool (decoded and uploaded once), its Blitz overlay
//! (rendered once per distinct overlay state, kept on the GPU by digest), and one composite.k dispatch into a resident
//! frame buffer. Scrubbing back to a visited frame is a single dispatch. Edits with 3D content preview through the
//! engine instead (the background here is the camera's clear colour).

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::Instant;

use bevy::ecs::entity::Entity;

use crate::eval::Tup;
use crate::ir::{Program, V};

type PoolKey = (String, i64);

fn num(v: &std::collections::BTreeMap<String, V>, k: &str, d: f64) -> f64 {
    v.get(k).map(V::num).unwrap_or(d)
}

/// Timings of one preview frame (ms).
#[derive(Debug, Clone, Default)]
pub struct FrameTimes {
    pub rules: f64,
    pub overlay: f64,
    pub decode_upload: f64,
    pub composite_gpu: f64,
    pub total: f64,
    pub uploads: usize,
    pub overlay_cached: bool,
}

/// A frame in the pool: its NV12 planes copied into a buffer, or (zero-copy) the decoder's own buffer as two textures.
enum Pooled {
    Buf(wgpu::Buffer),
    #[cfg(target_vendor = "apple")]
    Surf(zc::Planes),
}

impl Pooled {
    fn bytes(&self) -> usize {
        match self {
            Pooled::Buf(b) => b.size() as usize,
            #[cfg(target_vendor = "apple")]
            Pooled::Surf(p) => p.bytes,
        }
    }
}

/// Zero-copy on Apple: a decoded CVPixelBuffer (NV12, IOSurface-backed) becomes two wgpu textures, its luma plane as
/// R8 and its chroma plane as RG8, through CoreVideo's Metal texture cache. Nothing is copied: the GPU reads the
/// decoder's memory.
#[cfg(target_vendor = "apple")]
mod zc {
    use std::ptr::NonNull;

    use objc2_core_foundation::CFRetained;
    use objc2_core_video::{CVMetalTexture, CVMetalTextureCache, CVMetalTextureGetTexture, CVPixelBuffer};
    use objc2_metal::{MTLPixelFormat, MTLTextureType};

    pub struct Planes {
        pub y: wgpu::TextureView,
        pub uv: wgpu::TextureView,
        pub bytes: usize,
        _textures: (wgpu::Texture, wgpu::Texture),
        _cv: (CFRetained<CVMetalTexture>, CFRetained<CVMetalTexture>),
        _buffer: CFRetained<CVPixelBuffer>,
    }

    pub struct Cache(CFRetained<CVMetalTextureCache>);

    impl Cache {
        /// Let the cache drop textures nobody holds (CVMetalTextureCacheFlush): what the pool evicted goes now.
        pub fn flush(&self) {
            self.0.flush(0);
        }

        pub fn new(device: &wgpu::Device) -> Result<Cache, String> {
            let hal = unsafe { device.as_hal::<wgpu::hal::api::Metal>() }.ok_or("zero-copy: the device isn't Metal")?;
            let mtl = hal.raw_device().clone();
            let mut c: *mut CVMetalTextureCache = std::ptr::null_mut();
            let r = unsafe { CVMetalTextureCache::create(None, None, &mtl, None, NonNull::new(&mut c).unwrap()) };
            if r != 0 || c.is_null() {
                return Err(format!("CVMetalTextureCacheCreate: {r}"));
            }
            Ok(Cache(unsafe { CFRetained::from_raw(NonNull::new(c).unwrap()) }))
        }

        #[allow(clippy::too_many_arguments)]
        fn plane(&self, device: &wgpu::Device, pb: &CVPixelBuffer, plane: usize, w: u32, h: u32, mtl: MTLPixelFormat, format: wgpu::TextureFormat) -> Result<(wgpu::Texture, wgpu::TextureView, CFRetained<CVMetalTexture>), String> {
            let mut t: *mut CVMetalTexture = std::ptr::null_mut();
            let r = unsafe { CVMetalTextureCache::create_texture_from_image(None, &self.0, pb, None, mtl, w as usize, h as usize, plane, NonNull::new(&mut t).unwrap()) };
            if r != 0 || t.is_null() {
                return Err(format!("CVMetalTextureCacheCreateTextureFromImage (plane {plane}): {r}"));
            }
            let cv = unsafe { CFRetained::from_raw(NonNull::new(t).unwrap()) };
            let raw = CVMetalTextureGetTexture(&cv).ok_or("CVMetalTextureGetTexture: none")?;
            let extent = wgpu::hal::CopyExtent { width: w, height: h, depth: 1 };
            let hal_tex = unsafe { wgpu::hal::metal::Device::texture_from_raw(raw, format, MTLTextureType::Type2D, 1, 1, extent) };
            let desc = wgpu::TextureDescriptor {
                label: Some("zero-copy plane"),
                size: wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format,
                usage: wgpu::TextureUsages::TEXTURE_BINDING,
                view_formats: &[],
            };
            let tex = unsafe { device.create_texture_from_hal::<wgpu::hal::api::Metal>(hal_tex, &desc) };
            let view = tex.create_view(&Default::default());
            Ok((tex, view, cv))
        }

        pub fn wrap(&self, device: &wgpu::Device, buffer: CFRetained<CVPixelBuffer>, w: u32, h: u32) -> Result<Planes, String> {
            let (ty, yv, ycv) = self.plane(device, &buffer, 0, w, h, MTLPixelFormat::R8Unorm, wgpu::TextureFormat::R8Unorm)?;
            let (tu, uv, ucv) = self.plane(device, &buffer, 1, w / 2, h / 2, MTLPixelFormat::RG8Unorm, wgpu::TextureFormat::Rg8Unorm)?;
            Ok(Planes { y: yv, uv, bytes: (w * h * 3 / 2) as usize, _textures: (ty, tu), _cv: (ycv, ucv), _buffer: buffer })
        }
    }
}

pub struct GpuView {
    prog: Program,
    rows: Vec<Tup>,
    dir: PathBuf,
    pub gpu: raea_kgpu::Gpu,
    backend: Box<dyn raea_media::Backend>,
    decoders: HashMap<(String, usize), Box<dyn raea_media::VideoDecoder>>,
    pool: HashMap<PoolKey, Pooled>,
    pub pool_bytes: usize,
    states: HashMap<i64, Vec<Tup>>,
    /// every frame of the program, evaluated once (built on first use)
    tl: Option<std::sync::Arc<crate::timeline::Timeline>>,
    rw: raea::world::RowWorld,
    ents: HashMap<String, Entity>,
    overlays: HashMap<u64, wgpu::Buffer>,
    /// the next frame's overlay, painting on the GPU while this frame composites: its frame, digest and paint
    ahead: Option<(i64, u64, raea::row_render::Painting)>,
    /// an HTML composition's own paint (html.rs): the overlay instead of the 2D rows
    /// an HTML page in Bevy's schedule (raea::stage): its frames are runs of the schedule
    page: Option<raea::stage::Stage>,
    /// the page the caller handed for the next frame, already on this device (render_nv12_fed): the overlay as it is
    fed: Option<wgpu::Buffer>,
    /// a fed render is running (render_nv12_fed): the process-wide text switches stay as the caller set them
    fed_render: bool,
    /// the page painted ahead on workers (html::PagePool), and the pixels it handed over for the frame being made
    page_pool: Option<crate::html::PagePool>,
    pool_px: Option<Vec<u8>>,
    comp: raea_kgpu::Compiled,
    _bg: wgpu::Texture,
    bg_view: wgpu::TextureView,
    pub out: wgpu::Buffer,
    dummy: wgpu::Buffer,
    /// kgpu's mask M when no clip is masked: zeros, w x h words (composite.k reads it for every pixel)
    no_mask: wgpu::Buffer,
    /// the direct render's nv12buf.k and its output, made on first use
    nv12: Option<(raea_kgpu::Compiled, wgpu::Buffer)>,
    /// the direct render hands P010 (p010buf.k: 10-bit BT.709 limited, w x h x 3 bytes) instead of NV12, for a
    /// 10-bit encoder (RenderOpts.ten_bit)
    pub ten_bit: bool,
    /// RAEA_GPU_TIMES: timestamps around the direct render's GPU stages (8 per frame)
    ts: Option<wgpu::QuerySet>,
    /// uploads timed this frame (query pairs from 8)
    ts_uploads: u32,
    /// CPU ms spent handing uploads to the queue (the staging copy), summed
    pub upload_cpu_ms: f64,
    /// RAEA_ZEROCOPY (Apple): decoded frames stay in VideoToolbox's buffers, wrapped as textures; composite_tex.k
    #[cfg(target_vendor = "apple")]
    zc: Option<(zc::Cache, raea_kgpu::Compiled, wgpu::TextureView, wgpu::TextureView, (wgpu::Texture, wgpu::Texture))>,
    pub w: u32,
    pub h: u32,
    // the sketch: proxies packed for one batched dispatch
    sketch_k: raea_kgpu::Compiled,
    pp: Vec<u8>,
    pp_index: HashMap<PoolKey, (usize, u32, u32)>,
    oo: Vec<u32>,
    oo_index: HashMap<u64, usize>,
    pub pw: u32,
    pub ph: u32,
    bgc: u32,
    /// the packed proxies on the GPU, and how many bytes / words of them were uploaded
    pp_gpu: Option<(wgpu::Buffer, usize)>,
    oo_gpu: Option<(wgpu::Buffer, usize)>,
    /// compiled kernels by name (and shape)
    kernels: HashMap<String, std::rc::Rc<raea_kgpu::Compiled>>,
    /// when each pooled frame was last used, for the budget
    pool_used: HashMap<PoolKey, u64>,
    tick: u64,
}

/// Timings of a whole-edit sketch (ms).
#[derive(Debug, Clone, Default)]
pub struct SketchTimes {
    pub frames: usize,
    pub states: f64,
    pub ingest: f64,
    pub params: f64,
    pub upload: f64,
    pub gpu: f64,
    pub new_proxies: usize,
    pub new_overlays: usize,
}

impl SketchTimes {
    pub fn json(&self) -> serde_json::Value {
        serde_json::json!({"frames": self.frames, "states_ms": self.states, "ingest_ms": self.ingest, "params_ms": self.params,
            "upload_ms": self.upload, "sketch_gpu_ms": self.gpu, "new_proxies": self.new_proxies, "new_overlays": self.new_overlays})
    }
}

/// NV12 shrunk by box filter to (dw, dh) (both even).
fn shrink_nv12(src: &[u8], sw: usize, sh: usize, dw: usize, dh: usize) -> Vec<u8> {
    let mut out = vec![0u8; dw * dh * 3 / 2];
    let plane = |out: &mut [u8], base_s: usize, base_d: usize, w: usize, h: usize, ow: usize, oh: usize, step: usize, comp: usize| {
        for y in 0..oh {
            let (y0, y1) = (y * h / oh, ((y + 1) * h / oh).max(y * h / oh + 1));
            for x in 0..ow {
                let (x0, x1) = (x * w / ow, ((x + 1) * w / ow).max(x * w / ow + 1));
                let mut acc = 0u32;
                for yy in y0..y1 {
                    for xx in x0..x1 {
                        acc += src[base_s + yy * w * step + xx * step + comp] as u32;
                    }
                }
                out[base_d + y * ow * step + x * step + comp] = (acc / ((y1 - y0) * (x1 - x0)) as u32) as u8;
            }
        }
    };
    plane(&mut out, 0, 0, sw, sh, dw, dh, 1, 0);
    for comp in 0..2 {
        plane(&mut out, sw * sh, dw * dh, sw / 2, sh / 2, dw / 2, dh / 2, 2, comp);
    }
    out
}

/// Premultiplied RGBA shrunk by box filter to (dw, dh), as words.
fn shrink_rgba(src: &[u8], sw: usize, sh: usize, dw: usize, dh: usize) -> Vec<u32> {
    (0..dw * dh)
        .map(|i| {
            let (x, y) = (i % dw, i / dw);
            let (x0, x1, y0, y1) = (x * sw / dw, (x + 1) * sw / dw, y * sh / dh, (y + 1) * sh / dh);
            let mut acc = [0u32; 4];
            for yy in y0..y1 {
                for xx in x0..x1 {
                    for c in 0..4 {
                        acc[c] += src[(yy * sw + xx) * 4 + c] as u32;
                    }
                }
            }
            let n = ((y1 - y0) * (x1 - x0)).max(1) as u32;
            (acc[0] / n) | (acc[1] / n) << 8 | (acc[2] / n) << 16 | (acc[3] / n) << 24
        })
        .collect()
}

const SKETCH_K: &str = include_str!("../../kgpu/kernels/sketch.k");
const LOOK_GRID_K: &str = include_str!("../../kgpu/kernels/look_grid.k");
const LOOK_PROJ_K: &str = include_str!("../../kgpu/kernels/look_proj.k");
const LOOK_HAM_K: &str = include_str!("../../kgpu/kernels/look_ham.k");
const LOOK_PAIR_K: &str = include_str!("../../kgpu/kernels/look_pair.k");

const COMPOSITE_K: &str = include_str!("../../kgpu/kernels/composite.k");
const NV12BUF_K: &str = include_str!("../../kgpu/kernels/nv12buf.k");
const P010BUF_K: &str = include_str!("../../kgpu/kernels/p010buf.k");
#[cfg(target_vendor = "apple")]
const COMPOSITE_TEX_K: &str = include_str!("../../kgpu/kernels/composite_tex.k");

/// The overlay key of a fed page (no 2D state's digest is all ones in practice; a page's key is rotated).
const FED: u64 = u64::MAX;

/// RAEA_FRAME_LOG (raea-motion's fed render): each frame's wait for the GPU (ms), kept when the caller puts a Vec here
pub static FRAME_WAITS: std::sync::Mutex<Option<Vec<(i64, f64)>>> = std::sync::Mutex::new(None);

/// The paint device (raea::quads) as a kernel device, so the direct render composites pages on the device that
/// painted them (One renderer: one device); None where the GPU paint is off (RAEA_PAINT=cpu) or RAEA_SHARED_DEVICE=0.
pub fn paint_device() -> Option<raea_kgpu::Gpu> {
    if !raea::quads::enabled() || std::env::var("RAEA_SHARED_DEVICE").as_deref() == Ok("0") {
        return None;
    }
    let (_, adapter, device, queue) = raea::quads::handles().ok()?;
    let info = adapter.get_info();
    Some(raea_kgpu::Gpu { device, queue, adapter: format!("{} ({:?})", info.name, info.backend) })
}

impl GpuView {
    pub fn new(prog: &Program, rows: &[Tup], dir: &Path) -> Result<GpuView, String> {
        let (w, h) = (prog.width, prog.height);
        let gpu = paint_device().map_or_else(raea_kgpu::Gpu::new, Ok)?;
        // the 2D rows, as the engine installs them
        let rows2d: Vec<serde_json::Value> = rows.iter().filter(|t| crate::engine::is_2d(&t.kind)).map(|t| crate::engine::row2d(t, rows).1).collect();
        let edit = serde_json::json!({"fps": format!("{}", prog.fps.round() as u32), "width": w, "height": h, "frames": prog.frames.max(1), "instances": rows2d});
        let mut rw = raea::world::RowWorld::new_hosted(&edit, dir, 1.0)?;
        rw.world.resource_mut::<raea::world::Frame>().transparent = true;
        let mut ents = HashMap::new();
        let mut q = rw.world.query::<(Entity, &raea::world::Name, &raea::world::Resolved)>();
        let bases: Vec<(Entity, crate::engine::Base2d)> = q.iter(&rw.world).map(|(e, n, r)| {
            ents.insert(n.0.clone(), e);
            (e, crate::engine::Base2d(r.0.clone()))
        }).collect();
        for (e, b) in bases {
            rw.world.entity_mut(e).insert(b);
        }
        // the background: the camera's clear colour (the engine's camera clears to it)
        let bg = rows.iter().find(|t| t.kind == "camera").and_then(|t| match t.vals.get("bg") {
            Some(V::S(s)) => Some(s.clone()),
            _ => None,
        });
        let rgb = bg
            .and_then(|s| {
                let h = s.trim_start_matches('#').to_string();
                let b = |i: usize| u8::from_str_radix(h.get(i..i + 2)?, 16).ok();
                Some([b(0)?, b(2)?, b(4)?])
            })
            .unwrap_or([13, 10, 26]);
        let texels: Vec<u8> = (0..(w * (h + 1)) as usize).flat_map(|_| [rgb[0], rgb[1], rgb[2], 255]).collect();
        let tex = gpu.device.create_texture(&wgpu::TextureDescriptor {
            label: None,
            size: wgpu::Extent3d { width: w, height: h + 1, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[wgpu::TextureFormat::Rgba8Unorm],
        });
        gpu.queue.write_texture(
            wgpu::TexelCopyTextureInfo { texture: &tex, mip_level: 0, origin: wgpu::Origin3d::ZERO, aspect: wgpu::TextureAspect::All },
            &texels,
            wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(w * 4), rows_per_image: None },
            wgpu::Extent3d { width: w, height: h + 1, depth_or_array_layers: 1 },
        );
        let bg_view = tex.create_view(&wgpu::TextureViewDescriptor { format: Some(wgpu::TextureFormat::Rgba8Unorm), ..Default::default() });
        let k = raea_kgpu::parse(COMPOSITE_K)?;
        let sc: HashMap<&str, i32> = [("w", w as i32), ("h", h as i32), ("T_w", w as i32)].into();
        let comp = gpu.compile_tex(&k, &["P0", "P1", "P2", "P3", "O", "C", "M"], &["T"], &sc, &["px"])?;
        let no_mask = gpu.buffer((w * h * 4) as usize, Some(&vec![0u8; (w * h * 4) as usize]));
        let out = gpu.buffer((w * h * 4) as usize, None);
        let dummy = gpu.buffer(24, Some(&[0u8; 24]));
        let backend = raea_media::backend(&crate::engine::media_backend())?;
        let (pw, ph) = (144u32, (144.0 * h as f64 / w as f64).round() as u32 / 2 * 2);
        let sk = raea_kgpu::parse(SKETCH_K)?;
        let bgc = rgb[0] as u32 | (rgb[1] as u32) << 8 | (rgb[2] as u32) << 16 | 255 << 24;
        let ssc: HashMap<&str, i32> = [("pw", pw as i32), ("ph", ph as i32), ("bgc", bgc as i32)].into();
        let sketch_k = gpu.compile_tex(&sk, &["PP", "OO", "Q"], &[], &ssc, &["px"])?;
        let mut v = GpuView {
            prog: prog.clone(),
            rows: rows.to_vec(),
            dir: dir.to_path_buf(),
            gpu,
            backend,
            decoders: HashMap::new(),
            pool: HashMap::new(),
            pool_bytes: 0,
            states: HashMap::new(),
            tl: None,
            rw,
            ents,
            overlays: HashMap::new(),
            ahead: None,
            page: None,
            fed: None,
            fed_render: false,
            page_pool: None,
            pool_px: None,
            comp,
            _bg: tex,
            bg_view,
            out,
            dummy,
            no_mask,
            nv12: None,
            ten_bit: false,
            ts: None,
            ts_uploads: 0,
            upload_cpu_ms: 0.0,
            #[cfg(target_vendor = "apple")]
            zc: None,
            w,
            h,
            sketch_k,
            pp: Vec::new(),
            pp_index: HashMap::new(),
            oo: Vec::new(),
            oo_index: HashMap::new(),
            pw,
            ph,
            bgc,
            pp_gpu: None,
            oo_gpu: None,
            kernels: HashMap::new(),
            pool_used: HashMap::new(),
            tick: 0,
        };
        // RAEA_ZEROCOPY=1 (Apple): frames stay in VideoToolbox's buffers, read as textures by composite_tex.k
        #[cfg(target_vendor = "apple")]
        if std::env::var_os("RAEA_ZEROCOPY").is_some() {
            let cache = zc::Cache::new(&v.gpu.device)?;
            let k = raea_kgpu::parse(COMPOSITE_TEX_K)?;
            let sc: HashMap<&str, i32> = [("w", v.w as i32), ("h", v.h as i32), ("T_w", v.w as i32)].into();
            let ck = v.gpu.compile_tex(&k, &["O", "C", "M"], &["T", "Y0", "Y1", "Y2", "Y3", "U0", "U1", "U2", "U3"], &sc, &["px"])?;
            let one = |format: wgpu::TextureFormat| {
                let t = v.gpu.device.create_texture(&wgpu::TextureDescriptor {
                    label: None,
                    size: wgpu::Extent3d { width: 1, height: 1, depth_or_array_layers: 1 },
                    mip_level_count: 1,
                    sample_count: 1,
                    dimension: wgpu::TextureDimension::D2,
                    format,
                    usage: wgpu::TextureUsages::TEXTURE_BINDING,
                    view_formats: &[],
                });
                let view = t.create_view(&Default::default());
                (t, view)
            };
            let (ty, dy) = one(wgpu::TextureFormat::R8Unorm);
            let (tuv, duv) = one(wgpu::TextureFormat::Rg8Unorm);
            v.zc = Some((cache, ck, dy, duv, (ty, tuv)));
        }
        Ok(v)
    }

    /// The shared timeline of the current program (built once per program).
    fn timeline(&mut self) -> Result<&crate::timeline::Timeline, String> {
        if self.tl.is_none() {
            self.tl = Some(std::sync::Arc::new(crate::timeline::Timeline::new(&self.prog, &self.rows)?));
            self.mark_moving();
        }
        Ok(self.tl.as_ref().unwrap())
    }

    /// Use a timeline already evaluated for this program (shared with the sound).
    pub fn share_timeline(&mut self, tl: std::sync::Arc<crate::timeline::Timeline>) {
        self.tl = Some(tl);
        self.states.clear();
        self.mark_moving();
    }

    /// 2D rows whose x or y move over the edit paint their text smoothly (raea::world::mark_moving), as the engine's.
    fn mark_moving(&mut self) {
        let Some(tl) = self.tl.clone() else { return };
        // not for fed pages: the caller painted them as play paints them (a switch made between two pages moved their
        // text off the whole-pixel snap: 558 of ts1's 595 frames differed from the same pages through files)
        if !self.fed_render {
            raea::smooth_moving_text(true);
        }
        let moving: Vec<String> = self.rows.iter().enumerate()
            .filter(|(r, t)| crate::engine::is_2d(&t.kind) && tl.moves(*r, &["x", "y"]))
            .map(|(_, t)| format!("{}:{}", t.kind, t.id))
            .collect();
        if moving.is_empty() {
            return;
        }
        let mut q = self.rw.world.query::<(Entity, &raea::world::Name)>();
        let ents: Vec<Entity> = q.iter(&self.rw.world).filter(|(_, n)| moving.contains(&n.0)).map(|(e, _)| e).collect();
        for e in ents {
            raea::world::mark_moving(&mut self.rw.world, e);
        }
    }

    pub fn state(&mut self, f: i64) -> Result<&Vec<Tup>, String> {
        if !self.states.contains_key(&f) {
            let st = self.timeline()?.tups(f);
            self.states.insert(f, st);
        }
        Ok(&self.states[&f])
    }

    /// The overlay's digest at frame f: every 2D row's values and whether it shows (as the engine's overlay memo).
    /// The page's paint as the overlay (html.rs).
    pub fn set_page(&mut self, page: raea::scene::Composition) {
        self.page = Some(raea::stage::Stage::new(page));
    }

    /// Paint the page ahead on workers for these frames, in this order (html::PagePool).
    pub fn set_page_pool(&mut self, pool: crate::html::PagePool) {
        self.page_pool = Some(pool);
    }

    /// What the overlay shows at frame f: the page's state when there's a page, else the 2D rows'.
    fn overlay_key(&mut self, st: &[Tup], f: i64) -> u64 {
        let fps = self.prog.fps.max(1.0);
        if let Some((key, px)) = self.page_pool.as_mut().and_then(|p| p.take(f)) {
            self.pool_px = px;
            return key;
        }
        match self.page.as_mut() {
            Some(p) => p.comp().page_key(f as f64 / fps).rotate_left(1),
            None => Self::overlay_digest(st, f),
        }
    }

    pub(crate) fn overlay_digest(st: &[Tup], f: i64) -> u64 {
        use std::hash::{Hash, Hasher};
        let mut rows: Vec<&Tup> = st.iter().filter(|t| crate::engine::is_2d(&t.kind)).collect();
        rows.sort_by(|a, b| (&a.kind, &a.id).cmp(&(&b.kind, &b.id)));
        let mut h = std::collections::hash_map::DefaultHasher::new();
        for t in rows {
            let v = &t.vals;
            let on = f >= num(v, "from", f64::MIN) as i64 && v.get("to").map(|x| f < x.num() as i64).unwrap_or(true);
            (&t.kind, &t.id, on).hash(&mut h);
            if on {
                format!("{v:?}").hash(&mut h);
            }
        }
        h.finish()
    }

    /// Frame f into `out` (stays on the GPU). Returns where the time went.
    /// Data up to a new GPU buffer. Timed (RAEA_GPU_TIMES), it goes through a mapped staging buffer and an explicit
    /// copy between two timestamps, which is what the queue's write does underneath.
    fn upload(&mut self, data: &[u8]) -> wgpu::Buffer {
        let Some(q) = self.ts.as_ref().filter(|_| self.ts_uploads < 4) else {
            let t = Instant::now();
            let b = self.gpu.buffer(data.len(), Some(data));
            self.upload_cpu_ms += t.elapsed().as_secs_f64() * 1e3;
            return b;
        };
        let size = (data.len().max(4) as u64).div_ceil(4) * 4;
        let b = self.gpu.buffer(size as usize, None);
        let staging = self.gpu.device.create_buffer(&wgpu::BufferDescriptor { label: None, size, usage: wgpu::BufferUsages::MAP_WRITE | wgpu::BufferUsages::COPY_SRC, mapped_at_creation: true });
        staging.slice(..).get_mapped_range_mut().slice(..data.len()).copy_from_slice(data);
        staging.unmap();
        let i = 8 + 2 * self.ts_uploads;
        self.ts_uploads += 1;
        let mut enc = self.gpu.device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("rdb frame") });
        enc.write_timestamp(q, i);
        enc.copy_buffer_to_buffer(&staging, 0, &b, 0, size);
        enc.write_timestamp(q, i + 1);
        self.gpu.queue.submit([enc.finish()]);
        raea_gputrace::submitted(&self.gpu.queue, "rdb", "rdb frame");
        b
    }

    pub fn frame(&mut self, f: i64) -> Result<FrameTimes, String> {
        self.frame_wait(f, true)
    }

    /// As `frame`; without `wait` the composite is only submitted (the direct render keeps several frames in flight).
    fn frame_wait(&mut self, f: i64, wait: bool) -> Result<FrameTimes, String> {
        let t0 = Instant::now();
        self.tick += 1;
        let tick = self.tick;
        let mut ft = FrameTimes::default();
        let st = self.state(f)?.clone();
        ft.rules = t0.elapsed().as_secs_f64() * 1e3;

        // the overlay, rendered once per distinct state
        let t = Instant::now();
        // a page handed over on the GPU (render_nv12_fed) is this frame's overlay, under a key no state hashes to
        let fed = self.fed.take();
        let was_fed = fed.is_some();
        let dg = if was_fed { FED } else { self.overlay_key(&st, f) };
        ft.overlay_cached = fed.is_none() && self.overlays.contains_key(&dg);
        if let Some(b) = fed {
            self.overlays.insert(FED, b);
        } else if !ft.overlay_cached {
            let px = match self.ahead.take() {
                Some((af, adg, p)) if af == f && adg == dg => p.finish()?,
                Some((_, _, p)) => {
                    // painted for a frame that didn't come next: collected (it holds a GPU slot) and dropped
                    let _ = p.finish();
                    self.render_overlay(&st, f)
                }
                None => self.render_overlay(&st, f),
            };
            let b = self.upload(&px);
            // a page that changes every frame would keep a full-frame buffer per frame: keep the last few states
            if self.overlays.len() >= 32 {
                self.overlays.clear();
            }
            self.overlays.insert(dg, b);
        }
        // the next frame's row overlay painted on the GPU while this frame's clips decode and composite (a frame
        // waiting on its own paint cost ~5 ms of GPU latency each)
        // (not for a fed page: the caller paints the next one, and a row paint between would share its compositor)
        if !was_fed && self.page.is_none() && self.page_pool.is_none() && self.ahead.is_none() && f + 1 < self.prog.frames {
            let st1 = self.state(f + 1)?.clone();
            let dg1 = Self::overlay_digest(&st1, f + 1);
            if dg1 != dg && !self.overlays.contains_key(&dg1) && self.page_image_rows(&st1, f + 1).is_none() {
                let p = self.submit_overlay(&st1, f + 1);
                self.ahead = Some((f + 1, dg1, p));
            }
        }
        ft.overlay = t.elapsed().as_secs_f64() * 1e3;

        // the clips: decode, upload once, slot
        let t = Instant::now();
        let (fps, fw, fh) = (self.prog.fps, self.w as f64, self.h as f64);
        let mut clips: Vec<&Tup> = st.iter().filter(|t| t.kind == "clip").collect();
        clips.sort_by(|a, b| num(&a.vals, "z", 0.0).total_cmp(&num(&b.vals, "z", 0.0)).then(a.id.cmp(&b.id)));
        let mut slots: Vec<(PoolKey, raea_kgpu::Slot)> = Vec::new();
        let mut masks: Vec<Option<String>> = Vec::new();
        let mut slots_of: HashMap<String, usize> = HashMap::new();
        for c in clips {
            let v = &c.vals;
            let from = num(v, "from", 0.0) as i64;
            let to = v.get("to").map(|x| x.num() as i64).unwrap_or(i64::MAX);
            let opacity = num(v, "opacity", 1.0) as f32 as f64;
            if f < from || f >= to || opacity <= 0.0 {
                continue;
            }
            let Some(V::S(src)) = v.get("src") else { return Err(format!("clip {}: no src", c.id)) };
            let slot = {
                let s = slots_of.entry(src.clone()).or_insert(0);
                *s += 1;
                *s - 1
            };
            let at = num(v, "src_in", 0.0) + (f - from) as f64 / fps;
            if !self.decoders.contains_key(&(src.clone(), slot)) {
                let p = crate::engine::resolve_src(&self.rows, src, &self.dir)?;
                let d = self.backend.open_video(&p)?;
                self.decoders.insert((src.clone(), slot), d);
            }
            let dec = self.decoders.get_mut(&(src.clone(), slot)).unwrap();
            // the pool is keyed by the frame's index, known without decoding: a frame in the pool isn't decoded again
            let idx = dec.index_at(at);
            let (dw, dh) = (dec.info().width, dec.info().height);
            let hit = idx.map(|i| (src.clone(), i as i64)).filter(|k| self.pool.contains_key(k));
            let key: PoolKey = match hit {
                Some(k) => {
                    self.pool_used.insert(k.clone(), tick);
                    k
                }
                None => {
                    #[cfg(target_vendor = "apple")]
                    let zc_on = self.zc.is_some();
                    #[cfg(not(target_vendor = "apple"))]
                    let zc_on = false;
                    let (pts, pooled) = if zc_on {
                        #[cfg(target_vendor = "apple")]
                        {
                            let sf = dec.frame_surface_at(at)?.ok_or("RAEA_ZEROCOPY needs the apple backend's decoders")?;
                            let planes = self.zc.as_ref().unwrap().0.wrap(&self.gpu.device, sf.buffer, sf.width, sf.height)?;
                            (sf.pts, Pooled::Surf(planes))
                        }
                        #[cfg(not(target_vendor = "apple"))]
                        unreachable!()
                    } else {
                        let fr = dec.frame_nv12_at(at)?;
                        (fr.pts, Pooled::Buf(self.upload(&fr.rgba)))
                    };
                    let key = (src.clone(), idx.map(|i| i as i64).unwrap_or((pts * 1e6).round() as i64 + (1 << 40)));
                    self.pool_bytes += pooled.bytes();
                    self.pool.insert(key.clone(), pooled);
                    self.pool_used.insert(key.clone(), tick);
                    ft.uploads += 1;
                    // over budget: the least recently used frames not in this frame go
                    if self.pool_bytes > crate::engine::pool_budget() {
                        let mut old: Vec<(PoolKey, u64)> = self.pool_used.iter().filter(|(_, t)| **t != tick).map(|(k, t)| (k.clone(), *t)).collect();
                        old.sort_by_key(|x| x.1);
                        for (k, _) in old {
                            if self.pool_bytes <= crate::engine::pool_budget() {
                                break;
                            }
                            if let Some(b) = self.pool.remove(&k) {
                                self.pool_bytes -= b.bytes();
                            }
                            self.pool_used.remove(&k);
                        }
                    }
                    key
                }
            };
            let fr = (dw, dh);
            let (sw, sh) = (fr.0 as f64, fr.1 as f64);
            // the row's values through f32, as the engine's World holds them, so the slots (and frames) are the engine's
            let n32 = |k: &str, d: f64| num(v, k, d) as f32 as f64;
            let (x, y, w, h) = (n32("x", 0.0), n32("y", 0.0), n32("w", fw), n32("h", fh));
            let zoom = n32("zoom", 1.0).max(0.01);
            let (cx, cy, cw, ch) = crate::engine::cover(sw, sh, w, h, zoom, (n32("pan_x", 0.0), n32("pan_y", 0.0)));
            let q = |v: f64| (v * 65536.0).round() as i32;
            masks.push(crate::shape::mask_of(v));
            slots.push((
                key,
                raea_kgpu::Slot {
                    on: 1,
                    bx: x.round() as i32,
                    by: y.round() as i32,
                    ex: (x + w).round() as i32,
                    ey: (y + h).round() as i32,
                    sw: fr.0 as i32,
                    sh: fr.1 as i32,
                    cx: q(cx - cw / 2.0),
                    cy: q(cy - ch / 2.0),
                    dx: q(cw / w),
                    dy: q(ch / h),
                    al: (opacity.min(1.0) * 256.0).round() as i32,
                },
            ));
        }
        if slots.len() > 4 {
            return Err(format!("frame {f}: more than four clips at once"));
        }
        ft.decode_upload = t.elapsed().as_secs_f64() * 1e3;

        // one composite into the resident frame; clips with a mask (a solid in space and time) show its slice
        let t = Instant::now();
        let mut sl: Vec<raea_kgpu::Slot> = (0..4).map(|s| slots.get(s).map(|x| x.1).unwrap_or_else(raea_kgpu::Slot::off)).collect();
        let mask = crate::shape::slot_masks(&masks, &mut sl, f, self.w as usize, self.h as usize, self.prog.frames)?;
        let mask_buf = mask.map(|m| self.gpu.buffer(m.len() * 4, Some(bytemuck::cast_slice(&m))));
        let mbuf = mask_buf.as_ref().unwrap_or(&self.no_mask);
        let mut words: Vec<u8> = Vec::new();
        for s in sl.iter() {
            words.extend(s.words().iter().flat_map(|v| v.to_le_bytes()));
        }
        let params = self.gpu.buffer(words.len(), Some(&words));
        let o = &self.overlays[&dg];
        let n = (self.w * self.h) as usize;
        let ts = self.ts.as_ref().map(|q| (q, 2));
        #[cfg(target_vendor = "apple")]
        if let Some((_, comp_tex, dy, duv, _)) = &self.zc {
            // zero-copy: each slot's planes are the decoder's own buffer, as textures (composite_tex.k)
            let mut ys: Vec<&wgpu::TextureView> = Vec::new();
            let mut uvs: Vec<&wgpu::TextureView> = Vec::new();
            for s in 0..4 {
                match slots.get(s).map(|x| &self.pool[&x.0]) {
                    Some(Pooled::Surf(p)) => {
                        ys.push(&p.y);
                        uvs.push(&p.uv);
                    }
                    Some(Pooled::Buf(_)) => return Err("zero-copy: a pooled frame was copied".into()),
                    None => {
                        ys.push(dy);
                        uvs.push(duv);
                    }
                }
            }
            let tex: Vec<&wgpu::TextureView> = std::iter::once(&self.bg_view).chain(ys).chain(uvs).collect();
            self.gpu.dispatch_ts(comp_tex, &[o, &params, mbuf], &tex, &[&self.out], n, wait, ts);
            // RAEA_ZC_FLUSH=0 keeps CoreVideo's texture cache as it is (diagnostics); by default it's flushed each frame
            if std::env::var("RAEA_ZC_FLUSH").map(|v| v != "0").unwrap_or(true) {
                self.zc.as_ref().unwrap().0.flush();
            }
            ft.composite_gpu = t.elapsed().as_secs_f64() * 1e3;
            ft.total = t0.elapsed().as_secs_f64() * 1e3;
            return Ok(ft);
        }
        let planes: Vec<&wgpu::Buffer> = (0..4)
            .map(|s| match slots.get(s).map(|x| &self.pool[&x.0]) {
                Some(Pooled::Buf(b)) => b,
                _ => &self.dummy,
            })
            .collect();
        self.gpu.dispatch_ts(&self.comp, &[planes[0], planes[1], planes[2], planes[3], o, &params, mbuf], &[&self.bg_view], &[&self.out], n, wait, ts);
        ft.composite_gpu = t.elapsed().as_secs_f64() * 1e3;
        ft.total = t0.elapsed().as_secs_f64() * 1e3;
        Ok(ft)
    }

    /// Every frame's state, evaluated in parallel (the rules are pure), into the cache.
    pub fn states_all(&mut self, frames: &[i64]) -> Result<(), String> {
        let need: Vec<i64> = frames.iter().copied().filter(|f| !self.states.contains_key(f)).collect();
        if need.is_empty() {
            return Ok(());
        }
        let got: Vec<(i64, Vec<Tup>)> = {
            let tl = self.timeline()?;
            need.iter().map(|&f| (f, tl.tups(f))).collect()
        };
        self.states.extend(got);
        Ok(())
    }

    /// The overlay at frame f rendered by Blitz (premultiplied RGBA, full size).
    fn render_overlay(&mut self, st: &[Tup], f: i64) -> Vec<u8> {
        if let Some(px) = self.pool_px.take() {
            return px;
        }
        if let Some(p) = self.page.as_mut() {
            let mut px = Vec::new();
            p.frame(f as f64 / self.prog.fps.max(1.0), &mut px);
            return px;
        }
        if let Some(px) = self.page_image(st, f) {
            return px;
        }
        self.set_rows(st);
        let mut px = Vec::new();
        self.rw.render(f, &mut px);
        px
    }

    /// A frame whose only 2D row showing is one image over the whole canvas, as raea-motion's pages are: its picture
    /// is the overlay (transparent canvas, so the picture premultiplied, as vello takes it), decoded from its asset
    /// without painting a document around it. A page already painted once (on the GPU) was painted again here, for
    /// every frame (2026-10-17).
    fn page_image(&self, st: &[Tup], f: i64) -> Option<Vec<u8>> {
        let mut on = st.iter().filter(|t| crate::engine::is_2d(&t.kind)).filter(|t| {
            let v = &t.vals;
            f >= num(v, "from", f64::MIN) as i64 && v.get("to").map(|x| f < x.num() as i64).unwrap_or(true)
        });
        let t = on.next()?;
        if on.next().is_some() || t.kind != "image" {
            return None;
        }
        let v = &t.vals;
        let plain = v.keys().all(|k| matches!(k.as_str(), "src" | "x" | "y" | "w" | "h" | "from" | "to" | "z"));
        let whole = num(v, "x", 0.0) == 0.0 && num(v, "y", 0.0) == 0.0 && num(v, "w", -1.0) == self.w as f64 && num(v, "h", -1.0) == self.h as f64;
        let Some(V::S(src)) = v.get("src") else { return None };
        let hash = src.strip_prefix("asset:")?;
        if !(plain && whole) {
            return None;
        }
        let img = image::open(self.dir.join("assets").join(format!("{hash}.png"))).ok()?.into_rgba8();
        if img.width() != self.w || img.height() != self.h {
            return None;
        }
        let mut px = img.into_raw();
        vello_common::pixmap::premultiply_rgba8(&mut px);
        Some(px)
    }

    /// Whether `page_image` would take frame `f` (cheap: the rows only, no decode).
    fn page_image_rows(&self, st: &[Tup], f: i64) -> Option<()> {
        let mut on = st.iter().filter(|t| crate::engine::is_2d(&t.kind)).filter(|t| {
            let v = &t.vals;
            f >= num(v, "from", f64::MIN) as i64 && v.get("to").map(|x| f < x.num() as i64).unwrap_or(true)
        });
        let t = on.next()?;
        (on.next().is_none() && t.kind == "image").then_some(())
    }

    /// A row overlay's paint for frame `f`, started (render_overlay's row path, without waiting for the GPU).
    fn submit_overlay(&mut self, st: &[Tup], f: i64) -> raea::row_render::Painting {
        self.set_rows(st);
        self.rw.submit(f)
    }

    /// Each 2D row's frame values onto its DOM node, where they changed (the engine's to_dom).
    fn set_rows(&mut self, st: &[Tup]) {
        for row in st.iter().filter(|t| crate::engine::is_2d(&t.kind)) {
            let Some(e) = self.ents.get(&format!("{}:{}", row.kind, row.id)).copied() else { continue };
            let Some(base) = self.rw.world.get::<crate::engine::Base2d>(e) else { continue };
            let p = crate::engine::props_2d(base, &row.vals);
            if let Some(mut r) = self.rw.world.get_mut::<raea::world::Resolved>(e) {
                if r.0 != p {
                    r.0 = p;
                }
            }
        }
    }

    /// The whole edit (or `frames`) at proxy size, in one dispatch, into the returned resident buffer.
    pub fn sketch(&mut self, frames: &[i64]) -> Result<(wgpu::Buffer, SketchTimes), String> {
        let mut t = SketchTimes { frames: frames.len(), ..Default::default() };
        let t0 = Instant::now();
        self.states_all(frames)?;
        t.states = t0.elapsed().as_secs_f64() * 1e3;
        // ingest: proxies for clip frames and overlays not seen yet (done once; a real app does it at ingest)
        let t1 = Instant::now();
        let (fps, fw, fh) = (self.prog.fps, self.w as f64, self.h as f64);
        let s = self.pw as f64 / fw;
        let mut q: Vec<i32> = Vec::with_capacity(frames.len() * 53);
        let mut tparams = 0.0;
        for &f in frames {
            let st = self.states[&f].clone();
            let tp = Instant::now();
            let mut words = Vec::with_capacity(53);
            let mut clips: Vec<&Tup> = st.iter().filter(|t| t.kind == "clip").collect();
            clips.sort_by(|a, b| num(&a.vals, "z", 0.0).total_cmp(&num(&b.vals, "z", 0.0)).then(a.id.cmp(&b.id)));
            let mut slots_of: HashMap<String, usize> = HashMap::new();
            tparams += tp.elapsed().as_secs_f64();
            for c in clips {
                let v = &c.vals;
                let from = num(v, "from", 0.0) as i64;
                let to = v.get("to").map(|x| x.num() as i64).unwrap_or(i64::MAX);
                let opacity = num(v, "opacity", 1.0);
                if f < from || f >= to || opacity <= 0.0 || words.len() >= 52 {
                    continue;
                }
                let Some(V::S(src)) = v.get("src") else { return Err(format!("clip {}: no src", c.id)) };
                let slot = {
                    let s = slots_of.entry(src.clone()).or_insert(0);
                    *s += 1;
                    *s - 1
                };
                let at = num(v, "src_in", 0.0) + (f - from) as f64 / fps;
                if !self.decoders.contains_key(&(src.clone(), slot)) {
                    let p = crate::engine::resolve_src(&self.rows, src, &self.dir)?;
                    let d = self.backend.open_video(&p)?;
                    self.decoders.insert((src.clone(), slot), d);
                }
                let dec = self.decoders.get_mut(&(src.clone(), slot)).unwrap();
                let idx = dec.index_at(at);
                let (sw, sh) = (dec.info().width as usize, dec.info().height as usize);
                let key = (src.clone(), idx.map(|i| i as i64).unwrap_or((at * 1e6) as i64 + (1 << 40)));
                let (psw, psh) = (((sw as f64 * s / 2.0).round() as usize * 2).max(4), ((sh as f64 * s / 2.0).round() as usize * 2).max(4));
                if !self.pp_index.contains_key(&key) {
                    let fr = dec.frame_nv12_at(at)?;
                    let small = shrink_nv12(&fr.rgba, sw, sh, psw, psh);
                    // word-align each plane's start (byte reads index words)
                    let off = self.pp.len().div_ceil(4) * 4;
                    self.pp.resize(off, 0);
                    self.pp.extend_from_slice(&small);
                    self.pp_index.insert(key.clone(), (off, psw as u32, psh as u32));
                    t.new_proxies += 1;
                }
                let tp = Instant::now();
                let (off, psw, psh) = self.pp_index[&key];
                let (x, y, w, h) = (num(v, "x", 0.0) * s, num(v, "y", 0.0) * s, num(v, "w", fw) * s, num(v, "h", fh) * s);
                let zoom = num(v, "zoom", 1.0).max(0.01);
                let (sw, sh) = (psw as f64, psh as f64);
                let (cx, cy, cw, ch) = crate::engine::cover(sw, sh, w, h, zoom, (num(v, "pan_x", 0.0), num(v, "pan_y", 0.0)));
                let qf = |v: f64| (v * 65536.0).round() as i32;
                let sl = raea_kgpu::Slot {
                    on: 1,
                    bx: x.round() as i32,
                    by: y.round() as i32,
                    ex: (x + w).round() as i32,
                    ey: (y + h).round() as i32,
                    sw: psw as i32,
                    sh: psh as i32,
                    cx: qf(cx - cw / 2.0),
                    cy: qf(cy - ch / 2.0),
                    dx: qf(cw / w),
                    dy: qf(ch / h),
                    al: (opacity.min(1.0) * 256.0).round() as i32,
                };
                words.extend(sl.words());
                words.push(off as i32);
                tparams += tp.elapsed().as_secs_f64();
            }
            while words.len() < 52 {
                words.extend(raea_kgpu::Slot::off().words());
                words.push(0);
            }
            // the overlay proxy for this frame's overlay state
            let dg = self.overlay_key(&st, f);
            if !self.oo_index.contains_key(&dg) {
                let full = self.render_overlay(&st, f);
                let small = shrink_rgba(&full, self.w as usize, self.h as usize, self.pw as usize, self.ph as usize);
                self.oo_index.insert(dg, self.oo.len());
                self.oo.extend(small);
                t.new_overlays += 1;
            }
            words.push(self.oo_index[&dg] as i32);
            q.extend(words);
        }
        t.ingest = t1.elapsed().as_secs_f64() * 1e3 - tparams * 1e3;
        t.params = tparams * 1e3;
        // upload: the packed proxies (only when they grew) and the parameters
        let t2 = Instant::now();
        // the packs stay on the GPU; they go up again only when ingest grew them
        if self.pp_gpu.as_ref().map(|(_, n)| *n) != Some(self.pp.len()) {
            self.pp_gpu = Some((self.gpu.buffer(self.pp.len().max(4), Some(&self.pp)), self.pp.len()));
        }
        if self.oo_gpu.as_ref().map(|(_, n)| *n) != Some(self.oo.len()) {
            self.oo_gpu = Some((self.gpu.buffer(self.oo.len().max(1) * 4, Some(bytemuck::cast_slice(&self.oo))), self.oo.len()));
        }
        let (pp, oo) = (&self.pp_gpu.as_ref().unwrap().0, &self.oo_gpu.as_ref().unwrap().0);
        let qb = self.gpu.buffer(q.len() * 4, Some(bytemuck::cast_slice(&q)));
        let n = frames.len() * (self.pw * self.ph) as usize;
        let out = self.gpu.buffer(n * 4, None);
        self.gpu.queue.submit([]);
        let _ = self.gpu.device.poll(wgpu::PollType::wait_indefinitely());
        t.upload = t2.elapsed().as_secs_f64() * 1e3;
        let t3 = Instant::now();
        self.gpu.dispatch(&self.sketch_k, &[pp, oo, &qb], &[], &[&out], n, true);
        t.gpu = t3.elapsed().as_secs_f64() * 1e3;
        let _ = self.bgc;
        Ok((out, t))
    }

    /// Swap in an edited program (after a snippet): the frames whose content changed are found by comparing every
    /// frame's digest (its visible rows; the same digest as incremental export), states evaluated in parallel.
    /// Returns the changed frames; the caches (pool, proxies, overlays) carry over.
    pub fn replace(&mut self, prog: &Program, rows: &[Tup]) -> Result<Vec<i64>, String> {
        let all: Vec<i64> = (0..prog.frames.max(self.prog.frames)).collect();
        self.states_all(&all.iter().copied().filter(|&f| f < self.prog.frames).collect::<Vec<_>>())?;
        let before: HashMap<i64, u64> = all.iter().filter_map(|&f| self.states.get(&f).map(|st| (f, crate::frame_digest(st, f, self.prog.fps)))).collect();
        self.prog = prog.clone();
        self.rows = rows.to_vec();
        self.states.clear();
        self.tl = None;
        let now: Vec<i64> = (0..prog.frames).collect();
        self.states_all(&now)?;
        Ok(now.into_iter().filter(|f| before.get(f) != Some(&crate::frame_digest(&self.states[f], *f, prog.fps))).collect())
    }

    /// A kernel compiled once (per name and shape).
    fn kernel(&mut self, name: &str, src: &str, inputs: &[&str], sc: &HashMap<&str, i32>, out: &str) -> Result<std::rc::Rc<raea_kgpu::Compiled>, String> {
        let key = format!("{name}{:?}", { let mut v: Vec<_> = sc.iter().collect(); v.sort(); v });
        if !self.kernels.contains_key(&key) {
            let c = self.gpu.compile(&raea_kgpu::parse(src)?, inputs, sc, &[out])?;
            self.kernels.insert(key.clone(), std::rc::Rc::new(c));
        }
        Ok(self.kernels[&key].clone())
    }

    /// The look grid's features (432 a frame: 9 x 16 cells by luma, red-green, yellow-blue sums), read back.
    pub fn grid(&mut self, sketch: &wgpu::Buffer, n: usize) -> Result<Vec<i32>, String> {
        let sc: HashMap<&str, i32> = [("pw", self.pw as i32), ("ph", self.ph as i32), ("n", 0)].into();
        let cg = self.kernel("look_grid", LOOK_GRID_K, &["S"], &sc, "v")?;
        let g = self.gpu.buffer(n * 432 * 4, None);
        self.gpu.dispatch(&cg, &[sketch], &[], &[&g], n * 432, true);
        Ok(bytemuck::cast_slice(&self.gpu.read(&g, n * 432 * 4)).to_vec())
    }

    /// Look hypervectors (64 words each) for `n` sketched frames, computed from the sketch where it lies on the GPU.
    pub fn looks(&mut self, sketch: &wgpu::Buffer, n: usize) -> Result<(wgpu::Buffer, f64), String> {
        let sc: HashMap<&str, i32> = [("pw", self.pw as i32), ("ph", self.ph as i32), ("n", 0)].into();
        let cg = self.kernel("look_grid", LOOK_GRID_K, &["S"], &sc, "v")?;
        let cp = self.kernel("look_proj", LOOK_PROJ_K, &["G"], &sc, "v")?;
        let t = Instant::now();
        let g = self.gpu.buffer(n * 432 * 4, None);
        let hv = self.gpu.buffer(n * 64 * 4, None);
        self.gpu.dispatch(&cg, &[sketch], &[], &[&g], n * 432, false);
        self.gpu.dispatch(&cp, &[&g], &[], &[&hv], n * 64, true);
        Ok((hv, t.elapsed().as_secs_f64() * 1e3))
    }

    /// Every pair's look distance on the GPU, then a storyboard of k frames picked farthest-point over it (each next
    /// pick the frame least like any picked so far), against k evenly spaced frames: (picks, worst gap, even's worst
    /// gap, gpu ms, total ms).
    pub fn storyboard(&mut self, hv: &wgpu::Buffer, n: usize, k: usize) -> Result<(Vec<usize>, u32, u32, f64, f64), String> {
        let sc: HashMap<&str, i32> = [("n", n as i32)].into();
        let ch = self.kernel("look_ham", LOOK_HAM_K, &["H"], &sc, "d")?;
        let t = Instant::now();
        let d = self.gpu.buffer(n * n * 4, None);
        let tg = Instant::now();
        self.gpu.dispatch(&ch, &[hv], &[], &[&d], n * n, true);
        let gpu_ms = tg.elapsed().as_secs_f64() * 1e3;
        let dist: Vec<u32> = bytemuck::cast_slice(&self.gpu.read(&d, n * n * 4)).to_vec();
        let mut picks = vec![0usize];
        let mut near: Vec<u32> = (0..n).map(|j| dist[j]).collect();
        while picks.len() < k.min(n) {
            let (i, _) = near.iter().enumerate().max_by_key(|(i, v)| (**v, usize::MAX - *i)).unwrap();
            picks.push(i);
            for j in 0..n {
                near[j] = near[j].min(dist[i * n + j]);
            }
        }
        picks.sort();
        let worst = *near.iter().max().unwrap();
        let even: Vec<usize> = (0..k).map(|i| (i * n / k + n / (2 * k)).min(n - 1)).collect();
        let even_worst = (0..n).map(|j| even.iter().map(|&p| dist[p * n + j]).min().unwrap()).max().unwrap();
        Ok((picks, worst, even_worst, gpu_ms, t.elapsed().as_secs_f64() * 1e3))
    }

    /// How far each changed frame's look moved: the new looks (for `frames`, in order) against the old (all frames).
    pub fn look_deltas(&mut self, old: &wgpu::Buffer, new: &wgpu::Buffer, frames: &[i64]) -> Result<(Vec<u32>, f64), String> {
        let k = self.kernel("look_pair", LOOK_PAIR_K, &["HO", "HN", "IX"], &HashMap::new(), "d")?;
        let t = Instant::now();
        let ix: Vec<i32> = frames.iter().map(|f| *f as i32).collect();
        let ib = self.gpu.buffer(ix.len() * 4, Some(bytemuck::cast_slice(&ix)));
        let d = self.gpu.buffer(frames.len() * 4, None);
        self.gpu.dispatch(&k, &[old, new, &ib], &[], &[&d], frames.len(), true);
        let v: Vec<u32> = bytemuck::cast_slice(&self.gpu.read(&d, frames.len() * 4)).to_vec();
        Ok((v, t.elapsed().as_secs_f64() * 1e3))
    }

    /// The direct render, for edits without 3D: the frames in order, each composited from the pool (as `frame`), made
    /// NV12 by nv12buf.k on this device and read back with three frames in flight, so the CPU works on the next frames
    /// (rules, decode, overlay) while the GPU and the copy finish earlier ones. The sink gets NV12 as the engine's GPU
    /// path gives it (luma, then interleaved chroma; no tag). No Bevy schedules run.
    pub fn render_nv12(&mut self, list: &[i64], sink: impl FnMut(i64, Vec<u8>) -> Result<(), String>) -> Result<DirectTimes, String> {
        self.render_nv12_with(list, None, sink)
    }

    /// As `render_nv12`, each frame's overlay handed over by `feed` as a page already on this device (premultiplied
    /// RGBA8 packed, quads::page_buffer on the paint device this view runs on): a page goes from the paint to the
    /// composite with no readback, no file and no upload. `feed` is asked for the frames in order, each just before
    /// its composite, so a page buffer it reuses is overwritten only after the frame before read it (one queue).
    pub fn render_nv12_fed(&mut self, list: &[i64], mut feed: impl FnMut(i64) -> Result<wgpu::Buffer, String>, sink: impl FnMut(i64, Vec<u8>) -> Result<(), String>) -> Result<DirectTimes, String> {
        self.fed_render = true;
        let r = self.render_nv12_with(list, Some(&mut feed), sink);
        self.fed_render = false;
        r
    }

    fn render_nv12_with(&mut self, list: &[i64], mut feed: Option<&mut dyn FnMut(i64) -> Result<wgpu::Buffer, String>>, mut sink: impl FnMut(i64, Vec<u8>) -> Result<(), String>) -> Result<DirectTimes, String> {
        use std::sync::atomic::{AtomicBool, Ordering};
        use std::sync::Arc;
        const RING: usize = 3;
        let (w, h) = (self.w, self.h);
        // P010: two bytes a sample (a luma word holds two samples, a chroma word one Cb Cr pair)
        let (words, bytes) = if self.ten_bit {
            ((w * h / 2 + w * h / 4 + 1) as usize, (w * h * 3) as usize)
        } else {
            ((w * h / 4 + w * h / 8 + 1) as usize, (w * h * 3 / 2) as usize)
        };
        if self.nv12.is_none() {
            let k = raea_kgpu::parse(if self.ten_bit { P010BUF_K } else { NV12BUF_K })?;
            let sc: HashMap<&str, i32> = [("w", w as i32), ("h", h as i32), ("T_w", w as i32)].into();
            let c = self.gpu.compile_tex(&k, &["F"], &["T"], &sc, &["px"])?;
            self.nv12 = Some((c, self.gpu.buffer(words * 4, None)));
        }
        let ring: Vec<wgpu::Buffer> = (0..RING)
            .map(|_| self.gpu.device.create_buffer(&wgpu::BufferDescriptor { label: Some("direct readback"), size: (words * 4) as u64, usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false }))
            .collect();
        let mut dt = DirectTimes::default();
        // RAEA_GPU_TIMES: one frame at a time, with GPU timestamps: [2,3] the composite, [4,5] nv12buf.k, [6,7] the
        // copy into the readback ring, [8..16] up to four uploads (each its own copy)
        let timing = std::env::var_os("RAEA_GPU_TIMES").is_some() && self.gpu.device.features().contains(wgpu::Features::TIMESTAMP_QUERY | wgpu::Features::TIMESTAMP_QUERY_INSIDE_ENCODERS);
        if std::env::var_os("RAEA_GPU_TIMES").is_some() && !timing {
            eprintln!("RAEA_GPU_TIMES: this device has no timestamp queries inside encoders: GPU stage times unsupported");
        }
        dt.gpu_timed = timing;
        if timing {
            self.ts = Some(self.gpu.device.create_query_set(&wgpu::QuerySetDescriptor { label: None, ty: wgpu::QueryType::Timestamp, count: 16 }));
        }
        let resolve = self.gpu.device.create_buffer(&wgpu::BufferDescriptor { label: None, size: 128, usage: wgpu::BufferUsages::QUERY_RESOLVE | wgpu::BufferUsages::COPY_SRC, mapped_at_creation: false });
        let period = self.gpu.queue.get_timestamp_period() as f64;
        let depth = if timing { 1 } else { RING };
        let t_all = Instant::now();
        // in flight: (frame, ring slot, ready flag, submission)
        let mut flight: std::collections::VecDeque<(i64, usize, Arc<AtomicBool>, wgpu::SubmissionIndex)> = Default::default();
        let mut drain = |v: &mut Self, flight: &mut std::collections::VecDeque<(i64, usize, Arc<AtomicBool>, wgpu::SubmissionIndex)>, dt: &mut DirectTimes| -> Result<(), String> {
            let (f, slot, ready, sub) = flight.pop_front().unwrap();
            let t = Instant::now();
            while !ready.load(Ordering::Acquire) {
                let _ = v.gpu.device.poll(wgpu::PollType::Wait { submission_index: Some(sub.clone()), timeout: None });
            }
            let waited = t.elapsed().as_secs_f64() * 1e3;
            dt.wait += waited;
            if let Some(log) = FRAME_WAITS.lock().unwrap().as_mut() {
                log.push((f, waited));
            }
            let t = Instant::now();
            let px = ring[slot].slice(..).get_mapped_range()[..bytes].to_vec();
            ring[slot].unmap();
            dt.copy += t.elapsed().as_secs_f64() * 1e3;
            let t = Instant::now();
            sink(f, px)?;
            dt.sink += t.elapsed().as_secs_f64() * 1e3;
            Ok(())
        };
        for (n, &f) in list.iter().enumerate() {
            if flight.len() == depth {
                drain(self, &mut flight, &mut dt)?;
            }
            self.ts_uploads = 0;
            if let Some(feed) = feed.as_mut() {
                let t = Instant::now();
                self.fed = Some(feed(f)?);
                dt.fed += t.elapsed().as_secs_f64() * 1e3;
            }
            let ft = self.frame_wait(f, false)?;
            dt.rules += ft.rules;
            dt.overlay += ft.overlay;
            dt.decode_upload += ft.decode_upload;
            dt.uploads += ft.uploads;
            let t = Instant::now();
            let (c, nb) = self.nv12.as_ref().unwrap();
            self.gpu.dispatch_ts(c, &[&self.out], &[&self.bg_view], &[nb], words, false, self.ts.as_ref().map(|q| (q, 4)));
            let slot = n % RING;
            let mut enc = self.gpu.device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("rdb frame") });
            if let Some(q) = self.ts.as_ref() {
                enc.write_timestamp(q, 6);
            }
            enc.copy_buffer_to_buffer(nb, 0, &ring[slot], 0, (words * 4) as u64);
            if let Some(q) = self.ts.as_ref() {
                enc.write_timestamp(q, 7);
                enc.resolve_query_set(q, 0..16, &resolve, 0);
            }
            let sub = self.gpu.queue.submit([enc.finish()]);
            raea_gputrace::submitted(&self.gpu.queue, "rdb", "rdb frame");
            let ready = Arc::new(AtomicBool::new(false));
            let r2 = ready.clone();
            ring[slot].slice(..).map_async(wgpu::MapMode::Read, move |_| r2.store(true, Ordering::Release));
            dt.submit += t.elapsed().as_secs_f64() * 1e3;
            flight.push_back((f, slot, ready, sub));
            if timing {
                let t: Vec<u64> = bytemuck::cast_slice(&self.gpu.read(&resolve, 128)).to_vec();
                let ms = |a: usize, b: usize| t[b].saturating_sub(t[a]) as f64 * period / 1e6;
                let up: f64 = (0..self.ts_uploads as usize).map(|k| ms(8 + 2 * k, 9 + 2 * k)).sum();
                let (c, n12, cp) = (ms(2, 3), ms(4, 5), ms(6, 7));
                dt.gpu_upload += up;
                dt.gpu_composite += c;
                dt.gpu_nv12 += n12;
                dt.gpu_copy += cp;
                dt.gpu_total += up + c + n12 + cp;
            }
        }
        while !flight.is_empty() {
            drain(self, &mut flight, &mut dt)?;
        }
        dt.frames = list.len();
        dt.total = t_all.elapsed().as_secs_f64() * 1e3;
        Ok(dt)
    }

    /// The current frame, read back as RGBA.
    pub fn read(&self) -> Vec<u8> {
        self.gpu.read(&self.out, (self.w * self.h * 4) as usize)
    }
}

/// The direct render's time, ms summed over its frames.
#[derive(Default, Debug)]
pub struct DirectTimes {
    pub frames: usize,
    pub rules: f64,
    pub overlay: f64,
    pub decode_upload: f64,
    pub uploads: usize,
    /// composite already submitted; nv12buf.k, the copy into the ring and the map, recorded and submitted
    pub submit: f64,
    /// waiting for the oldest frame in flight
    pub wait: f64,
    /// out of the mapped ring buffer
    pub copy: f64,
    /// the sink (the encoder, or hashing)
    pub sink: f64,
    /// the caller making each frame's page (render_nv12_fed: React, layout and the paint's recording)
    pub fed: f64,
    pub total: f64,
    /// RAEA_GPU_TIMES, GPU ms from timestamps: the uploads (clip planes, a new overlay: each a copy from a staging
    /// buffer), composite.k, nv12buf.k, the copy into the readback ring, and their sum
    pub gpu_upload: f64,
    pub gpu_composite: f64,
    pub gpu_nv12: f64,
    pub gpu_copy: f64,
    pub gpu_total: f64,
    pub gpu_timed: bool,
}

impl DirectTimes {
    pub fn json(&self) -> serde_json::Value {
        let n = self.frames.max(1) as f64;
        serde_json::json!({"frames": self.frames, "fps": n / (self.total / 1e3), "ms_per_frame": {"rules": self.rules / n, "overlay": self.overlay / n,
            "decode_upload": self.decode_upload / n, "submit": self.submit / n, "wait": self.wait / n, "copy": self.copy / n, "sink": self.sink / n,
            "fed": self.fed / n, "wall": self.total / n}, "uploads": self.uploads,
            "gpu_ms_per_frame": if self.gpu_timed { serde_json::json!({"upload": self.gpu_upload / n, "composite": self.gpu_composite / n, "nv12": self.gpu_nv12 / n, "copy": self.gpu_copy / n, "total": self.gpu_total / n}) } else { serde_json::Value::Null }})
    }
}

impl FrameTimes {
    pub fn json(&self) -> serde_json::Value {
        serde_json::json!({"rules": self.rules, "overlay": self.overlay, "decode_upload": self.decode_upload, "composite_gpu": self.composite_gpu,
            "total": self.total, "uploads": self.uploads, "overlay_cached": self.overlay_cached})
    }
}
