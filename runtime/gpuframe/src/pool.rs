//! The frame pool on the GPU: frames composited and kept for reuse within a byte budget, and clips drawn on the GPU.

use super::*;

/// The GPU frame path (G8): clips are NV12 planes in a GPU pool, composited by kernels/composite.k over the camera's
/// target with the Blitz overlay, then kernels/nv12buf.k makes the NV12 readback. Built fresh each update by the
/// main world: what to draw, what to upload, what to evict, and the overlay when it changed.
pub type PoolKey = (String, i64);

#[derive(Resource, Clone, Default, ExtractResource)]
#[extract_app(bevy::render::RenderApp)]
pub struct GpuFrame {
    pub on: bool,
    pub slots: Vec<(PoolKey, raea_core_k::Slot)>,
    pub uploads: Vec<(PoolKey, std::sync::Arc<Vec<u8>>)>,
    pub evict: Vec<PoolKey>,
    pub overlay: Option<std::sync::Arc<Vec<u8>>>,
    /// kgpu's mask M for the masked clips (their slots' on = 2), when any clip has a mask
    pub mask: Option<std::sync::Arc<Vec<u32>>>,
}

/// The main world's book of what the GPU pool holds (bytes, last frame used), so uploads happen once per source
/// frame and the least recently used go when the budget (1 GB) is passed.
#[derive(Resource, Default)]
pub struct PoolBook {
    pub resident: HashMap<PoolKey, (usize, u64)>,
    pub bytes: usize,
    pub tick: u64,
    pub uploads: u64,
    pub hits: u64,
}


/// The render world's half: the pooled planes and the kernels.
#[derive(Resource, Default)]
pub struct PoolGpu {
    /// RAEA_GPU_TIMES (with RAEA_SERIAL): timestamps per frame, [2,3] composite.k, [4,5] nv12buf.k, [6,7] the
    /// overlay's upload, [8..16] clip uploads (each a staging copy); resolved into the first buffer, read via the second
    pub ts: Option<(wgpu::QuerySet, wgpu::Buffer, wgpu::Buffer)>,
    pub ts_uploads: u32,
    pub ts_overlay: bool,
    pub ts_pending: bool,
    /// ms summed: clip uploads, overlay upload, composite, nv12; and frames timed
    pub ts_ms: [f64; 4],
    pub ts_frames: u64,
    /// said once that timestamps are unsupported
    pub ts_unsupported_said: bool,
    pub bufs: HashMap<PoolKey, wgpu::Buffer>,
    pub overlay: Option<wgpu::Buffer>,
    pub frame: Option<wgpu::Buffer>,
    /// M when no clip is masked: zeros (composite.k reads it for every pixel)
    pub no_mask: Option<wgpu::Buffer>,
    pub dummy: Option<wgpu::Buffer>,
    pub comp: Option<wgpu::ComputePipeline>,
    pub nv12: Option<wgpu::ComputePipeline>,
}

pub const COMPOSITE_K: &str = include_str!("../../../core/k/kernels/composite.k");
pub const NV12BUF_K: &str = include_str!("../../../core/k/kernels/nv12buf.k");

pub fn compile_k(device: &wgpu::Device, src: &str, buffers: &[&str], w: u32, h: u32) -> wgpu::ComputePipeline {
    let k = raea_core_k::parse(src).expect("kernel parses");
    let sc: std::collections::HashMap<&str, i32> = [("w", w as i32), ("h", h as i32), ("T_w", w as i32)].into();
    let wgsl = raea_sys_k::wgsl_tex(&k, buffers, &["T"], &sc, &["px"]).expect("kernel compiles");
    let module = device.create_shader_module(wgpu::ShaderModuleDescriptor { label: Some("k"), source: wgpu::ShaderSource::Wgsl(wgsl.into()) });
    device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
        label: Some("k"),
        layout: None,
        module: &module,
        entry_point: Some("main"),
        compilation_options: Default::default(),
        cache: None,
    })
}

pub fn storage(device: &wgpu::Device, size: u64) -> wgpu::Buffer {
    device.create_buffer(&wgpu::BufferDescriptor {
        label: None,
        size: size.div_ceil(4) * 4,
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::COPY_SRC,
        mapped_at_creation: false,
    })
}

