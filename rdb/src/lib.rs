//! rdb as a library: the edit's modules, loading an edit (from a file, or SQL text,
//! as on iOS), and rendering it. The CLI (main.rs) is a thin layer over this.

// the image codecs rdb links (PNG, JPEG), shared with the media verbs and dependents without a second copy
pub use image;
pub mod animation;
pub mod cli;
pub mod shape;
pub mod volumes;
pub mod beats;
pub mod direction;
pub mod assets;
pub mod db;
pub mod engine;
pub mod eval;
pub mod gherkin;
pub mod cpuview;
pub mod gpuview;
pub mod ir;
pub mod kernel;
pub mod mixline;
// `rdb media VERB`, `rdb ffmpeg`, `rdb ffprobe`: the agent's media work on raea-media (docs/media.feature M1)
pub mod media_verbs;
#[path = "../../meaning/src/k.rs"]
#[allow(dead_code)]
pub mod k;
pub mod captions;
pub mod energy;
pub mod fonts;
pub mod fingerprint;
pub mod flash;
pub mod previz;
pub mod previz_report;
pub mod html;
pub mod query;
pub mod serve;
pub mod stills;
pub mod store;
pub mod timeline;

use std::path::{Path, PathBuf};

use eval::Tup;
use ir::{Program, V};

/// An edit from SQL text (a .sql file's contents): its program and its rows as inserted. Needs no duckdb.
pub fn load_sql(text: &str) -> Result<(Program, Vec<Tup>), String> {
    let p = ir::compile(text)?;
    let rows = inserted_of(&p);
    Ok((p, rows))
}

/// A .sql file's rows as inserted, in file order.
pub fn inserted_of(p: &Program) -> Vec<Tup> {
    let mut out = Vec::new();
    for (kind, rows) in &p.spawns {
        for r in rows {
            let id = match r.get("id") {
                Some(V::S(s)) => s.clone(),
                _ => format!("{kind}{}", out.len()),
            };
            out.push(Tup { kind: kind.clone(), id, vals: r.iter().filter(|(k, _)| *k != "id").map(|(k, v)| (k.clone(), v.clone())).collect() });
        }
    }
    out
}

/// An edit from an .rdb (the live edit; an old .duckdb still loads), or a .sql file (built in one go). An .rrl edit
/// (the retired record language) no longer builds: a TypeScript project exports as SQL (`raea-motion render -o OUT.sql`).
pub fn open(path: &str) -> Result<(Program, Vec<Tup>), String> {
    if path.ends_with(".rrl") {
        Err(format!("{path}: .rrl edits are retired (2026-10-17); edits are TypeScript projects (raea render), or rdb SQL"))
    } else if path.ends_with(".sql") {
        let p = ir::compile(&std::fs::read_to_string(path).map_err(|e| format!("{path}: {e}"))?)?;
        let rows = inserted_of(&p);
        Ok((p, rows))
    } else {
        db::Db::load(Path::new(path))?.state.program()
    }
}

/// The edit's sound as columns: each audio row, and each clip (its `sound`, else its `src`) that names a sound or has a
/// `gain` (0 dB when it has none; `sound = ''` mutes it), is a lane on the
/// timeline between its `from` and `to` frames, starting `src_in` seconds into its source, its gain (dB) read off the
/// frame's state at every frame (so ducking is a rule and its easing a transition row), with 5 ms fades at its ends.
pub fn sound(prog: &Program, rows: &[Tup], dir: &Path) -> Result<Option<raea_media::Audio>, String> {
    sound_tl(prog, &timeline::Timeline::new(prog, rows)?, rows, dir)
}

/// As sound, reading the gains off a timeline already evaluated.
pub fn sound_tl(prog: &Program, tl: &timeline::Timeline, rows: &[Tup], dir: &Path) -> Result<Option<raea_media::Audio>, String> {
    Ok(mastered(prog, rows, lanes_tl(prog, tl, rows, dir)?))
}

