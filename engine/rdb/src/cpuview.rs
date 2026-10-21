//! The direct render with no GPU (docs/cpu.feature C1): the same frames as gpuview's, made on the CPU. The 2D overlay
//! is painted by Blitz on vello_cpu (as gpuview's is), clips are decoded to NV12 by the media backend, and the GPU's
//! kernels run as their native twins: kgpu's composite_cpu (bit-equal to composite.k) and nv12_cpu (bit-equal
//! to nv12buf.k). Used when RAEA_CPU is set or no GPU adapter is found.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;

use bevy::ecs::entity::Entity;

use crate::eval::Tup;
use crate::ir::{Program, V};

fn num(v: &std::collections::BTreeMap<String, V>, k: &str, d: f64) -> f64 {
    match v.get(k) {
        Some(V::N(x)) => *x,
        _ => d,
    }
}

/// Where a CPU render's time went, in ms per frame.
#[derive(Default, Debug)]
pub struct CpuTimes {
    pub frames: usize,
    pub rules: f64,
    pub overlay: f64,
    pub decode: f64,
    pub composite: f64,
    pub nv12: f64,
    pub sink: f64,
    pub overlays_painted: usize,
    /// decode-ahead workers (0: decoded in the render's own loop)
    pub decode_ahead: usize,
}

impl CpuTimes {
    pub fn json(&self) -> serde_json::Value {
        let n = self.frames.max(1) as f64;
        serde_json::json!({"cpu": true, "frames": self.frames, "overlays_painted": self.overlays_painted, "decode_ahead": self.decode_ahead,
            "ms_per_frame": {"rules": self.rules / n, "overlay": self.overlay / n, "decode": self.decode / n,
            "composite": self.composite / n, "nv12": self.nv12 / n, "sink": self.sink / n}})
    }
}

/// Decoding ahead in parallel (the buffering for a CPU render): the frames a render will need, grouped by the keyframe
/// that starts their group of pictures, are decoded by worker threads, each with its own decoders, no further than
/// `ahead` frames past the render, into a shared map the render waits on. Each group decodes from its own keyframe,
/// so the frames equal one decoder's. Only for backends that know their keyframes (raea-media's soft decoder).
struct Job {
    src: String,
    path: PathBuf,
    first: usize,
    frames: Vec<(usize, f64)>,
}

#[derive(Default)]
struct AheadState {
    next: usize,
    consumed: usize,
    frames: HashMap<(String, usize), Arc<Vec<u8>>>,
    err: Option<String>,
    stop: bool,
}

struct Ahead {
    jobs: Vec<Job>,
    state: std::sync::Mutex<AheadState>,
    cv: std::sync::Condvar,
    window: usize,
    /// at most this many decoded frames held (each about w x h x 1.5 bytes: 3 MB at 1080x1920), except frames the
    /// render needs within the next few, so the next frame always gets through
    cap: usize,
    first_use: HashMap<(String, usize), usize>,
}

impl Ahead {
    fn work(&self, backend: &str) {
        let Ok(b) = raea_media::backend(backend) else { return };
        let mut decs: HashMap<String, Box<dyn raea_media::VideoDecoder>> = HashMap::new();
        loop {
            let j = {
                let mut s = self.state.lock().unwrap();
                loop {
                    if s.stop || s.err.is_some() || s.next >= self.jobs.len() {
                        return;
                    }
                    if self.jobs[s.next].first <= s.consumed + self.window {
                        s.next += 1;
                        break s.next - 1;
                    }
                    s = self.cv.wait(s).unwrap();
                }
            };
            let job = &self.jobs[j];
            for &(idx, at) in &job.frames {
                {
                    // the bound on memory: wait while the map is full, unless this frame is needed soon
                    let soon = self.first_use.get(&(job.src.clone(), idx)).copied().unwrap_or(0);
                    let mut s = self.state.lock().unwrap();
                    while !s.stop && s.err.is_none() && s.frames.len() >= self.cap && soon > s.consumed + 8 {
                        s = self.cv.wait(s).unwrap();
                    }
                    if s.stop || s.err.is_some() {
                        return;
                    }
                }
                let dec = match decs.entry(job.src.clone()) {
                    std::collections::hash_map::Entry::Occupied(e) => e.into_mut(),
                    std::collections::hash_map::Entry::Vacant(e) => match b.open_video(&job.path) {
                        Ok(d) => e.insert(d),
                        Err(err) => {
                            self.state.lock().unwrap().err = Some(err);
                            self.cv.notify_all();
                            return;
                        }
                    },
                };
                match dec.frame_nv12_at(at) {
                    Ok(fr) => {
                        self.state.lock().unwrap().frames.insert((job.src.clone(), idx), Arc::new(fr.rgba));
                    }
                    Err(err) => {
                        self.state.lock().unwrap().err = Some(err);
                    }
                }
                self.cv.notify_all();
            }
        }
    }

