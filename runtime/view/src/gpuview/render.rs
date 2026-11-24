//! The direct render: frames in order made NV12 (or P010, or kept RGBA) on the device and read back three in flight,
//! from rows or fed in.

use super::*;

impl GpuView {
    /// The direct render, for edits without 3D: the frames in order, each composited from the pool (as `frame`), made
    /// NV12 by nv12buf.k on this device and read back with three frames in flight, so the CPU works on the next frames
    /// (rules, decode, overlay) while the GPU and the copy finish earlier ones. The sink gets NV12 as the engine's GPU
    /// path gives it (luma, then interleaved chroma; no tag). No Bevy schedules run.
    pub fn render_nv12(&mut self, list: &[i64], sink: impl FnMut(i64, Vec<u8>) -> Result<(), String>) -> Result<DirectTimes, String> {
        self.render_nv12_with(list, None, sink)
    }

    /// As `render_nv12`, each frame's overlay handed over by `feed` as a page already on this device (premultiplied
    /// RGBA8 packed, on the device this view runs on): a page goes from the paint to the
    /// composite with no readback, no file and no upload. `feed` is asked for the frames in order, each just before
    /// its composite, so a page buffer it reuses is overwritten only after the frame before read it (one queue).
    pub fn render_nv12_fed(&mut self, list: &[i64], mut feed: impl FnMut(i64) -> Result<wgpu::Buffer, String>, sink: impl FnMut(i64, Vec<u8>) -> Result<(), String>) -> Result<DirectTimes, String> {
        self.render_nv12_with(list, Some(&mut feed), sink)
    }

    pub(crate) fn render_nv12_with(&mut self, list: &[i64], mut feed: Option<&mut dyn FnMut(i64) -> Result<wgpu::Buffer, String>>, mut sink: impl FnMut(i64, Vec<u8>) -> Result<(), String>) -> Result<DirectTimes, String> {
        use std::sync::atomic::{AtomicBool, Ordering};
        use std::sync::Arc;
        const RING: usize = 3;
        let (w, h) = (self.w, self.h);
        // RGBA: the composite itself (`rgba`); P010: two bytes a sample (a luma word holds two samples, a chroma word one
        // Cb Cr pair)
        let (words, bytes) = if self.rgba {
            ((w * h) as usize, (w * h * 4) as usize)
        } else if self.ten_bit {
            ((w * h / 2 + w * h / 4 + 1) as usize, (w * h * 3) as usize)
        } else {
            ((w * h / 4 + w * h / 8 + 1) as usize, (w * h * 3 / 2) as usize)
        };
        if self.nv12.is_none() && !self.rgba {
            let k = raea_core_k::parse(if self.ten_bit { P010BUF_K } else { NV12BUF_K })?;
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
        let timing = raea_io_trace::stat("RAEA_GPU_TIMES") && self.gpu.device.features().contains(wgpu::Features::TIMESTAMP_QUERY | wgpu::Features::TIMESTAMP_QUERY_INSIDE_ENCODERS);
        if raea_io_trace::stat("RAEA_GPU_TIMES") && !timing {
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
            let px = ring[slot].slice(..).get_mapped_range().expect("a mapped buffer")[..bytes].to_vec();
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
            // the frame's page and overlay, a panic in them an error naming the frame (#33); on any failure the frames
            // in flight reach the sink first, so the render keeps every frame drawn before this one
            let drawn = raea_gpu_device::check::frame_caught(f, || {
                self.ts_uploads = 0;
                if let Some(feed) = feed.as_mut() {
                    let t = Instant::now();
                    self.fed = Some(feed(f)?);
                    dt.fed += t.elapsed().as_secs_f64() * 1e3;
                }
                self.frame_wait(f, false)
            });
            let ft = match drawn {
                Ok(ft) => ft,
                Err(e) => {
                    while !flight.is_empty() {
                        drain(self, &mut flight, &mut dt)?;
                    }
                    return Err(e);
                }
            };
            dt.rules += ft.rules;
            dt.overlay += ft.overlay;
            dt.decode_upload += ft.decode_upload;
            dt.uploads += ft.uploads;
            let t = Instant::now();
            let src = match self.nv12.as_ref().filter(|_| !self.rgba) {
                Some((c, nb)) => {
                    self.gpu.dispatch_ts(c, &[&self.out], &[&self.bg_view], &[nb], words, false, self.ts.as_ref().map(|q| (q, 4)));
                    nb
                }
                None => &self.out,
            };
            let slot = n % RING;
            let mut enc = self.gpu.device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("raea-render frame") });
            if let Some(q) = self.ts.as_ref() {
                enc.write_timestamp(q, 6);
            }
            enc.copy_buffer_to_buffer(src, 0, &ring[slot], 0, (words * 4) as u64);
            if let Some(q) = self.ts.as_ref() {
                enc.write_timestamp(q, 7);
                enc.resolve_query_set(q, 0..16, &resolve, 0);
            }
            let sub = self.gpu.queue.submit([enc.finish()]);
            raea_gpu_device::submitted(&self.gpu.queue, "render", "raea-render frame");
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