/// What the master does to the edit's sound: the gain (dB) it puts on the mix before its limiter, and the ceiling
/// (dBTP). Live play puts a game's voices through the same gain and a limiter at that ceiling, as its render masters
/// them. None without sound or without a master row.
pub fn master_gain(prog: &Program, rows: &[Tup], dir: &Path) -> Result<Option<(f64, f64)>, String> {
    let Some(m) = rows.iter().find(|t| t.kind == "master") else { return Ok(None) };
    let lanes = lanes_tl(prog, &timeline::Timeline::new(prog, rows)?, rows, dir)?;
    if lanes.is_empty() {
        return Ok(None);
    }
    let out: Vec<_> = lanes.into_iter().map(|(_, l)| l).collect();
    // stereo: a stereo source keeps both channels (the mono mix took only its left; live play has both), a mono one
    // plays on both
    let mix = raea_media::audio::mix(&out, prog.frames as f64 / prog.fps, 2);
    let ceiling = m.vals.get("ceiling").map(V::num).unwrap_or(-1.0);
    let (mastered, gain) = raea_media::audio::master_gained(&mix, m.vals.get("lufs").map(V::num).unwrap_or(-14.0), ceiling);
    let (before, after) = (raea_media::audio::lufs(&mix), raea_media::audio::lufs(&mastered));
    if std::env::var_os("RAEA_MASTER_TRACE").is_some() {
        eprintln!("master_gain: mix {before:.2} LUFS ({:.1} s), gain {gain:.2} dB before the limiter, mastered {after:.2} LUFS, true peak {:.2} dBTP", mix.duration(), raea_media::audio::true_peak_db(&mastered));
    }
    if !before.is_finite() || !after.is_finite() || !gain.is_finite() {
        return Ok(None);
    }
    // the gain before the limiter: live play puts its own limiter at the same ceiling after it
    Ok(Some((gain, ceiling)))
}

/// The lanes mixed, then mastered to the master row's loudness and ceiling.
fn mastered(prog: &Program, rows: &[Tup], lanes: Vec<(String, raea_media::audio::Lane)>) -> Option<raea_media::Audio> {
    if lanes.is_empty() {
        return None;
    }
    let out: Vec<_> = lanes.into_iter().map(|(_, l)| l).collect();
    // stereo: a stereo source keeps both channels (the mono mix took only its left; live play has both), a mono one
    // plays on both
    let mix = raea_media::audio::mix(&out, prog.frames as f64 / prog.fps, 2);
    // the master row: loudness target and peak ceiling
    Some(match rows.iter().find(|t| t.kind == "master") {
        // held to the ceiling after AAC on Apple (the encoder the file gets); elsewhere the limiter's own ceiling
        #[cfg(target_vendor = "apple")]
        Some(m) => raea_media::audio::master_for(&mix, m.vals.get("lufs").map(V::num).unwrap_or(-14.0), m.vals.get("ceiling").map(V::num).unwrap_or(-1.0), &raea_media::apple::aac_round_trip),
        #[cfg(not(target_vendor = "apple"))]
        Some(m) => raea_media::audio::master(&mix, m.vals.get("lufs").map(V::num).unwrap_or(-14.0), m.vals.get("ceiling").map(V::num).unwrap_or(-1.0)),
        None => mix,
    })
}

/// What a brief can hear: a `sound` row (id 'sound') whose columns are measured from the mix. Per frame:
/// voice_lufs and music_lufs, the momentary loudness (400 ms) of the clips' sound and of the audio rows. Constant:
/// voice_lufs_i, the voice's integrated loudness, and the master's lufs and peak_db.
pub fn hearing(prog: &mut Program, rows: &mut Vec<Tup>, dir: &Path) -> Result<(), String> {
    use raea_media::audio::{lufs, mix, momentary, peak_db};
    let tl = timeline::Timeline::new(prog, rows)?;
    let lanes = lanes_tl(prog, &tl, rows, dir)?;
    if lanes.is_empty() {
        return Ok(());
    }
    let (fps, n) = (prog.fps, prog.frames as usize);
    let secs = n as f64 / fps;
    let stem = |kind: &str| mix(&lanes.iter().filter(|(k, _)| k.starts_with(kind)).map(|(_, l)| l.clone()).collect::<Vec<_>>(), secs, 1);
    let (voice, music) = (stem("clip:"), stem("audio:"));
    let master = mastered(prog, rows, lanes.clone()).unwrap_or_default();
    let mut vals = std::collections::BTreeMap::new();
    vals.insert("voice_lufs_i".into(), V::N(lufs(&voice)));
    vals.insert("lufs".into(), V::N(lufs(&master)));
    vals.insert("peak_db".into(), V::N(peak_db(&master)));
    for (col, a) in [("voice_lufs", &voice), ("music_lufs", &music)] {
        let m = momentary(a, fps, n);
        vals.insert(col.into(), V::N(m[0]));
        prog.keys.insert(("sound".into(), col.into()), m.iter().enumerate().map(|(f, v)| (f as i64, *v, "step".to_string())).collect());
    }
    rows.push(Tup { kind: "sound".into(), id: "sound".into(), vals });
    Ok(())
}

/// The sound's lanes, by "kind:id".
pub fn lanes(prog: &Program, rows: &[Tup], dir: &Path) -> Result<Vec<(String, raea_media::audio::Lane)>, String> {
    lanes_tl(prog, &timeline::Timeline::new(prog, rows)?, rows, dir)
}