pub fn shape(device: &wgpu::Device, n: u32) -> (wgpu::Buffer, u32, u32) {
    let groups = n.div_ceil(256);
    let gx = groups.min(65535);
    let gy = groups.div_ceil(gx);
    let b = wgpu::util::DeviceExt::create_buffer_init(device, &wgpu::util::BufferInitDescriptor {
        label: None,
        contents: &[n.to_le_bytes(), (gx * 256).to_le_bytes()].concat(),
        usage: wgpu::BufferUsages::UNIFORM,
    });
    (b, gx, gy)
}

/// Upload, evict, composite, convert: one encoder after the cameras.
pub fn gpu_frame_pass(world: &mut World) {
    let Some(job) = world.get_resource::<Nv12Job>().cloned() else { return };
    let Some(frame) = world.get_resource::<GpuFrame>().cloned() else { return };
    if !frame.on {
        return;
    }
    let device = world.resource::<bevy::render::renderer::RenderDevice>().wgpu_device().clone();
    let queue = world.resource::<bevy::render::renderer::RenderQueue>().clone().into_inner();
    let (w, h) = (job.w, job.h);
    {
        let mut pool = world.resource_mut::<PoolGpu>();
        if pool.comp.is_none() {
            pool.comp = Some(compile_k(&device, COMPOSITE_K, &["P0", "P1", "P2", "P3", "O", "C", "M"], w, h));
            pool.no_mask = Some(wgpu::util::DeviceExt::create_buffer_init(&device, &wgpu::util::BufferInitDescriptor { label: None, contents: &vec![0u8; (w * h * 4) as usize], usage: wgpu::BufferUsages::STORAGE }));
            pool.nv12 = Some(compile_k(&device, NV12BUF_K, &["F"], w, h));
            pool.overlay = Some(storage(&device, (w * h * 4) as u64));
            pool.frame = Some(storage(&device, (w * h * 4) as u64));
            pool.dummy = Some(storage(&device, 24));
        }
        let want = raea_io_trace::stat("RAEA_GPU_TIMES");
        let timing = want && device.features().contains(wgpu::Features::TIMESTAMP_QUERY | wgpu::Features::TIMESTAMP_QUERY_INSIDE_ENCODERS);
        if want && !timing && !pool.ts_unsupported_said {
            pool.ts_unsupported_said = true;
            eprintln!("RAEA_GPU_TIMES: this device has no timestamp queries inside encoders: GPU stage times unsupported");
        }
        if timing && pool.ts.is_none() {
            let q = device.create_query_set(&wgpu::QuerySetDescriptor { label: None, ty: wgpu::QueryType::Timestamp, count: 16 });
            let r = device.create_buffer(&wgpu::BufferDescriptor { label: None, size: 128, usage: wgpu::BufferUsages::QUERY_RESOLVE | wgpu::BufferUsages::COPY_SRC, mapped_at_creation: false });
            let m = device.create_buffer(&wgpu::BufferDescriptor { label: None, size: 128, usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false });
            pool.ts = Some((q, r, m));
        }
        pool.ts_uploads = 0;
        pool.ts_overlay = false;
        for k in &frame.evict {
            pool.bufs.remove(k);
        }
        // timed, an upload is a staging buffer and a copy between two timestamps (what write_buffer does underneath)
        let upload = |pool: &mut PoolGpu, dst: &wgpu::Buffer, data: &[u8], pair: Option<u32>| {
            let mut d = data.to_vec();
            // NV12 is w*h*1.5 bytes: pad to a word for write_buffer
            d.resize(d.len().div_ceil(4) * 4, 0);
            match (pool.ts.as_ref(), pair) {
                (Some((q, _, _)), Some(i)) => {
                    let st = wgpu::util::DeviceExt::create_buffer_init(&device, &wgpu::util::BufferInitDescriptor { label: None, contents: &d, usage: wgpu::BufferUsages::COPY_SRC });
                    let mut enc = device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("raea-render engine") });
                    enc.write_timestamp(q, i);
                    enc.copy_buffer_to_buffer(&st, 0, dst, 0, d.len() as u64);
                    enc.write_timestamp(q, i + 1);
                    queue.submit([enc.finish()]);
                    raea_gpu_device::submitted(&queue, "render", "raea-render engine");
                }
                _ => queue.write_buffer(dst, 0, &d),
            }
        };
        for (k, data) in &frame.uploads {
            let b = storage(&device, data.len() as u64);
            let pair = (pool.ts_uploads < 4).then(|| 8 + 2 * pool.ts_uploads);
            if pair.is_some() && pool.ts.is_some() {
                pool.ts_uploads += 1;
            }
            upload(&mut pool, &b, data, pair);
            pool.bufs.insert(k.clone(), b);
        }
        if let Some(o) = &frame.overlay {
            let ob = pool.overlay.clone().unwrap();
            pool.ts_overlay = pool.ts.is_some();
            upload(&mut pool, &ob, o, Some(6));
        }
    }
    let (Some(img), Some(out)) = (world.resource::<RenderAssets<GpuImage>>().get(&job.target), world.resource::<RenderAssets<GpuShaderBuffer>>().get(&job.out)) else {
        return;
    };
    let view = img.texture.create_view(&wgpu::TextureViewDescriptor { format: Some(wgpu::TextureFormat::Rgba8Unorm), ..Default::default() });
    let pool = world.resource::<PoolGpu>();
    let mut words: Vec<u8> = Vec::with_capacity(4 * 12 * 4);
    let mut planes: Vec<&wgpu::Buffer> = Vec::new();
    for s in 0..4 {
        match frame.slots.get(s) {
            Some((k, slot)) if pool.bufs.contains_key(k) => {
                planes.push(&pool.bufs[k]);
                words.extend(slot.words().iter().flat_map(|v| v.to_le_bytes()));
            }
            _ => {
                planes.push(pool.dummy.as_ref().unwrap());
                words.extend(raea_core_k::Slot::off().words().iter().flat_map(|v| v.to_le_bytes()));
            }
        }
    }
    let params = wgpu::util::DeviceExt::create_buffer_init(&device, &wgpu::util::BufferInitDescriptor { label: None, contents: &words, usage: wgpu::BufferUsages::STORAGE });
    let (comp, nv12) = (pool.comp.clone().unwrap(), pool.nv12.clone().unwrap());
    let (fbuf, obuf) = (pool.frame.as_ref().unwrap(), pool.overlay.as_ref().unwrap());
    let mut enc = device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("raea-render engine") });
    {
        let (sh, gx, gy) = shape(&device, w * h);
        let mut e: Vec<wgpu::BindGroupEntry> = planes.iter().enumerate().map(|(k, b)| wgpu::BindGroupEntry { binding: k as u32, resource: b.as_entire_binding() }).collect();
        let mask_buf = frame.mask.as_ref().map(|m| wgpu::util::DeviceExt::create_buffer_init(&device, &wgpu::util::BufferInitDescriptor { label: None, contents: bytemuck::cast_slice(m), usage: wgpu::BufferUsages::STORAGE }));
        let mbuf = mask_buf.as_ref().unwrap_or(pool.no_mask.as_ref().unwrap());
        e.push(wgpu::BindGroupEntry { binding: 4, resource: obuf.as_entire_binding() });
        e.push(wgpu::BindGroupEntry { binding: 5, resource: params.as_entire_binding() });
        e.push(wgpu::BindGroupEntry { binding: 6, resource: mbuf.as_entire_binding() });
        e.push(wgpu::BindGroupEntry { binding: 7, resource: wgpu::BindingResource::TextureView(&view) });
        e.push(wgpu::BindGroupEntry { binding: 8, resource: fbuf.as_entire_binding() });
        e.push(wgpu::BindGroupEntry { binding: 9, resource: sh.as_entire_binding() });
        let bind = device.create_bind_group(&wgpu::BindGroupDescriptor { label: None, layout: &comp.get_bind_group_layout(0), entries: &e });
        let timestamp_writes = pool.ts.as_ref().map(|(q, _, _)| wgpu::ComputePassTimestampWrites { query_set: q, beginning_of_pass_write_index: Some(2), end_of_pass_write_index: Some(3) });
        let mut pass = enc.begin_compute_pass(&wgpu::ComputePassDescriptor { label: None, timestamp_writes });
        pass.set_pipeline(&comp);
        pass.set_bind_group(0, &bind, &[]);
        pass.dispatch_workgroups(gx, gy, 1);
    }
    if job.rgba {
        // stills: the composited frame's words, then the tag texel after them (at packed_tag: a texture copy's
        // buffer offset is 256-aligned on Metal; at 700 x 260 the unaligned copy left the tag 0 and every still refused)
        enc.copy_buffer_to_buffer(fbuf, 0, &out.buffer, 0, (w * h * 4) as u64);
        enc.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo { texture: &img.texture, mip_level: 0, origin: wgpu::Origin3d { x: 8, y: h, z: 0 }, aspect: wgpu::TextureAspect::All },
            wgpu::TexelCopyBufferInfo { buffer: &out.buffer, layout: wgpu::TexelCopyBufferLayout { offset: packed_tag(w, h) as u64, bytes_per_row: None, rows_per_image: None } },
            wgpu::Extent3d { width: 1, height: 1, depth_or_array_layers: 1 },
        );
    } else {
        let (sh, gx, gy) = shape(&device, nv12_words(w, h) as u32);
        let bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &nv12.get_bind_group_layout(0),
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: fbuf.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::TextureView(&view) },
                wgpu::BindGroupEntry { binding: 2, resource: out.buffer.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 3, resource: sh.as_entire_binding() },
            ],
        });
        let timestamp_writes = pool.ts.as_ref().map(|(q, _, _)| wgpu::ComputePassTimestampWrites { query_set: q, beginning_of_pass_write_index: Some(4), end_of_pass_write_index: Some(5) });
        let mut pass = enc.begin_compute_pass(&wgpu::ComputePassDescriptor { label: None, timestamp_writes });
        pass.set_pipeline(&nv12);
        pass.set_bind_group(0, &bind, &[]);
        pass.dispatch_workgroups(gx, gy, 1);
    }
    if let Some((q, r, m)) = pool.ts.as_ref() {
        enc.resolve_query_set(q, 0..16, r, 0);
        enc.copy_buffer_to_buffer(r, 0, m, 0, 128);
    }
    let timed = pool.ts.is_some();
    world.resource_mut::<bevy::render::renderer::PendingCommandBuffers>().push_encoder(enc, "raea gpu frame");
    if timed {
        world.resource_mut::<PoolGpu>().ts_pending = true;
    }
}