    fn get(&self, src: &str, idx: usize) -> Result<Arc<Vec<u8>>, String> {
        let key = (src.to_string(), idx);
        let mut s = self.state.lock().unwrap();
        loop {
            if let Some(p) = s.frames.get(&key) {
                return Ok(p.clone());
            }
            if let Some(e) = &s.err {
                return Err(e.clone());
            }
            s = self.cv.wait(s).unwrap();
        }
    }

    /// The render is past position `pos`: frames last used by then go, and workers may start further ahead.
    fn advance(&self, pos: usize, last_use: &HashMap<(String, usize), usize>) {
        let mut s = self.state.lock().unwrap();
        s.consumed = pos;
        s.frames.retain(|k, _| last_use.get(k).is_some_and(|l| *l > pos));
        self.cv.notify_all();
    }

    fn stop(&self) {
        self.state.lock().unwrap().stop = true;
        self.cv.notify_all();
    }
}

pub struct CpuView {
    prog: Program,
    rows: Vec<Tup>,
    dir: PathBuf,
    pub w: u32,
    pub h: u32,
    tl: Option<Arc<crate::timeline::Timeline>>,
    rw: raea::world::RowWorld,
    ents: HashMap<String, Entity>,
    bg: Vec<u32>,
    backend: Box<dyn raea_media::Backend>,
    decoders: HashMap<(String, usize), Box<dyn raea_media::VideoDecoder>>,
    /// decoded NV12 frames by (source, frame index), the last few kept
    pool: HashMap<(String, i64), Arc<Vec<u8>>>,
    overlays: HashMap<u64, Arc<Vec<u32>>>,
    threads: usize,
    /// render_nv12 hands P010 (kgpu's p010_cpu, bit-equal to p010buf.k) instead of NV12 (RenderOpts.ten_bit)
    pub ten_bit: bool,
}

