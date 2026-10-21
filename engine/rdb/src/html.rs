//! An HTML composition (raea/engine's the HTML composition format-format page) on rdb's GPU path. The page's `<video>`s become clip
//! rows that the GPU decodes and composites (VideoToolbox zero-copy where it can), its `<audio>` becomes sound rows,
//! and everything else is engine's own paint of the page, over transparency, repainted only when the page changes.
//! A page whose videos aren't plain pictures in boxes (transforms, opacity, rounded corners, content under them) is
//! refused with the reason, and renders on engine's CPU path (`raea render`) instead.

use std::path::Path;

use crate::eval::Tup;
use crate::ir::{Program, V};

/// The composition, lowered: rdb's program and rows, the page that paints the overlay, and its clear colour.
pub struct Lowered {
    pub prog: Program,
    pub rows: Vec<Tup>,
    pub page: raea::scene::Composition,
}

fn row(kind: &str, id: &str, vals: &[(&str, V)]) -> Tup {
    Tup { kind: kind.into(), id: id.into(), vals: vals.iter().map(|(k, v)| (k.to_string(), v.clone())).collect() }
}

/// Lower an HTML composition. Must run on engine's render thread (`raea::api::on_render_thread`).
pub fn lower(path: &Path) -> Result<Lowered, String> {
    let o = raea::api::RenderOptions::from_json(None)?;
    let (html, dir) = raea::api::resolve_input(path)?;
    let fps = o.fps.unwrap_or_else(|| raea::api::default_fps(&html));
    let probe = raea::api::probe(&o, &html, &dir, fps)?;
    let fpsf = fps.0 as f64 / fps.1.max(1) as f64;
    let duration = raea::api::probe_json(&probe, fps)["duration"].as_f64().unwrap_or(0.0);
    let mut page = probe.comp;
    let (w, h) = page.output_size();
    if duration <= 0.0 {
        return Err("the composition's length is unknown (no data-duration)".into());
    }
    let frames = raea::api::frame_count(duration, fpsf) as i64;
    let clear = page.host_media()?;
    // remote sources (http/https) as engine fetches them, into the same cache
    let net = raea::net::SyncNet::new(&dir, true);
    let cache = raea::media::media_cache(&net);
    let local = |src: &str| -> Result<String, String> { Ok(raea::media::fetch_source(&net, &cache, src)?.to_string_lossy().into_owned()) };
    let hex = format!("#{:02x}{:02x}{:02x}", clear[0], clear[1], clear[2]);
    let n = |x: f64| V::N(x);
    let s = |x: &str| V::S(x.to_string());
    let mut rows = vec![row("camera", "cam", &[("bg", s(&hex))])];
    // the first frame at or after t (engine shows a clip on frames whose time is inside its window)
    let frame = |t: f64| (t * fpsf - 1e-6).ceil();
    // every frame sampled once for every video (a seek lays the page out): each video's box and opacity per frame
    let mut seen: std::collections::HashMap<String, Vec<(i64, raea::scene::MediaLayer)>> = std::collections::HashMap::new();
    for f in 0..frames {
        for l in page.media_layers(f as f64 / fpsf) {
            seen.entry(l.id.clone()).or_default().push((f, l));
        }
    }
    let mut keys: std::collections::BTreeMap<(String, String), Vec<(i64, f64, String)>> = std::collections::BTreeMap::new();
    for v in &probe.videos {
        if v.looping || (v.playback_rate - 1.0).abs() > 1e-9 {
            return Err(format!("video {}: looping or a playback rate", v.id));
        }
        let (from, to) = (frame(v.start), frame(v.end).min(frames as f64));
        let shown: Vec<&(i64, raea::scene::MediaLayer)> = seen.get(&v.id).map(|x| x.iter().filter(|(f, _)| (*f as f64) >= from && (*f as f64) < to).collect()).unwrap_or_default();
        let Some((_, b)) = shown.first() else { continue };
        for (_, l) in &shown {
            if let Some(why) = &l.not_simple {
                return Err(format!("video {}: {why}", v.id));
            }
            if l.fit != "cover" && l.fit != "fill" {
                return Err(format!("video {}: object-fit {}", v.id, l.fit));
            }
            if (l.position.0 - 0.5).abs() > 1e-6 || (l.position.1 - 0.5).abs() > 1e-6 {
                return Err(format!("video {}: object-position isn't centred", v.id));
            }
        }
        let src = local(&v.src)?;
        let mut vals = vec![
            ("src", s(&src)),
            // its first frame lands after v.start: the source time there, as engine would read it
            ("src_in", n(v.media_start + from / fpsf - v.start)),
            ("from", n(from)),
            ("to", n(to)),
            ("z", n(b.z as f64)),
        ];
        // a column when it holds still, keys at each frame when it moves (a slide, a Ken Burns box, a fade)
        let cols: [(&str, fn(&raea::scene::MediaLayer) -> f64); 5] = [("x", |l| l.x), ("y", |l| l.y), ("w", |l| l.w), ("h", |l| l.h), ("opacity", |l| l.opacity)];
        for (name, get) in cols {
            let first = get(b);
            if shown.iter().all(|(_, l)| (get(l) - first).abs() < 1e-3) {
                if name != "opacity" || (first - 1.0).abs() > 1e-6 {
                    vals.push((name, n(first)));
                }
            } else {
                keys.insert((v.id.clone(), name.to_string()), shown.iter().map(|(f, l)| (*f, get(l), "linear".to_string())).collect());
            }
        }
        if v.has_audio {
            vals.push(("sound", s(&src)));
            vals.push(("gain", n(20.0 * v.volume.max(1e-6).log10())));
        } else {
            vals.push(("sound", s("")));
        }
        rows.push(row("clip", &v.id, &vals));
    }
    for a in &probe.audio {
        if a.looping || (a.playback_rate - 1.0).abs() > 1e-9 {
            return Err(format!("audio {}: looping or a playback rate", a.id));
        }
        rows.push(row("audio", &a.id, &[
            ("src", s(&local(&a.src)?)),
            ("src_in", n(a.media_start + frame(a.start) / fpsf - a.start)),
            ("from", n(frame(a.start))),
            ("to", n(frame(a.end))),
            ("gain", n(20.0 * a.volume.max(1e-6).log10())),
        ]));
    }
    let prog = Program { fps: fpsf, width: w, height: h, frames, keys, ..Default::default() };
    Ok(Lowered { prog, rows, page })
}

