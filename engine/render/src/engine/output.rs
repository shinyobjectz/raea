//! Frames out of an Engine: stepped, read back, checked for black, in look runs, to RGBA.

use super::*;

impl Engine {
    #[cfg(feature = "native")]
    pub(crate) fn tag_at(&self, d: &[u8]) -> Option<(u8, u8, u8)> {
        if self.nv12 {
            // the last word is the tag texel
            let n = nv12_words(self.w, self.h) * 4;
            return (d.len() >= n).then(|| (d[n - 4], d[n - 3], d[n - 2]));
        }
        let (w, h) = (self.w as usize, self.h as usize);
        let padded = (w * 4).div_ceil(256) * 256;
        let at = if self.packed { packed_tag(self.w, self.h) } else { padded * h + 8 * 4 };
        (d.len() >= at + 3).then(|| (d[at], d[at + 1], d[at + 2]))
    }

    /// One app.update(), timed; with RAEA_SERIAL set, then wait for the GPU and time the wait (what the GPU still
    /// had to do once the CPU had submitted: its un-overlapped cost).
    #[cfg(feature = "native")]
    pub(crate) fn step(&mut self) {
        self.pre_step();
        let t = std::time::Instant::now();
        plugins::arm(self.app.world_mut());
        self.pipe.update(&mut self.app); raea_io_trace::count("engine/updates", 1);
        let dt = t.elapsed().as_nanos() as u64;
        let mut wait = 0;
        if self.serial {
            let t = std::time::Instant::now();
            let dev = self.app.sub_app(bevy::render::RenderApp).world().resource::<bevy::render::renderer::RenderDevice>().clone();
            let _ = dev.wgpu_device().poll(bevy::render::render_resource::PollType::wait_indefinitely());
            wait = t.elapsed().as_nanos() as u64;
            // RAEA_GPU_TIMES: this frame's timestamps (outside the timed wait)
            let period = self.app.sub_app(bevy::render::RenderApp).world().resource::<bevy::render::renderer::RenderQueue>().get_timestamp_period() as f64;
            let rw = self.app.sub_app_mut(bevy::render::RenderApp).world_mut();
            if let Some(mut pool) = rw.get_resource_mut::<PoolGpu>() {
                if pool.ts_pending {
                    pool.ts_pending = false;
                    let m = pool.ts.as_ref().unwrap().2.clone();
                    m.slice(..).map_async(wgpu::MapMode::Read, |_| {});
                    let _ = dev.wgpu_device().poll(bevy::render::render_resource::PollType::wait_indefinitely());
                    let t: Vec<u64> = bytemuck::cast_slice(&m.slice(..).get_mapped_range().expect("a mapped buffer")).to_vec();
                    m.unmap();
                    let ms = |a: usize, b: usize| t[b].saturating_sub(t[a]) as f64 * period / 1e6;
                    let up: f64 = (0..pool.ts_uploads as usize).map(|k| ms(8 + 2 * k, 9 + 2 * k)).sum();
                    let ov = if pool.ts_overlay { ms(6, 7) } else { 0.0 };
                    let (c, n) = (ms(2, 3), ms(4, 5));
                    pool.ts_ms[0] += up;
                    pool.ts_ms[1] += ov;
                    pool.ts_ms[2] += c;
                    pool.ts_ms[3] += n;
                    pool.ts_frames += 1;
                }
            }
        }
        let mut st = self.app.world_mut().resource_mut::<Stages>();
        st.update += dt;
        st.gpu_wait += wait;
        st.updates += 1;
    }

    /// A frame as straight RGBA, whichever way it came back.
    #[cfg(feature = "native")]
    pub fn to_rgba(&self, px: &[u8]) -> Vec<u8> {
        if self.nv12 { raea_io_media::yuv::nv12_to_rgba(px, self.w as usize, self.h as usize) } else { px.to_vec() }
    }

    /// RAEA_GPU_TIMES with RAEA_SERIAL: GPU ms per timed frame for the clip uploads, the overlay's upload,
    /// composite.k and nv12buf.k, and the frames timed.
    #[cfg(feature = "native")]
    pub fn gpu_stage_times(&self) -> Option<([f64; 4], u64)> {
        let p = self.app.sub_app(bevy::render::RenderApp).world().get_resource::<PoolGpu>()?;
        (p.ts_frames > 0).then(|| (p.ts_ms.map(|v| v / p.ts_frames as f64), p.ts_frames))
    }