/// As lanes, over a timeline already evaluated.
pub fn lanes_tl(prog: &Program, tl: &timeline::Timeline, rows: &[Tup], dir: &Path) -> Result<Vec<(String, raea_media::audio::Lane)>, String> {
    let fps = prog.fps;
    let mut lanes: std::collections::BTreeMap<String, (Tup, Vec<(f64, f64)>)> = Default::default();
    let live = |t: &Tup, f: i64| (f as f64) >= t.vals.get("from").map(V::num).unwrap_or(0.0) && (f as f64) < t.vals.get("to").map(V::num).unwrap_or(f64::INFINITY);
    let explicit = |t: &Tup| t.kind == "clip" && (t.vals.contains_key("gain") || matches!(t.vals.get("sound"), Some(V::S(s)) if !s.is_empty()));
    // a clip that says nothing about sound plays its own unless it lies over a clip that sounds (b-roll is mute):
    // a lone clip written without gain was silent (an agent session's React-parts Reel, 2026-10-15)
    let mut over: std::collections::HashSet<String> = Default::default();
    for f in 0..prog.frames {
        let tups = tl.tups(f);
        if tups.iter().any(|t| explicit(t) && live(t, f)) {
            for t in tups.iter().filter(|t| t.kind == "clip" && !explicit(t) && live(t, f)) {
                over.insert(t.id.clone());
            }
        }
    }
    let mut implicit: std::collections::HashSet<String> = Default::default();
    for f in 0..prog.frames {
        for t in tl.tups(f) {
            // sound = '' says mute, whatever the gain
            let muted = matches!(t.vals.get("sound"), Some(V::S(s)) if s.is_empty());
            let quiet_clip = t.kind == "clip" && !explicit(&t);
            let sounds = !muted && (t.kind == "audio" || explicit(&t) || (quiet_clip && !over.contains(&t.id)));
            if !sounds {
                continue;
            }
            if quiet_clip {
                implicit.insert(format!("clip:{}", t.id));
            }
            let g = |k: &str, d: f64| t.vals.get(k).map(V::num).unwrap_or(d);
            if (f as f64) < g("from", 0.0) || (f as f64) >= g("to", f64::INFINITY) {
                continue;
            }
            let key = format!("{}:{}", t.kind, t.id);
            let gain = g("gain", 0.0);
            lanes.entry(key).or_insert_with(|| (t.clone(), Vec::new())).1.push((f as f64 / fps, gain));
        }
    }
    let mut out = Vec::new();
    for (key, (t, gains)) in lanes {
        let src = match t.vals.get("sound").filter(|v| !matches!(v, V::S(s) if s.is_empty())).or(t.vals.get("src")) {
            Some(V::S(s)) => s.clone(),
            _ => return Err(format!("{} {}: no src", t.kind, t.id)),
        };
        let g = |k: &str, d: f64| t.vals.get(k).map(V::num).unwrap_or(d);
        let from = g("from", 0.0);
        let to = g("to", prog.frames as f64).min(prog.frames as f64);
        // only the stretch of the source this lane plays (and a little over), not the whole file per lane: a long take
        // under four clips was decoded whole and copied four times (renders peaked at 1.7-2.5 GB on 20 s edits)
        let (src_in, len) = (g("src_in", 0.0).max(0.0), (to - from) / fps);
        let path = engine::resolve_src(rows, &src, dir).map_err(|e| format!("{} {}'s sound: {e}", t.kind, t.id))?;
        let audio = match raea_media::decode_audio_window(&path, src_in, len + 0.1) {
            Ok(a) if a.len() > 0 => a,
            // footage with no sound track, playing by default: nothing to hear
            Ok(_) | Err(_) if implicit.contains(&key) => continue,
            Ok(_) => continue,
            Err(e) => return Err(format!("{} {}'s sound {src} ({}): {e}", t.kind, t.id, path.display())),
        };
        out.push((key, raea_media::audio::Lane {
            audio,
            at: from / fps,
            from: 0.0,
            len: (to - from) / fps,
            gain_db: gains,
            fade_in: g("fade_in", 0.005),
            fade_out: g("fade_out", 0.005),
        }));
    }
    Ok(out)
}

pub fn dir_of(path: &str) -> PathBuf {
    std::fs::canonicalize(Path::new(path).parent().filter(|d| !d.as_os_str().is_empty()).unwrap_or(Path::new("."))).unwrap_or_else(|_| PathBuf::from("."))
}

pub fn save_png(path: &Path, rgba: &[u8], w: u32, h: u32) -> Result<(), String> {
    image::save_buffer(path, rgba, w, h, image::ColorType::Rgba8).map_err(|e| format!("{}: {e}", path.display()))
}