/// Render an HTML composition on the GPU path (see the module): OUT as for `render_file`.
pub fn render_file(edit: &str, out: &Path, opts: &crate::RenderOpts) -> Result<(), String> {
    let path = std::path::PathBuf::from(edit);
    let dir = crate::dir_of(edit);
    let out = out.to_path_buf();
    let opts = opts.clone();
    raea::api::on_render_thread(move || {
        let l = lower(&path)?;
        crate::render_with(&l.prog, &l.rows, &dir, &out, &opts, Some((l.page, path.clone())))
    })
}

/// The page painted ahead on worker threads (each its own composition: a page's scripts can't cross threads). Worker i
/// paints frames i, i+N, ... of the render's list, skipping a page state another worker has painted already, and the
/// GPU path takes them in order. An animation-heavy page repaints every frame (kinetic type: 9 ms a frame on one
/// thread), so this is where its time goes.
pub struct PagePool {
    rx: std::sync::mpsc::Receiver<(i64, u64, Option<Vec<u8>>)>,
    early: std::collections::BTreeMap<i64, (u64, Option<Vec<u8>>)>,
    _workers: Vec<std::thread::JoinHandle<()>>,
}

impl PagePool {
    pub fn start(path: &Path, frames: Vec<i64>, fps: f64, workers: usize) -> PagePool {
        let (tx, rx) = std::sync::mpsc::sync_channel(workers * 4);
        let seen = std::sync::Arc::new(std::sync::Mutex::new(std::collections::HashSet::<u64>::new()));
        let mut handles = Vec::new();
        for w in 0..workers {
            let (tx, seen, path, mine) = (tx.clone(), seen.clone(), path.to_path_buf(), frames.iter().skip(w).step_by(workers).copied().collect::<Vec<_>>());
            let h = std::thread::Builder::new().name(format!("page-{w}")).stack_size(64 << 20).spawn(move || {
                let page = (|| -> Result<raea::scene::Composition, String> {
                    let o = raea::api::RenderOptions::from_json(None)?;
                    let (html, dir) = raea::api::resolve_input(&path)?;
                    let fps = o.fps.unwrap_or_else(|| raea::api::default_fps(&html));
                    let mut c = raea::api::probe(&o, &html, &dir, fps)?.comp;
                    c.host_media()?;
                    Ok(c)
                })();
                let Ok(page) = page else { return };
                // each worker's page in Bevy's schedule (raea::stage): its frames are runs of the schedule
                let mut page = raea::stage::Stage::new(page);
                for f in mine {
                    let t = f as f64 / fps;
                    let key = page.comp().page_key(t).rotate_left(1);
                    let fresh = seen.lock().map(|mut s| s.insert(key)).unwrap_or(true);
                    let px = fresh.then(|| {
                        let mut px = Vec::new();
                        page.frame(t, &mut px);
                        px
                    });
                    if tx.send((f, key, px)).is_err() {
                        return;
                    }
                }
            });
            if let Ok(h) = h {
                handles.push(h);
            }
        }
        PagePool { rx, early: Default::default(), _workers: handles }
    }

    /// Frame f's page key and, unless another frame painted that state first, its pixels. None: the pool has stopped.
    pub fn take(&mut self, f: i64) -> Option<(u64, Option<Vec<u8>>)> {
        loop {
            if let Some(x) = self.early.remove(&f) {
                return Some(x);
            }
            let (g, key, px) = self.rx.recv().ok()?;
            if g == f {
                return Some((key, px));
            }
            self.early.insert(g, (key, px));
        }
    }
}