/// The clip rows on the GPU path: each visible clip's NV12 frame (decoded once, uploaded once into the pool) and its
/// slot (cover, zoom, pan and opacity as 16.16 maps), drawn by z then id.
pub fn clips_gpu(
    clock: Res<Clock>,
    edit: Res<Edit>,
    mut media: NonSendMut<Media>,
    mut err: ResMut<Error>,
    mut st: ResMut<Stages>,
    mut book: ResMut<PoolBook>,
    mut gf: ResMut<GpuFrame>,
    frame: Res<FrameRows>,
) {
    let t_clips = std::time::Instant::now();
    gf.uploads.clear();
    gf.evict.clear();
    gf.overlay = None;
    gf.slots.clear();
    gf.mask = None;
    let mut masks: Vec<Option<String>> = Vec::new();
    book.tick += 1;
    let tick = book.tick;
    let (f, fps) = (clock.0, edit.prog.fps);
    let (fw, fh) = (edit.prog.width as f64, edit.prog.height as f64);
    let mut rows: Vec<eval::Tup> = frame.cols.rows_of("clip").into_iter().map(|r| frame.cols.tup(r)).collect();
    rows.sort_by(|a, b| num(&a.vals, "z", 0.0).total_cmp(&num(&b.vals, "z", 0.0)).then(a.id.cmp(&b.id)));
    let mut slots_of: HashMap<String, usize> = HashMap::new();
    for row in &rows {
        let v = &row.vals;
        let from = num(v, "from", 0.0) as i64;
        let to = v.get("to").map(|x| x.num() as i64).unwrap_or(i64::MAX);
        let opacity = num(v, "opacity", 1.0) as f64;
        if f < from || f >= to || opacity <= 0.0 {
            continue;
        }
        let Some(V::S(src)) = v.get("src") else {
            err.0 = Some(format!("clip {}: no src", row.id));
            return;
        };
        let slot = {
            let s = slots_of.entry(src.clone()).or_insert(0);
            *s += 1;
            *s - 1
        };
        let t = num(v, "src_in", 0.0) as f64 + (f - from) as f64 / fps;
        let m = &mut *media;
        let dec = match m.decoders.entry((src.clone(), slot)) {
            std::collections::hash_map::Entry::Occupied(o) => o.into_mut(),
            std::collections::hash_map::Entry::Vacant(e) => match resolve_src(&edit.media, src, &m.dir).and_then(|p| m.backend.open_video(&p)) {
                Ok(d) => e.insert(d),
                Err(x) => {
                    err.0 = Some(format!("clip {}: {x}", row.id));
                    return;
                }
            },
        };
        // keyed by the frame's index (known without decoding): a frame the pool holds isn't decoded again
        let idx = dec.index_at(t);
        let (dw, dh) = (dec.info().width, dec.info().height);
        let pooled = idx.map(|i| (src.clone(), i as i64)).filter(|k| book.resident.contains_key(k));
        let key: PoolKey = match pooled {
            Some(k) => {
                book.resident.get_mut(&k).unwrap().1 = tick;
                book.hits += 1;
                k
            }
            None => {
                let frame = match dec.frame_nv12_at(t) {
                    Ok(fr) => fr,
                    Err(x) => {
                        err.0 = Some(format!("clip {} at {t:.3}s: {x}", row.id));
                        return;
                    }
                };
                let key = (src.clone(), idx.map(|i| i as i64).unwrap_or((frame.pts * 1e6).round() as i64 + (1 << 40)));
                let n = frame.rgba.len();
                book.resident.insert(key.clone(), (n, tick));
                book.bytes += n;
                book.uploads += 1;
                gf.uploads.push((key.clone(), std::sync::Arc::new(frame.rgba)));
                key
            }
        };
        // cover the box, zoom about the centre, pan within the slack; all as 16.16 maps from output to source pixels
        let frame = (dw, dh);
        let (sw, sh) = (frame.0 as f64, frame.1 as f64);
        let (x, y, w, h) = (num(v, "x", 0.0) as f64, num(v, "y", 0.0) as f64, num(v, "w", fw as f32) as f64, num(v, "h", fh as f32) as f64);
        let zoom = (num(v, "zoom", 1.0) as f64).max(0.01);
        let (cx, cy, cw, ch) = cover(sw, sh, w, h, zoom, (num(v, "pan_x", 0.0) as f64, num(v, "pan_y", 0.0) as f64));
        let fx = |v: f64| (v * 65536.0).round() as i32;
        let s = raea_core_k::Slot {
            on: 1,
            bx: x.round() as i32,
            by: y.round() as i32,
            ex: (x + w).round() as i32,
            ey: (y + h).round() as i32,
            sw: frame.0 as i32,
            sh: frame.1 as i32,
            cx: fx(cx - cw / 2.0),
            cy: fx(cy - ch / 2.0),
            dx: fx(cw / w),
            dy: fx(ch / h),
            al: (opacity.min(1.0) * 256.0).round() as i32,
        };
        if gf.slots.len() == 4 {
            err.0 = Some(format!("frame {f}: more than four clips at once (the composite kernel has four slots)"));
            return;
        }
        masks.push(raea_runtime_view::masks::mask_of(v));
        gf.slots.push((key, s));
    }
    // clips with a mask (a solid in space and time) show its slice through this frame
    if masks.iter().any(Option::is_some) {
        let mut sl: Vec<raea_core_k::Slot> = gf.slots.iter().map(|x| x.1).collect();
        match raea_runtime_view::masks::slot_masks(&masks, &mut sl, f, fw as usize, fh as usize, edit.prog.frames) {
            Ok(m) => {
                for (slot, s) in gf.slots.iter_mut().zip(sl) {
                    slot.1 = s;
                }
                gf.mask = m.map(std::sync::Arc::new);
            }
            Err(e) => {
                err.0 = Some(e);
                return;
            }
        }
    }
    // over budget: the least recently used frames not drawn now go
    if book.bytes > raea_runtime_view::pool_budget() {
        let mut by_age: Vec<(PoolKey, u64, usize)> = book.resident.iter().filter(|(_, (_, t))| *t != tick).map(|(k, (n, t))| (k.clone(), *t, *n)).collect();
        by_age.sort_by_key(|x| x.1);
        for (k, _, n) in by_age {
            if book.bytes <= raea_runtime_view::pool_budget() {
                break;
            }
            book.resident.remove(&k);
            book.bytes -= n;
            gf.evict.push(k);
        }
    }
    gf.on = true;
    st.clips += t_clips.elapsed().as_nanos() as u64;
}