/// What a frame shows, as a digest of its rows: every visible row's values, and for clips the source time they show.
/// Two frames with the same digest are the same pixels (the engine draws only from rows), wherever they sit in time.
pub fn frame_digest(st: &[Tup], f: i64, fps: f64) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut rows: Vec<&Tup> = st
        .iter()
        .filter(|t| {
            let g = |k: &str, d: f64| t.vals.get(k).map(V::num).unwrap_or(d);
            (f as f64) >= g("from", f64::MIN) && (f as f64) < g("to", f64::MAX)
        })
        .collect();
    rows.sort_by(|a, b| (&a.kind, &a.id).cmp(&(&b.kind, &b.id)));
    let mut h = std::collections::hash_map::DefaultHasher::new();
    for t in rows {
        (&t.kind, &t.id).hash(&mut h);
        for (c, v) in &t.vals {
            if matches!(c.as_str(), "from" | "to" | "gain" | "sound") {
                continue;
            }
            (c, format!("{v:?}")).hash(&mut h);
        }
        if t.kind == "clip" {
            let g = |k: &str| t.vals.get(k).map(V::num).unwrap_or(0.0);
            (((g("src_in") + (f as f64 - g("from")) / fps) * 1e6).round() as i64).hash(&mut h);
        }
    }
    h.finish()
}

/// What a render makes and how (the CLI's flags).
#[derive(Clone, Debug, Default)]
pub struct RenderOpts {
    /// the frames, in order (all of them when None)
    pub frames: Option<Vec<i64>>,
    /// the media backend that encodes (apple, soft, av1, ffmpeg); the platform's when None
    pub codec: Option<String>,
    pub kbps: Option<u32>,
    /// keep every 30th frame and report the encode's SSIM against them
    pub ssim: bool,
}

/// Render an edit file (.sql or .rdb) to OUT: .mp4 (video and its sound), .txt (one hash per
/// frame), .raw (the frames as they came back) or a directory of PNGs. Edits without 3D take the direct path to .mp4,
/// .mov and .txt (RAEA_DIRECT=1 forces it). Its stats go to stderr as one JSON line.
pub fn render_file(edit: &str, out: &Path, opts: &RenderOpts) -> Result<(), String> {
    engine::mem_mark("render: start");
    // an HTML composition: its videos on the GPU, the page over them (html.rs)
    if edit.ends_with(".html") || edit.ends_with(".htm") {
        return html::render_file(edit, out, opts);
    }
    let (prog, rows) = open(edit)?;
    render(&prog, &rows, &dir_of(edit), out, opts)
}

/// Where an edit with clips shows none, and where an edit with sound has none, as spans over a quarter second.
pub fn coverage(prog: &Program, tl: &timeline::Timeline) -> Vec<String> {
    let fps = prog.fps.max(1.0);
    let (mut pic, mut snd) = (vec![false; prog.frames.max(0) as usize], vec![false; prog.frames.max(0) as usize]);
    let (mut any_clip, mut any_sound) = (false, false);
    for f in 0..prog.frames {
        for t in tl.tups(f) {
            let g = |k: &str, d: f64| t.vals.get(k).map(V::num).unwrap_or(d);
            let on = (f as f64) >= g("from", 0.0) && (f as f64) < g("to", f64::INFINITY);
            let named = |k: &str| matches!(t.vals.get(k), Some(V::S(s)) if !s.is_empty());
            let muted = matches!(t.vals.get("sound"), Some(V::S(s)) if s.is_empty());
            if t.kind == "clip" {
                any_clip = true;
                pic[f as usize] |= on;
            }
            if !muted && (t.kind == "audio" || (t.kind == "clip" && (t.vals.contains_key("gain") || named("sound")))) {
                any_sound = true;
                snd[f as usize] |= on;
            }
        }
    }
    let spans = |covered: &[bool], what: &str| -> Vec<String> {
        let mut out = vec![];
        let mut f = 0;
        while f < covered.len() {
            if covered[f] {
                f += 1;
                continue;
            }
            let a = f;
            while f < covered.len() && !covered[f] {
                f += 1;
            }
            if (f - a) as f64 / fps > 0.25 {
                out.push(format!("{what} {:.2}–{:.2} s (frames {a}–{}) of {:.2} s", a as f64 / fps, f as f64 / fps, f - 1, prog.frames as f64 / fps));
            }
        }
        out
    };
    let mut out = vec![];
    if any_clip {
        out.extend(spans(&pic, "no clip on screen"));
    }
    if any_sound {
        out.extend(spans(&snd, "no sound"));
    }
    out
}

/// Render a loaded edit (see `render_file`; `dir` is where its relative media paths start).
pub fn render(prog: &Program, rows: &[Tup], dir: &Path, out: &Path, ro: &RenderOpts) -> Result<(), String> {
    render_with(prog, rows, dir, out, ro, None)
}