impl CpuView {
    pub fn new(prog: &Program, rows: &[Tup], dir: &Path) -> Result<CpuView, String> {
        let (w, h) = (prog.width, prog.height);
        // the 2D rows, as the engine installs them (as gpuview does)
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
        // the background: the camera's clear colour
        let bg = rows.iter().find(|t| t.kind == "camera").and_then(|t| match t.vals.get("bg") {
            Some(V::S(s)) => Some(s.clone()),
            _ => None,
        });
        let rgb = bg
            .and_then(|s| {
                let hx = s.trim_start_matches('#').to_string();
                let b = |i: usize| u8::from_str_radix(hx.get(i..i + 2)?, 16).ok();
                Some([b(0)?, b(2)?, b(4)?])
            })
            .unwrap_or([13, 10, 26]);
        let word = rgb[0] as u32 | (rgb[1] as u32) << 8 | (rgb[2] as u32) << 16 | 255 << 24;
        let threads = std::env::var("RAEA_CPU_THREADS").ok().and_then(|t| t.parse().ok())
            .unwrap_or_else(|| std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4));
        Ok(CpuView {
            prog: prog.clone(),
            rows: rows.to_vec(),
            dir: dir.to_path_buf(),
            w,
            h,
            tl: None,
            rw,
            ents,
            bg: vec![word; (w * h) as usize],
            backend: raea_media::backend(&crate::engine::media_backend())?,
            decoders: HashMap::new(),
            pool: HashMap::new(),
            overlays: HashMap::new(),
            threads,
            ten_bit: false,
        })
    }

    pub fn share_timeline(&mut self, tl: Arc<crate::timeline::Timeline>) {
        self.tl = Some(tl);
    }

    fn overlay(&mut self, st: &[Tup], f: i64, t: &mut CpuTimes) -> Arc<Vec<u32>> {
        let dg = crate::gpuview::GpuView::overlay_digest(st, f);
        if let Some(o) = self.overlays.get(&dg) {
            return o.clone();
        }
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
        let mut px = Vec::new();
        self.rw.render(f, &mut px);
        let words: Vec<u32> = px.chunks_exact(4).map(|c| u32::from_le_bytes([c[0], c[1], c[2], c[3]])).collect();
        t.overlays_painted += 1;
        if self.overlays.len() >= 32 {
            self.overlays.clear();
        }
        let o = Arc::new(words);
        self.overlays.insert(dg, o.clone());
        o
    }

    /// The frames of `list` as NV12 (w x h x 3/2 bytes each), in order, to `sink`: what gpuview's render_nv12 gives.
    pub fn render_nv12(&mut self, list: &[i64], mut sink: impl FnMut(i64, Vec<u8>) -> Result<(), String>) -> Result<CpuTimes, String> {
        let mut tm = CpuTimes::default();
        if self.tl.is_none() {
            self.tl = Some(Arc::new(crate::timeline::Timeline::new(&self.prog, &self.rows)?));
        }
        // the plan: each frame's state and its clips' sources (src, slot, time, frame index, display size)
        let t0 = Instant::now();
        let fps = self.prog.fps;
        let mut plan: Vec<(Vec<Tup>, Vec<(Tup, String, usize, f64, Option<usize>, u32, u32)>)> = Vec::with_capacity(list.len());
        for &f in list {
            let st = self.tl.as_ref().unwrap().tups(f);
            let mut clips: Vec<Tup> = st.iter().filter(|t| t.kind == "clip").cloned().collect();
            clips.sort_by(|a, b| num(&a.vals, "z", 0.0).total_cmp(&num(&b.vals, "z", 0.0)).then(a.id.cmp(&b.id)));
            let mut slots_of: HashMap<String, usize> = HashMap::new();
            let mut need = Vec::new();
            for c in clips {
                let v = &c.vals;
                let from = num(v, "from", 0.0) as i64;
                let to = v.get("to").map(|x| x.num() as i64).unwrap_or(i64::MAX);
                if f < from || f >= to || num(v, "opacity", 1.0) as f32 as f64 <= 0.0 {
                    continue;
                }
                let Some(V::S(src)) = v.get("src").cloned() else { return Err(format!("clip {}: no src", c.id)) };
                let slot = {
                    let s = slots_of.entry(src.clone()).or_insert(0);
                    *s += 1;
                    *s - 1
                };
                let at = num(v, "src_in", 0.0) + (f - from) as f64 / fps;
                if !self.decoders.contains_key(&(src.clone(), slot)) {
                    let p = crate::engine::resolve_src(&self.rows, &src, &self.dir)?;
                    let d = self.backend.open_video(&p)?;
                    self.decoders.insert((src.clone(), slot), d);
                }
                let dec = self.decoders.get_mut(&(src.clone(), slot)).unwrap();
                let (idx, dw, dh) = (dec.index_at(at), dec.info().width, dec.info().height);
                need.push((c.clone(), src, slot, at, idx, dw, dh));
            }
            plan.push((st, need));
        }
        tm.rules += t0.elapsed().as_secs_f64() * 1e3;
        // decoding ahead in parallel, where every source knows its keyframes (RAEA_DECODE_AHEAD=0 turns it off)
        let mut keyframes: HashMap<String, Vec<usize>> = HashMap::new();
        for ((src, _), d) in &self.decoders {
            if let Some(k) = d.keyframes() {
                keyframes.insert(src.clone(), k);
            }
        }
        let all_known = plan.iter().all(|(_, n)| n.iter().all(|x| x.4.is_some() && keyframes.contains_key(&x.1)));
        let workers = std::env::var("RAEA_DECODE_AHEAD").ok().and_then(|v| v.parse::<usize>().ok())
            .unwrap_or_else(|| (std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4) / 2).max(1));
        let mut last_use: HashMap<(String, usize), usize> = HashMap::new();
        let ahead = if all_known && workers > 0 && plan.iter().any(|(_, n)| !n.is_empty()) {
            // jobs: (src, the group's keyframe) -> its frames, in order of first use
            let mut groups: HashMap<(String, usize), (usize, Vec<(usize, f64)>)> = HashMap::new();
            for (pos, (_, need)) in plan.iter().enumerate() {
                for (_, src, _, at, idx, _, _) in need {
                    let idx = idx.unwrap();
                    last_use.insert((src.clone(), idx), pos);
                    let ks = &keyframes[src];
                    let g = ks.partition_point(|k| *k <= idx).saturating_sub(1);
                    let e = groups.entry((src.clone(), ks.get(g).copied().unwrap_or(0))).or_insert((pos, Vec::new()));
                    if !e.1.iter().any(|x| x.0 == idx) {
                        e.1.push((idx, *at));
                    }
                }
            }
            let mut jobs: Vec<Job> = groups
                .into_iter()
                .map(|((src, _), (first, mut frames))| {
                    frames.sort_by_key(|x| x.0);
                    let path = crate::engine::resolve_src(&self.rows, &src, &self.dir).unwrap_or_default();
                    Job { src, path, first, frames }
                })
                .collect();
            jobs.sort_by_key(|j| j.first);
            let window = std::env::var("RAEA_DECODE_WINDOW").ok().and_then(|v| v.parse().ok()).unwrap_or(90);
            let mut first_use: HashMap<(String, usize), usize> = HashMap::new();
            for (pos, (_, need)) in plan.iter().enumerate() {
                for (_, src, _, _, idx, _, _) in need {
                    first_use.entry((src.clone(), idx.unwrap())).or_insert(pos);
                }
            }
            let cap = std::env::var("RAEA_DECODE_CAP").ok().and_then(|v| v.parse().ok()).unwrap_or(48);
            Some(Ahead { jobs, state: Default::default(), cv: std::sync::Condvar::new(), window, cap, first_use })
        } else {
            None
        };
        let backend_name = crate::engine::media_backend();
        let (w, h) = (self.w as usize, self.h as usize);
        let dummy = vec![0u8; 24];
        let r: Result<(), String> = std::thread::scope(|sc| {
            if let Some(a) = ahead.as_ref() {
                for _ in 0..workers {
                    let bn = backend_name.clone();
                    sc.spawn(move || a.work(&bn));
                }
            }
            let out = (|| -> Result<(), String> {
                for (pos, (&f, (st, need))) in list.iter().zip(plan.iter()).enumerate() {
                    let t = Instant::now();
                    let overlay = self.overlay(st, f, &mut tm);
                    tm.overlay += t.elapsed().as_secs_f64() * 1e3;
                    let t = Instant::now();
                    let (fw, fh) = (w as f64, h as f64);
                    let mut slots: Vec<(Arc<Vec<u8>>, raea_kgpu::Slot)> = Vec::new();
                    let mut masks: Vec<Option<String>> = Vec::new();
                    let mut used: Vec<(String, i64)> = Vec::new();
                    for (c, src, slot, at, idx, dw, dh) in need {
                        let v = &c.vals;
                        masks.push(crate::shape::mask_of(v));
                        let opacity = num(v, "opacity", 1.0) as f32 as f64;
                        let planes = match ahead.as_ref() {
                            Some(a) => a.get(src, idx.unwrap())?,
                            None => {
                                let key = idx.map(|i| (src.clone(), i as i64));
                                match key.as_ref().and_then(|k| self.pool.get(k)) {
                                    Some(p) => p.clone(),
                                    None => {
                                        let dec = self.decoders.get_mut(&(src.clone(), *slot)).unwrap();
                                        let fr = dec.frame_nv12_at(*at)?;
                                        let p = Arc::new(fr.rgba);
                                        let k = key.clone().unwrap_or((src.clone(), (fr.pts * 1e6).round() as i64 + (1 << 40)));
                                        self.pool.insert(k, p.clone());
                                        p
                                    }
                                }
                            }
                        };
                        if let Some(i) = idx {
                            used.push((src.clone(), *i as i64));
                        }
                        let (dw, dh) = (*dw, *dh);
                let (sw, sh) = (dw as f64, dh as f64);
                let n32 = |k: &str, d: f64| num(v, k, d) as f32 as f64;
                let (x, y, bw, bh) = (n32("x", 0.0), n32("y", 0.0), n32("w", fw), n32("h", fh));
                let zoom = n32("zoom", 1.0).max(0.01);
                let (cx, cy, cw, ch) = crate::engine::cover(sw, sh, bw, bh, zoom, (n32("pan_x", 0.0), n32("pan_y", 0.0)));
                let q = |v: f64| (v * 65536.0).round() as i32;
                slots.push((
                    planes,
                    raea_kgpu::Slot {
                        on: 1,
                        bx: x.round() as i32,
                        by: y.round() as i32,
                        ex: (x + bw).round() as i32,
                        ey: (y + bh).round() as i32,
                        sw: dw as i32,
                        sh: dh as i32,
                        cx: q(cx - cw / 2.0),
                        cy: q(cy - ch / 2.0),
                        dx: q(cw / bw),
                        dy: q(ch / bh),
                        al: (opacity.min(1.0) * 256.0).round() as i32,
                    },
                ));
            }
            if slots.len() > 4 {
                return Err(format!("frame {f}: more than four clips at once"));
            }
            // frames not shown now go (a render moves forward; a pool of the frame's own clips is enough)
            self.pool.retain(|k, _| used.contains(k));
            tm.decode += t.elapsed().as_secs_f64() * 1e3;
            let t = Instant::now();
            let mut sl: Vec<raea_kgpu::Slot> = (0..4).map(|s| slots.get(s).map(|x| x.1).unwrap_or_else(raea_kgpu::Slot::off)).collect();
            let pl: Vec<&[u8]> = (0..4).map(|s| slots.get(s).map(|x| x.0.as_slice()).unwrap_or(&dummy[..])).collect();
            // clips with a mask (a solid in space and time) show only its slice through this frame
            let mask = crate::shape::slot_masks(&masks, &mut sl, f, w, h, self.prog.frames)?;
            let mut rgba = raea_kgpu::composite_cpu_masked(&self.bg, w, h, &pl, &sl, &overlay, self.threads, mask.as_deref());
            tm.composite += t.elapsed().as_secs_f64() * 1e3;
            let t = Instant::now();
            // nv12_cpu reads a (h + 1)-row frame (its last row carries the GPU's frame tag); the tag isn't output
            rgba.extend(std::iter::repeat_n(0u32, w));
            let nv: Vec<u8> = if self.ten_bit {
                bytemuck::cast_slice::<u32, u8>(&raea_kgpu::p010_cpu(&rgba, w, h, self.threads))[..w * h * 3].to_vec()
            } else {
                bytemuck::cast_slice::<u32, u8>(&raea_kgpu::nv12_cpu(&rgba, w, h, self.threads))[..w * h * 3 / 2].to_vec()
            };
            tm.nv12 += t.elapsed().as_secs_f64() * 1e3;
            let t = Instant::now();
            sink(f, nv)?;
            tm.sink += t.elapsed().as_secs_f64() * 1e3;
            tm.frames += 1;
            if let Some(a) = ahead.as_ref() {
                a.advance(pos, &last_use);
            }
                }
                Ok(())
            })();
            if let Some(a) = ahead.as_ref() {
                a.stop();
            }
            out
        });
        r?;
        tm.decode_ahead = ahead.as_ref().map(|_| workers).unwrap_or(0);
        Ok(tm)
    }
}