    /// GPU pool: (frames uploaded, frames reused, bytes held).
    #[cfg(feature = "native")]
    pub fn pool_stats(&self) -> (u64, u64, usize) {
        self.app.world().get_resource::<PoolBook>().map(|b| (b.uploads, b.hits, b.bytes)).unwrap_or_default()
    }

    /// The stage times so far.
    pub fn stages(&self) -> Stages {
        self.app.world().resource::<Stages>().clone()
    }

    #[cfg(feature = "native")]
    pub(crate) fn unpad(&self, d: &[u8]) -> Vec<u8> {
        if self.nv12 {
            // NV12: the luma plane then the interleaved chroma plane, no padding (the tag word dropped)
            return d[..(self.w * self.h * 3 / 2) as usize].to_vec();
        }
        let (w, h) = (self.w as usize, self.h as usize);
        let (row_bytes, padded) = (w * 4, (w * 4).div_ceil(256) * 256);
        let px: Vec<u8> = if self.packed { d[..w * h * 4].to_vec() } else { d.chunks(padded).take(h).flat_map(|r| &r[..row_bytes]).copied().collect() };
        if self.ss > 1 { downsample(&px, w, h, self.ss as usize) } else { px }
    }

    /// One frame, straight RGBA8 (opaque), the careful way: two tagged readbacks in a row that agree (the first frame
    /// after a load, when meshes, materials and the overlay are new to the GPU).
    #[cfg(feature = "native")]
    pub fn frame(&mut self, f: i64) -> Result<Vec<u8>, String> {
        self.catch_up(f)?;
        self.app.world_mut().resource_mut::<Clock>().0 = f;
        raea_gpu_device::check::panic_fault(f);
        let want = tag_colour(f, self.app.world().resource::<Gen>().0);
        let mut prev: Option<Vec<u8>> = None;
        for i in 0..240 {
            if raea_io_trace::stat("RAEA_TRACE") {
                eprintln!("trace frame {f} update {i}");
            }
            self.step();
            self.app.world_mut().resource_mut::<Error>().0.take().map_or(Ok(()), Err)?;
            let got = std::mem::take(&mut self.app.world_mut().resource_mut::<Out>().0);
            // the first frame after a load: readbacks the warm-up queued carry its tag but can predate parts that take a
            // few updates to show (a spot light's first frames drew without it), so the first few updates' are dropped
            if !self.warm && i < 4 {
                // and the GPU waited for, so every readback queued before this frame (the warm-up's, which can still
                // be on its way when the frame is large or heavy: supersampled, traced) has landed and is dropped here
                // rather than taken for this frame later
                raea_sys_frame::pipe::wait_gpu(&self.app);
                continue;
            }
            for d in got {
                if self.tag_at(&d) == Some(want) {
                    // two in a row that agree: exactly, or after NEAR_AFTER updates within a few bytes' noise. A scene
                    // can draw a few bytes differently every update (the paint look's hill on intro/hill.tsx: 40-110
                    // bytes of 3.1 M, by 1-12, around the horizon, on every one of 240 updates), and holding out for an
                    // exact pair failed the frame: `--at 5,40,80` lost its second frame, which starts a new look and so
                    // is taken this way rather than pipelined (2026-10-22)
                    let settled = prev.as_deref().is_some_and(|p| p == &d[..] || (i >= NEAR_AFTER && nearly_same(p, &d)));
                    if settled {
                        if !self.warm {
                            mem_mark(&format!("engine: first frame back ({} updates)", i + 1));
                        }
                        self.warm = true;
                        return Ok(self.unpad(&d));
                    }
                    prev = Some(d);
                }
            }
        }
        Err(format!("frame {f}: no two readbacks with its tag agreed in 240 updates"))
    }

    /// A frame that came out all black (RGBA, not NV12) drawn once more the careful way, every page drawn again with
    /// Vello's own bump buffers (raea-page `redraw_full`): a Vello overflow or a GPU fault (wgpu-hal's Metal backend
    /// doesn't report a command buffer that ended in error) draws a frame black and says nothing. Black again, the
    /// GPU's canary says whether it's the scene's own black (kept) or a GPU that's dropping work (refused). The page
    /// render does the same (raea-gpu-device check.rs).
    #[cfg(feature = "native")]
    pub(crate) fn black_checked(&mut self, f: i64, px: Vec<u8>) -> Result<Vec<u8>, String> {
        if self.nv12 {
            if let Some(why) = raea_gpu_device::check::refused() {
                return Err(format!("frame {f}: {why}"));
            }
            return raea_gpu_device::check::skip_checked(f, px, raea_sys_page::redraw_full, || self.frame(f));
        }
        let (px, black) = raea_gpu_device::check::black_checked(f, px, raea_sys_page::redraw_full, || self.frame(f))?;
        if black {
            raea_gpu_device::check::black_twice(f, self.canary())?;
        }
        Ok(px)
    }