/// The direct render with its pages handed over on the GPU: `feed` gives frame f's page (premultiplied RGBA8 packed,
/// on the paint device, quads::page_buffer) just before f composites, so the pages go from the paint to the encoder
/// with no file and no readback but the NV12 the encoder takes (gate 5 of no-V8: renders hand pages over as
/// textures). `prog` is the picture's program (its size, rate, length and background; no rows: the page is the
/// picture), and `sound_of` the sound's, asked for once the last frame is made (the caller learns it from the frames).
pub fn render_fed(
    prog: &Program,
    dir: &Path,
    out: &Path,
    ro: &RenderOpts,
    feed: impl FnMut(i64) -> Result<wgpu::Buffer, String>,
    sound_of: impl FnOnce() -> Result<Option<(Program, Vec<Tup>)>, String>,
) -> Result<(), String> {
    if gpuview::paint_device().is_none() {
        return Err("a fed render needs the paint device on the GPU".into());
    }
    let list: Vec<i64> = ro.frames.clone().unwrap_or_else(|| (0..prog.frames).collect());
    let t0 = std::time::Instant::now();
    let mut v = gpuview::GpuView::new(prog, &[], dir)?;
    let (w, h) = (v.w, v.h);
    let setup_s = t0.elapsed().as_secs_f64();
    let hashes = out.extension().is_some_and(|x| x == "txt");
    let mut lines = Vec::new();
    // the encoder on its own thread; the sound reaches it after the frames (the mix waits on every frame's sounds)
    let mut tx = None;
    let mut enc_thread = None;
    let (mix_tx, mix_rx) = std::sync::mpsc::channel::<Option<raea_media::Audio>>();
    if !hashes {
        let codec = ro.codec.clone().unwrap_or_else(engine::media_backend);
        let opts = raea_media::EncodeOpts { width: w, height: h, fps: prog.fps, kbps: ro.kbps.unwrap_or(10000), nv12: true, realtime: false };
        let (t, rx) = std::sync::mpsc::sync_channel::<Vec<u8>>(4);
        tx = Some(t);
        let out = out.to_path_buf();
        enc_thread = Some(std::thread::spawn(move || -> Result<(raea_media::EncodeStats, f64), String> {
            let mut enc = raea_media::backend(&codec).and_then(|b| b.encoder(&out, &opts))?;
            for px in rx {
                enc.push_nv12(&px, w as usize, h as usize)?;
            }
            let mix = mix_rx.recv().map_err(|_| "the sound never came".to_string())?;
            let t = std::time::Instant::now();
            let st = enc.finish(mix.as_ref())?;
            Ok((st, t.elapsed().as_secs_f64()))
        }));
    }
    let t_frames = std::time::Instant::now();
    let dt = v.render_nv12_fed(&list, feed, |f, px| {
        if let Some(tx) = tx.as_ref() {
            return tx.send(px).map_err(|_| "the encoder stopped".to_string());
        }
        let mut hsh = 0xcbf2_9ce4_8422_2325u64;
        for b in &px {
            hsh ^= *b as u64;
            hsh = hsh.wrapping_mul(0x100_0000_01b3);
        }
        lines.push(format!("{f} {hsh:016x}"));
        Ok(())
    });
    drop(tx);
    let sent = match dt {
        Ok(_) if !hashes => {
            let ta = std::time::Instant::now();
            let mix = match sound_of()? {
                Some((sp, rows)) => sound(&sp, &rows, dir)?,
                None => None,
            };
            let _ = mix_tx.send(mix);
            Some(ta.elapsed().as_secs_f64())
        }
        _ => None,
    };
    drop(mix_tx);
    let joined = match enc_thread {
        Some(j) => Some(j.join().map_err(|_| "the encoder panicked".to_string())?),
        None => None,
    };
    let dt = dt?;
    let joined = joined.transpose()?;
    let finish_s = joined.as_ref().map(|j| j.1).unwrap_or(0.0);
    let frames_s = t_frames.elapsed().as_secs_f64() - finish_s - sent.unwrap_or(0.0);
    let stats = joined.map(|j| j.0);
    if hashes {
        std::fs::write(out, lines.join("\n") + "\n").map_err(|e| e.to_string())?;
    }
    eprintln!("{}", serde_json::json!({"direct": true, "fed": true, "frames": list.len(), "fps_incl_encode": list.len() as f64 / frames_s, "setup_s": setup_s, "finish_s": finish_s, "sound_s": sent,
        "bytes": stats.as_ref().map(|s| s.bytes), "video": stats.as_ref().map(|s| s.video_codec.clone()), "times": dt.json()}));
    Ok(())
}

/// As render; with a page (an HTML composition's own paint, html.rs) as the overlay over the clips, on the direct path.
pub fn render_with(prog: &Program, rows: &[Tup], dir: &Path, out: &Path, ro: &RenderOpts, page: Option<(raea::scene::Composition, PathBuf)>) -> Result<(), String> {
    let (prog, rows) = (prog.clone(), rows.to_vec());
    let list: Vec<i64> = ro.frames.clone().unwrap_or_else(|| (0..prog.frames).collect());
    // every frame evaluated once, shared by the picture and the sound
    let tl = std::sync::Arc::new(timeline::Timeline::new(&prog, &rows)?);
    // a whole render says where the picture or the sound has nothing in it (an agent can't hear a silent tail)
    if ro.frames.is_none() {
        for w in coverage(&prog, &tl) {
            println!("warning: {w}");
        }
    }
    // frames in order reuse almost nothing from the pool: a small budget (RAEA_POOL_MB still wins)
    engine::LINEAR_RENDER.store(list.windows(2).all(|p| p[1] > p[0]), std::sync::atomic::Ordering::Relaxed);
    // the direct render is the default for edits without 3D going to video or hashes: the same bytes as the engine's
    // GPU path (the gate checks both against G8's reference), 1.8x the frames a second (a CJ concept: 197 fps vs 107)
    // (the engine's own path when one of its options is asked for: RAEA_READBACK, RAEA_COMPOSITE choose its stages)
    let engine_opts = std::env::var_os("RAEA_READBACK").is_some() || std::env::var_os("RAEA_COMPOSITE").is_some();
    let to_direct = out.extension().is_some_and(|x| matches!(x.to_str(), Some("mp4" | "mov" | "txt"))) && !ro.ssim && !engine::has_3d(&rows) && !engine_opts;
    // the CPU render (docs/cpu.feature C1): RAEA_CPU, or a machine with no GPU adapter; the same frames as the GPU's
    let cpu = page.is_none() && (std::env::var_os("RAEA_CPU").is_some() || ((to_direct || std::env::var_os("RAEA_DIRECT").is_some()) && !raea_kgpu::Gpu::available()));
    if cpu {
        let t0 = std::time::Instant::now();
        let mut v = cpuview::CpuView::new(&prog, &rows, dir)?;
        v.share_timeline(tl.clone());
        let (w, h) = (v.w, v.h);
        let setup_s = t0.elapsed().as_secs_f64();
        let hashes = out.extension().is_some_and(|x| x == "txt");
        let mut lines = Vec::new();
        let mut tx = None;
        let mut enc_thread = None;
        if !hashes {
            let mix = sound_tl(&prog, &tl, &rows, dir)?;
            let codec = ro.codec.clone().unwrap_or_else(engine::media_backend);
            let opts = raea_media::EncodeOpts { width: w, height: h, fps: prog.fps, kbps: ro.kbps.unwrap_or(10000), nv12: true, realtime: false };
            let (t, rx) = std::sync::mpsc::sync_channel::<Vec<u8>>(4);
            tx = Some(t);
            let out = out.to_path_buf();
            enc_thread = Some(std::thread::spawn(move || -> Result<raea_media::EncodeStats, String> {
                let mut enc = raea_media::backend(&codec).and_then(|b| b.encoder(&out, &opts))?;
                for px in rx {
                    enc.push_nv12(&px, w as usize, h as usize)?;
                }
                enc.finish(mix.as_ref())
            }));
        }
        let t_frames = std::time::Instant::now();
        let tm = v.render_nv12(&list, |f, px| {
            if let Some(tx) = tx.as_ref() {
                return tx.send(px).map_err(|_| "the encoder stopped".to_string());
            }
            let mut hsh = 0xcbf2_9ce4_8422_2325u64;
            for b in &px {
                hsh ^= *b as u64;
                hsh = hsh.wrapping_mul(0x100_0000_01b3);
            }
            lines.push(format!("{f} {hsh:016x}"));
            Ok(())
        })?;
        drop(tx);
        let stats = match enc_thread {
            Some(j) => Some(j.join().map_err(|_| "the encoder panicked".to_string())??),
            None => None,
        };
        let frames_s = t_frames.elapsed().as_secs_f64();
        if hashes {
            std::fs::write(out, lines.join("\n") + "\n").map_err(|e| e.to_string())?;
        }
        eprintln!("{}", serde_json::json!({"direct": true, "cpu": true, "frames": list.len(), "fps_incl_encode": list.len() as f64 / frames_s, "setup_s": setup_s,
            "bytes": stats.as_ref().map(|s| s.bytes), "video": stats.as_ref().map(|s| s.video_codec.clone()), "times": tm.json()}));
        return Ok(());
    }
    if std::env::var_os("RAEA_DIRECT").is_some() || page.is_some() || to_direct {
        // the direct render (edits without 3D): gpuview's pool and kernels, no Bevy schedules; NV12 to the encoder
        let t0 = std::time::Instant::now();
        let mut v = gpuview::GpuView::new(&prog, &rows, dir)?;
        if let Some((p, path)) = page {
            v.set_page(p);
            // painted ahead on workers when there's more than a frame or two (RAEA_PAGE_WORKERS, default: cores - 2)
            let workers = std::env::var("RAEA_PAGE_WORKERS").ok().and_then(|w| w.parse().ok())
                .unwrap_or_else(|| std::thread::available_parallelism().map(|c| c.get().saturating_sub(2)).unwrap_or(4)).max(1);
            if list.len() > 2 && workers > 1 {
                v.set_page_pool(html::PagePool::start(&path, list.clone(), prog.fps, workers));
            }
        }
        v.share_timeline(tl.clone());
        let (w, h) = (v.w, v.h);
        let setup_s = t0.elapsed().as_secs_f64();
        let hashes = out.extension().is_some_and(|x| x == "txt");
        let mut lines = Vec::new();
        // the encoder on its own thread (frames over a short queue), so encoding overlaps the next frames
        let mut tx = None;
        let mut enc_thread = None;
        if !hashes {
            let mix = sound_tl(&prog, &tl, &rows, dir)?;
            let codec = ro.codec.clone().unwrap_or_else(engine::media_backend);
            let opts = raea_media::EncodeOpts { width: w, height: h, fps: prog.fps, kbps: ro.kbps.unwrap_or(10000), nv12: true, realtime: engine::has_3d(&rows) };
            let (t, rx) = std::sync::mpsc::sync_channel::<Vec<u8>>(4);
            tx = Some(t);
            let out = out.to_path_buf();
            enc_thread = Some(std::thread::spawn(move || -> Result<(raea_media::EncodeStats, f64), String> {
                let mut enc = raea_media::backend(&codec).and_then(|b| b.encoder(&out, &opts))?;
                for px in rx {
                    enc.push_nv12(&px, w as usize, h as usize)?;
                }
                let t = std::time::Instant::now();
                let st = enc.finish(mix.as_ref())?;
                Ok((st, t.elapsed().as_secs_f64()))
            }));
        }
        let t_frames = std::time::Instant::now();
        let dt = v
            .render_nv12(&list, |f, px| {
                if let Some(tx) = tx.as_ref() {
                    return tx.send(px).map_err(|_| "the encoder stopped".to_string());
                }
                let mut hsh = 0xcbf2_9ce4_8422_2325u64;
                for b in &px {
                    hsh ^= *b as u64;
                    hsh = hsh.wrapping_mul(0x100_0000_01b3);
                }
                lines.push(format!("{f} {hsh:016x}"));
                Ok(())
            })
            ?;
        let t = std::time::Instant::now();
        drop(tx);
        let joined = match enc_thread {
            Some(j) => Some(j.join().map_err(|_| "the encoder panicked".to_string())??),
            None => None,
        };
        // frames: until the last frame is encoded (the queue drained), without the file's finishing
        let finish_s = joined.as_ref().map(|j| j.1).unwrap_or(0.0);
        let frames_s = t_frames.elapsed().as_secs_f64() - finish_s;
        let drain_s = t.elapsed().as_secs_f64() - finish_s;
        let stats = joined.map(|j| j.0);
        if hashes {
            std::fs::write(&out, lines.join("\n") + "\n").map_err(|e| e.to_string())?;
        }
        eprintln!("{}", serde_json::json!({"direct": true, "frames": list.len(), "fps_incl_encode": list.len() as f64 / frames_s, "setup_s": setup_s, "finish_s": finish_s, "encoder_drain_s": drain_s,
            "bytes": stats.as_ref().map(|s| s.bytes), "video": stats.as_ref().map(|s| s.video_codec.clone()), "times": dt.json(), "pool_mb": v.pool_bytes as f64 / 1e6, "upload_cpu_ms_per_upload": v.upload_cpu_ms / dt.uploads.max(1) as f64}));
        return Ok(());
    }
    let mut e = engine::Engine::new_shared(&prog, &rows, dir, Some(tl.clone()))?;
    let (w, h) = e.size();
    let t0 = std::time::Instant::now();
    if out.extension().is_some_and(|x| x == "mp4") {
        // the sound: every clip or audio row with a gain, its gain read off the rules frame by frame
        let ta = std::time::Instant::now();
        let mix = sound_tl(&prog, &tl, &rows, dir)?;
        let audio_s = ta.elapsed().as_secs_f64();
        engine::mem_mark("render: audio mixed");
        let codec = ro.codec.clone().unwrap_or_else(engine::media_backend);
        let opts = raea_media::EncodeOpts { width: w, height: h, fps: prog.fps, kbps: ro.kbps.unwrap_or(10000), nv12: e.nv12, realtime: engine::has_3d(&rows) };
        let mut enc = raea_media::backend(&codec).and_then(|b| b.encoder(&out, &opts))?;
        engine::mem_mark("render: encoder made");
        let mut encode_s = 0.0;
        let t_all = std::time::Instant::now();
        // --ssim: keep every 30th frame uncompressed, and compare the encoded file's frames with them after
        let keep = ro.ssim;
        let mut kept: Vec<(i64, Vec<u8>)> = Vec::new();
        let nv12 = e.nv12;
        e.frames(&list, |f, px| {
            if keep && f % 30 == 0 {
                kept.push((f, px.clone()));
            }
            let t = std::time::Instant::now();
            let r = if nv12 { enc.push_nv12(&px, w as usize, h as usize) } else { enc.push(&px) };
            encode_s += t.elapsed().as_secs_f64();
            r
        })
        ?;
        let render_s = t_all.elapsed().as_secs_f64() - encode_s;
        let t = std::time::Instant::now();
        let stats = enc.finish(mix.as_ref())?;
        encode_s += t.elapsed().as_secs_f64();
        let n = list.len() as f64;
        let st = e.stages();
        let ms = |ns: u64| ns as f64 / 1e6 / st.frames.max(1) as f64;
        let convert = raea_media::CONVERT_NS.load(std::sync::atomic::Ordering::Relaxed);
        let enc_convert = raea_media::ENCODE_CONVERT_NS.load(std::sync::atomic::Ordering::Relaxed);
        let stages = serde_json::json!({
            "frames": st.frames, "updates_per_frame": st.updates as f64 / st.frames.max(1) as f64,
            "rules": ms(st.rules), "decode": ms(st.clips.saturating_sub(st.clip_prep).saturating_sub(convert)),
            "yuv_to_rgba": ms(convert), "clip_texture_write": ms(st.clip_prep), "overlay": ms(st.overlay),
            "main_world_total": ms(st.main), "render_world": ms(st.update.saturating_sub(st.main)),
            "readback_copy": ms(st.readback_copy), "readback_bytes": if e.nv12 { (w * h / 4 + w * h / 8 + 1) as usize * 4 } else { (w as usize * 4).div_ceil(256) * 256 * (h as usize + 1) },
            "gpu_wait_serial": ms(st.gpu_wait),
            "encode_rgba_convert": ms(enc_convert), "encode_total": encode_s * 1e3 / st.frames.max(1) as f64,
            "wall_per_frame": t0.elapsed().as_secs_f64() * 1e3 / n,
        });
        let ssim = if keep {
            let mut d = raea_media::backend("ffmpeg").and_then(|b| b.open_video(&out))?;
            let v: Vec<f64> = kept
                .iter()
                .map(|(f, px)| -> Result<f64, String> {
                    let fr = d.frame_at(*f as f64 / prog.fps)?;
                    Ok(raea_media::ssim(&e.to_rgba(px), &fr.rgba, w as usize, h as usize))
                })
                .collect::<Result<_, _>>()?;
            Some(v.iter().sum::<f64>() / v.len().max(1) as f64)
        } else {
            None
        };
        eprintln!(
            "{}",
            serde_json::json!({"frames": list.len(), "video": stats.video_codec, "audio": stats.audio_codec, "bytes": stats.bytes,
                "render_fps": n / render_s, "overlay_drawn_reused": e.overlay_stats(), "stages_ms_per_frame": stages, "pool_uploads_reuses_bytes": e.pool_stats(), "gpu_stage_ms": e.gpu_stage_times().map(|(m, n)| serde_json::json!({"clip_uploads": m[0], "overlay_upload": m[1], "composite": m[2], "nv12": m[3], "frames": n})), "encode_fps": n / encode_s, "ssim": ssim, "seconds": t0.elapsed().as_secs_f64(), "audio_s": audio_s,
                "lufs": mix.as_ref().map(raea_media::audio::lufs), "peak_db": mix.as_ref().map(raea_media::audio::peak_db)})
        );
    } else if out.extension().is_some_and(|x| x == "raw") {
        // raw: the frames as they came back (RGBA or NV12), concatenated (for exactness checks)
        let mut f = std::io::BufWriter::new(std::fs::File::create(&out).map_err(|e| e.to_string())?);
        e.frames(&list, |_, px| std::io::Write::write_all(&mut f, &px).map_err(|e| e.to_string()))?;
    } else if out.extension().is_some_and(|x| x == "txt") {
        // hash-only: one line per frame, "frame fnv64-of-rgba", nothing else on disk
        let mut lines = Vec::new();
        e.frames(&list, |f, px| {
            let mut h = 0xcbf2_9ce4_8422_2325u64;
            for b in &px {
                h ^= *b as u64;
                h = h.wrapping_mul(0x100_0000_01b3);
            }
            lines.push(format!("{f} {h:016x}"));
            Ok(())
        })
        ?;
        std::fs::write(&out, lines.join("\n") + "\n").map_err(|e| e.to_string())?;
    } else {
        std::fs::create_dir_all(&out).map_err(|e| e.to_string())?;
        let nv12 = e.nv12;
        let rgba_of = |px: Vec<u8>| if nv12 { raea_media::yuv::nv12_to_rgba(&px, w as usize, h as usize) } else { px };
        e.frames(&list, |f, px| save_png(&out.join(format!("frame-{f:05}.png")), &rgba_of(px), w, h))?;
    }
    eprintln!("{} ({} frames) in {:.2}s", out.display(), list.len(), t0.elapsed().as_secs_f64());
    Ok(())
}