    /// Frames in order, pipelined: frame k+1 is drawn while frame k's readback is on its way back, and each readback is
    /// matched to its frame by its tag, so the output is the same frames `frame` makes, one update per frame.
    #[cfg(feature = "native")]
    pub fn frames(&mut self, list: &[i64], mut sink: impl FnMut(i64, Vec<u8>) -> Result<(), String>) -> Result<(), String> {
        // a frame whose look comes on, changes or goes (frame_look_i, FrameRows::vals) has new materials and cells to
        // make, which a pipelined update can read back before they draw: each run of one look starts the careful way
        let runs = raea_sys_frame::tables::runs(&self.app.world().resource::<Edit>().prog, list);
        let many = runs.len() > 1;
        for r in runs {
            if many {
                self.warm = false;
            }
            // a run after a cut warms up as a load does (8 updates): a new effect can take more updates to come than the
            // careful way's 4 (the sky's light on the scene came 2 frames late at RR0's sunset cut, RR1)
            if many && r.first() != list.first() {
                self.catch_up(r[0])?;
                self.app.world_mut().resource_mut::<Clock>().0 = r[0];
                (0..8).for_each(|_| self.step());
            }
            // a panic drawing a frame an error naming the newest frame sent, the one drawing (#33)
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| self.frames_run(r, &mut sink))).map_err(|p| raea_gpu_device::check::panicked(p, self.app.world().resource::<Clock>().0))??;
        }
        Ok(())
    }

    #[cfg(feature = "native")]
    pub(crate) fn frames_run(&mut self, list: &[i64], sink: &mut dyn FnMut(i64, Vec<u8>) -> Result<(), String>) -> Result<(), String> {
        let mut start = 0;
        if !self.warm {
            let Some(&f0) = list.first() else { return Ok(()) };
            let px = self.frame(f0)?;
            sink(f0, self.black_checked(f0, px)?)?;
            start = 1;
        }
        let generation = self.app.world().resource::<Gen>().0;
        let mut got: HashMap<(u8, u8, u8), Vec<u8>> = HashMap::new();
        let (mut next, mut sent, mut idle) = (start, start, 0);
        while next < list.len() {
            if sent < list.len() && sent < next + 3 {
                self.catch_up(list[sent])?;
                self.app.world_mut().resource_mut::<Clock>().0 = list[sent];
                raea_gpu_device::check::panic_fault(list[sent]);
                sent += 1;
            }
            if raea_io_trace::stat("RAEA_TRACE") {
                eprintln!("trace pipelined next {} sent {sent}", list[next]);
            }
            self.step();
            if idle >= 3 && !self.serial {
                // every frame in flight is sent and none has come back: the GPU is behind. Wait for it rather than
                // queueing more updates (each one more frame of GPU work), so a heavy pass (traced AO) slows the
                // render down instead of starving its readbacks into the careful path
                raea_sys_frame::pipe::wait_gpu(&self.app);
            }
            self.app.world_mut().resource_mut::<Error>().0.take().map_or(Ok(()), Err)?;
            for d in std::mem::take(&mut self.app.world_mut().resource_mut::<Out>().0) {
                // only this load's frames (a motion-blur pre-step's carry generation 0)
                if let Some(t) = self.tag_at(&d).filter(|t| t.2 == generation) {
                    got.insert(t, d);
                }
            }
            idle += 1;
            while next < sent {
                let Some(d) = got.remove(&tag_colour(list[next], generation)) else { break };
                let t = std::time::Instant::now();
                let px = self.unpad(&d);
                {
                    let mut st = self.app.world_mut().resource_mut::<Stages>();
                    st.readback_copy += t.elapsed().as_nanos() as u64;
                    st.frames += 1;
                }
                let px = self.black_checked(list[next], px)?;
                sink(list[next], px)?;
                if next % mark_every() == 0 {
                    mem_mark(&format!("engine: frame {} sent on", list[next]));
                }
                next += 1;
                idle = 0;
            }
            if idle > 30 {
                // a readback went missing: that frame the careful way, then carry on
                let f = list[next];
                let px = self.frame(f)?;
                sink(f, self.black_checked(f, px)?)?;
                next += 1;
                sent = sent.max(next);
                idle = 0;
            }
        }
        Ok(())
    }
}
