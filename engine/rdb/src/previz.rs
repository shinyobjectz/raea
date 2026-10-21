//! Previz: the edit seen without the renderer. The rows already say what every frame shows, so a frame can be
//! sketched straight from them: clips sampled from small proxies, captions and titles as boxes, all as whole-array
//! gathers (K's way) at a tenth of the size. Then hypervectors on both sides of the picture: one from the rows (what
//! the edit says, HDC role-filler binding) and one from the proxy pixels (what you would see, a sign projection). The
//! two together answer questions an agent asks before paying for a render: which frames make a storyboard, which
//! authored cuts don't read as cuts, which frames a snippet changed.

use std::collections::{BTreeMap, HashMap};
use std::path::Path;

use crate::eval::Tup;
use crate::ir::{Program, V};
use crate::k::{self, Hv};

pub const PW: usize = 144;
pub const PH: usize = 256;
const BITS: usize = 2048;

/// One proxy picture: PW x PH... or any size, RGB8 row-major.
#[derive(Clone)]
pub struct Pic {
    pub w: usize,
    pub h: usize,
    pub rgb: Vec<u8>,
}

/// A source's needed frames, decoded once, shrunk to proxies (box filter), by source frame index.
pub struct Proxies {
    pub by: HashMap<String, (f64, BTreeMap<usize, Pic>)>,
    pub decoded: usize,
}

fn num(v: &BTreeMap<String, V>, k: &str, d: f64) -> f64 {
    v.get(k).map(V::num).unwrap_or(d)
}

pub(crate) fn showing(t: &Tup, f: i64) -> bool {
    f as f64 >= num(&t.vals, "from", f64::MIN) && (f as f64) < num(&t.vals, "to", f64::MAX) && num(&t.vals, "opacity", 1.0) > 0.0
}

/// Shrink RGBA to a proxy whose longer side fits `long` (box average: every source pixel counts once).
fn shrink(rgba: &[u8], w: usize, h: usize, long: usize) -> Pic {
    let s = (w.max(h) as f64 / long as f64).max(1.0);
    let (pw, ph) = (((w as f64 / s).round() as usize).max(1), ((h as f64 / s).round() as usize).max(1));
    let mut acc = vec![0u32; pw * ph * 3];
    let mut cnt = vec![0u32; pw * ph];
    // the column of target x for every source x, and of target y for every source y: a gather plan, then one pass
    let tx: Vec<usize> = (0..w).map(|x| (x * pw / w).min(pw - 1)).collect();
    let ty: Vec<usize> = (0..h).map(|y| (y * ph / h).min(ph - 1)).collect();
    for y in 0..h {
        let row = &rgba[y * w * 4..(y + 1) * w * 4];
        let base = ty[y] * pw;
        for x in 0..w {
            let o = base + tx[x];
            acc[o * 3] += row[x * 4] as u32;
            acc[o * 3 + 1] += row[x * 4 + 1] as u32;
            acc[o * 3 + 2] += row[x * 4 + 2] as u32;
            cnt[o] += 1;
        }
    }
    let rgb = acc.iter().enumerate().map(|(i, a)| (a / cnt[i / 3].max(1)) as u8).collect();
    Pic { w: pw, h: ph, rgb }
}

/// The media rows (they're in every frame's state; frame 0's will do).
fn inserted_of_states(states: &[Vec<Tup>]) -> Vec<Tup> {
    states.first().map(|s| s.iter().filter(|t| t.kind == "media").cloned().collect()).unwrap_or_default()
}

/// A real frame at the proxy size (for fidelity).
pub fn shrink_pub(rgba: &[u8], w: usize, h: usize) -> Pic {
    shrink(rgba, w, h, PH)
}

/// Every frame's state, once (pure; the rules run here, nothing renders).
pub fn states(prog: &Program, rows: &[Tup]) -> Result<Vec<Vec<Tup>>, String> {
    let tl = crate::timeline::Timeline::new(prog, rows)?;
    Ok((0..prog.frames).map(|f| tl.tups(f)).collect())
}

/// The decode plan as a query over the states: the distinct (source, source frame) pairs any visible clip needs,
/// decoded in source order, each exactly once.
pub fn proxies(prog: &Program, states: &[Vec<Tup>], dir: &Path, backend: &str) -> Result<Proxies, String> {
    let mut px = Proxies { by: HashMap::new(), decoded: 0 };
    extend(&mut px, prog, states, dir, backend)?;
    Ok(px)
}

/// Decode only the (source, frame) pairs the states need that `px` doesn't hold yet (a snippet's new needs).
pub fn extend(px: &mut Proxies, prog: &Program, states: &[Vec<Tup>], dir: &Path, backend: &str) -> Result<(), String> {
    let mut need: BTreeMap<String, Vec<f64>> = BTreeMap::new();
    for (f, st) in states.iter().enumerate() {
        for t in st.iter().filter(|t| t.kind == "clip" && showing(t, f as i64)) {
            let Some(V::S(src)) = t.vals.get("src") else { continue };
            let at = num(&t.vals, "src_in", 0.0) + (f as f64 - num(&t.vals, "from", 0.0)) / prog.fps;
            need.entry(src.clone()).or_default().push(at);
        }
    }
    let media = raea_media::backend(backend)?;
    for (src, mut times) in need {
        times.sort_by(f64::total_cmp);
        let mut dec: Option<Box<dyn raea_media::VideoDecoder>> = None;
        let fps = match px.by.get(&src) {
            Some((fps, _)) => *fps,
            None => {
                let d = media.open_video(&crate::engine::resolve_src(&inserted_of_states(states), &src, dir)?)?;
                let fps = d.info().fps;
                dec = Some(d);
                px.by.insert(src.clone(), (fps, BTreeMap::new()));
                fps
            }
        };
        for t in times {
            let i = (t * fps + 1e-6).floor().max(0.0) as usize;
            if px.by[&src].1.contains_key(&i) {
                continue;
            }
            if dec.is_none() {
                dec = Some(media.open_video(&crate::engine::resolve_src(&inserted_of_states(states), &src, dir)?)?);
            }
            let fr = dec.as_mut().unwrap().frame_at(i as f64 / fps)?;
            px.by.get_mut(&src).unwrap().1.insert(i, shrink(&fr.rgba, fr.width as usize, fr.height as usize, PH));
            px.decoded += 1;
        }
    }
    Ok(())
}

fn colour(v: Option<&V>) -> Option<[f32; 4]> {
    let Some(V::S(s)) = v else { return None };
    let s = s.trim();
    if let Some(h) = s.strip_prefix('#') {
        let b = |i: usize| u8::from_str_radix(h.get(i..i + 2)?, 16).ok().map(|x| x as f32);
        return Some([b(0)?, b(2)?, b(4)?, if h.len() == 8 { b(6)? / 255.0 } else { 1.0 }]);
    }
    if let Some(inner) = s.strip_prefix("rgba(").and_then(|x| x.strip_suffix(')')) {
        let p: Vec<f32> = inner.split(',').filter_map(|x| x.trim().parse().ok()).collect();
        if p.len() == 4 {
            return Some([p[0], p[1], p[2], p[3]]);
        }
    }
    None
}

/// Fill a rectangle (proxy pixels) with a colour at an alpha: one vectorised blend over the rect's rows.
fn fill(p: &mut Pic, x0: f64, y0: f64, x1: f64, y1: f64, c: [f32; 4]) {
    let (x0, x1) = (x0.max(0.0).round() as usize, (x1.round().max(0.0) as usize).min(p.w));
    let (y0, y1) = (y0.max(0.0).round() as usize, (y1.round().max(0.0) as usize).min(p.h));
    let a = c[3].clamp(0.0, 1.0);
    for y in y0..y1 {
        for px in p.rgb[(y * p.w + x0) * 3..(y * p.w + x1.max(x0)) * 3].chunks_mut(3) {
            for k in 0..3 {
                px[k] = (px[k] as f32 * (1.0 - a) + c[k] * a).round() as u8;
            }
        }
    }
}

/// One previz frame from its state: clips by z (then id), cover-cropped and zoomed exactly as the engine does,
/// sampled from the proxies; then text rows as boxes in their colours.
pub fn sketch(prog: &Program, st: &[Tup], f: i64, px: &Proxies) -> Pic {
    let s = PW as f64 / prog.width as f64;
    let mut out = Pic { w: PW, h: PH, rgb: vec![0; PW * PH * 3] };
    let mut clips: Vec<&Tup> = st.iter().filter(|t| t.kind == "clip" && showing(t, f)).collect();
    clips.sort_by(|a, b| num(&a.vals, "z", 0.0).total_cmp(&num(&b.vals, "z", 0.0)).then(a.id.cmp(&b.id)));
    for t in clips {
        let v = &t.vals;
        let Some(V::S(src)) = v.get("src") else { continue };
        let Some((fps, pics)) = px.by.get(src) else { continue };
        let at = num(v, "src_in", 0.0) + (f as f64 - num(v, "from", 0.0)) / prog.fps;
        let Some((_, pic)) = pics.range(..=((at * fps + 1e-6).floor().max(0.0) as usize)).next_back() else { continue };
        let (bx, by, bw, bh) = (num(v, "x", 0.0) * s, num(v, "y", 0.0) * s, num(v, "w", prog.width as f64) * s, num(v, "h", prog.height as f64) * s);
        let (sw, sh) = (pic.w as f64, pic.h as f64);
        let zoom = num(v, "zoom", 1.0).max(0.01);
        let (cx, cy, cw, ch) = crate::engine::cover(sw, sh, bw, bh, zoom, (num(v, "pan_x", 0.0), num(v, "pan_y", 0.0)));
        let a = num(v, "opacity", 1.0).clamp(0.0, 1.0) as f32;
        // the gather maps: target column -> source column, target row -> source row
        let xs: Vec<(usize, usize)> = (bx.max(0.0) as usize..((bx + bw) as usize).min(PW))
            .map(|x| (x, (((x as f64 + 0.5 - bx) / bw * cw + cx - cw / 2.0) as usize).min(pic.w - 1)))
            .collect();
        for y in by.max(0.0) as usize..((by + bh) as usize).min(PH) {
            let sy = (((y as f64 + 0.5 - by) / bh * ch + cy - ch / 2.0) as usize).min(pic.h - 1);
            for &(x, sx) in &xs {
                let (o, i) = ((y * PW + x) * 3, (sy * pic.w + sx) * 3);
                for k in 0..3 {
                    out.rgb[o + k] = (out.rgb[o + k] as f32 * (1.0 - a) + pic.rgb[i + k] as f32 * a).round() as u8;
                }
            }
        }
    }
    let mut texts: Vec<&Tup> = st.iter().filter(|t| matches!(t.kind.as_str(), "caption" | "title" | "text") && showing(t, f)).collect();
    texts.sort_by(|a, b| a.id.cmp(&b.id));
    for t in texts {
        let v = &t.vals;
        let text = match v.get("text") {
            Some(V::S(x)) => x.clone(),
            _ => String::new(),
        };
        let size = num(v, "size", 48.0);
        let pad = num(v, "padding", 0.0);
        let (x, y, w) = (num(v, "x", 0.0), num(v, "y", 0.0), num(v, "w", prog.width as f64));
        let a = num(v, "opacity", 1.0).clamp(0.0, 1.0);
        // lines by a mean glyph width of 0.55 em; each line a bar of the text's colour, 0.62 em tall
        let per_line = ((w - 2.0 * pad) / (size * 0.5)).max(1.0) as usize;
        let lines = text.chars().count().div_ceil(per_line).max(1);
        let h = lines as f64 * size * 1.2 + 2.0 * pad;
        if let Some(mut bg) = colour(v.get("bg")) {
            bg[3] *= a as f32;
            fill(&mut out, x * s, y * s, (x + w) * s, (y + h) * s, bg);
        }
        let mut fg = colour(v.get("color")).unwrap_or([255.0, 255.0, 255.0, 1.0]);
        fg[3] *= 0.9 * a as f32;
        let chars: Vec<char> = text.chars().collect();
        for l in 0..lines {
            let n = chars.len().saturating_sub(l * per_line).min(per_line) as f64;
            let lw = (n * size * 0.5).min(w - 2.0 * pad);
            let lx = if matches!(v.get("align"), Some(V::S(a)) if a == "center") { x + (w - lw) / 2.0 } else { x + pad };
            let ly = y + pad + l as f64 * size * 1.2 + size * 0.3;
            fill(&mut out, lx * s, ly * s, (lx + lw) * s, (ly + size * 0.62) * s, fg);
        }
    }
    out
}

// === hypervectors ===

pub(crate) fn sym(name: &str) -> Hv {
    let mut h = 0xcbf2_9ce4_8422_2325u64;
    for b in name.bytes() {
        h ^= b as u64;
        h = h.wrapping_mul(0x100_0000_01b3);
    }
    k::random_hv(BITS, h)
}

/// A level hypervector for x in [0, 1]: a thermometer over half the bits, so near values are near vectors and the
/// ends are orthogonal.
pub(crate) fn level(col: &str, x: f64) -> Hv {
    let base = sym(&format!("level:{col}"));
    let flip = sym(&format!("flip:{col}"));
    let q = (x.clamp(0.0, 1.0) * (BITS / 2) as f64) as usize;
    let mut out = base.0.clone();
    for i in 0..q {
        let bit = 1u64 << (i % 64);
        out[i / 64] = (out[i / 64] & !bit) | (flip.0[i / 64] & bit);
    }
    Hv(out)
}

/// What the edit says a frame is: each visible row is its identity bound to the bundle of its columns (each column
/// bound to its value), and the frame is the bundle of its rows. A clip's place in its source counts as a column, so a
/// shot drifts slowly and a cut jumps.
pub fn row_hv(prog: &Program, st: &[Tup], f: i64) -> Hv {
    let mut rows = Vec::new();
    for t in st.iter().filter(|t| showing(t, f)) {
        if !matches!(t.kind.as_str(), "clip" | "caption" | "title" | "text" | "image" | "box") {
            continue;
        }
        let v = &t.vals;
        let mut cols = Vec::new();
        for (c, x) in v {
            if matches!(c.as_str(), "from" | "to" | "id" | "src_in" | "gain" | "sound") {
                continue;
            }
            let val = match x {
                V::S(s) => sym(&format!("{c}={s}")),
                V::N(n) => {
                    let span = match c.as_str() {
                        "x" | "w" => prog.width as f64,
                        "y" | "h" => prog.height as f64,
                        "size" => 200.0,
                        "zoom" => 2.0,
                        "z" => 10.0,
                        _ => 1.0,
                    };
                    level(c, n / span)
                }
            };
            cols.push(k::bind(&sym(&format!("col:{c}")), &val));
        }
        if t.kind == "clip" {
            let at = num(v, "src_in", 0.0) + (f as f64 - num(v, "from", 0.0)) / prog.fps;
            let src = match v.get("src") {
                Some(V::S(s)) => s.as_str(),
                _ => "",
            };
            // the source time on a 4 s scale per source: within a shot, frames a second apart stay similar
            let lv = level(&format!("t:{src}"), (at / 4.0).fract());
            let turns = sym(&format!("turn:{src}:{}", (at / 4.0).floor() as i64));
            cols.push(k::bind(&sym("col:at"), &k::bind(&lv, &turns)));
        }
        if cols.is_empty() {
            continue;
        }
        rows.push(k::bind(&sym(&format!("{}:{}", t.kind, t.id)), &k::majority(&cols)));
    }
    if rows.is_empty() {
        return sym("empty frame");
    }
    k::majority(&rows)
}

/// What a frame looks like, as 64 words (2048 bits): kgpu's look_grid.k and look_proj.k on the CPU, the same integer
/// function bit for bit (one look, wherever it runs; previz.feature's B1). The frame is a 9 x 16 grid of cell sums
/// (cw x ch pixels each, the remainder dropped) in three channels: luma r+g+b, red-green, and (r+g)/2 - blue; luma is
/// centred on the frame's mean cell; each bit is the sign of the features weighted +1/-1 by a seeded integer hash.
pub fn look_words(pw: usize, ph: usize, px: impl Fn(usize, usize) -> (i32, i32, i32)) -> [u32; 64] {
    let (cw, ch) = (pw / 9, ph / 16);
    let mut g = [0i32; 432];
    for c in 0..144 {
        let (cx, cy) = (c % 9, c / 9);
        let mut sums = [0i32; 3];
        for j in 0..cw * ch {
            let (r, gg, b) = px(cx * cw + j % cw, cy * ch + j / cw);
            sums[0] += r + gg + b;
            sums[1] += r - gg;
            sums[2] += ((r + gg) >> 1) - b;
        }
        g[c * 3..c * 3 + 3].copy_from_slice(&sums);
    }
    let m = (0..144).map(|c| g[c * 3]).sum::<i32>() / 144;
    let mut words = [0u32; 64];
    for (wd, word) in words.iter_mut().enumerate() {
        for bit in 0..32 {
            let mut dot = 0i32;
            for (k, gk) in g.iter().enumerate() {
                let h = ((((wd * 32 + bit) as i32).wrapping_mul(73856093) ^ (k as i32).wrapping_mul(19349663)).wrapping_add(40503)).wrapping_mul(-1640531535);
                let w = 2 * ((h >> 15) & 1) - 1;
                dot = dot.wrapping_add((gk - if k % 3 == 0 { m } else { 0 }).wrapping_mul(w));
            }
            if dot > 0 {
                *word |= 1 << bit;
            }
        }
    }
    words
}

/// The look's words as a hypervector (word 2i in the low half of u64 i, as the GPU's buffer lies).
pub fn look_hv(words: &[u32; 64]) -> Hv {
    Hv((0..32).map(|i| words[2 * i] as u64 | (words[2 * i + 1] as u64) << 32).collect())
}

/// What a frame looks like (look_words on a sketch).
pub struct Look;

impl Look {
    pub fn new() -> Look {
        Look
    }
    pub fn hv(&self, p: &Pic) -> Hv {
        look_hv(&look_words(p.w, p.h, |x, y| {
            let i = (y * p.w + x) * 3;
            (p.rgb[i] as i32, p.rgb[i + 1] as i32, p.rgb[i + 2] as i32)
        }))
    }
}

/// What each frame shows by content (docs/the roadmap B2): the top visible clip's SigLIP 2 row at its place in its source,
/// from the source's ingest (`raea-perceive ingest`, keyed by the same sha-256 as the edit's media ids), and that row
/// signed onto BITS seeded hyperplanes. None where the frame shows no clip or its source isn't ingested; text and
/// overlays are what the rows' hypervector says, so only the footage is read here.
pub struct Content {
    pub vectors: Vec<Option<Vec<f32>>>,
    pub hvs: Vec<Option<Hv>>,
    /// sources the edit shows that have no ingest yet
    pub missing: Vec<String>,
}

/// The ingest store: $RAEA_INGEST, else ~/.cache/raea-ingest (ai/perception/src/ingest.rs).
pub(crate) fn ingest_store() -> std::path::PathBuf {
    std::env::var_os("RAEA_INGEST")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| std::path::PathBuf::from(std::env::var("HOME").unwrap_or_default()).join(".cache/raea-ingest"))
}

/// Media rows named by the agent ('take') rather than by their hash: id -> the file's sha256, so their ingest is found.
/// Registered by the entry points that know the edit's folder (energy, the previz report); hashes cached by file.
fn media_keys() -> &'static std::sync::Mutex<HashMap<String, String>> {
    static M: std::sync::OnceLock<std::sync::Mutex<HashMap<String, String>>> = std::sync::OnceLock::new();
    M.get_or_init(Default::default)
}

pub(crate) fn register_media(rows: &[Tup], dir: &std::path::Path) {
    static BY_FILE: std::sync::OnceLock<std::sync::Mutex<HashMap<(std::path::PathBuf, u64, std::time::SystemTime), String>>> = std::sync::OnceLock::new();
    for t in rows.iter().filter(|t| t.kind == "media") {
        if t.id.len() >= 16 && t.id.chars().all(|c| c.is_ascii_hexdigit()) {
            continue;
        }
        let Ok(path) = crate::engine::resolve_src(rows, &format!("media:{}", t.id), dir) else { continue };
        let Ok(meta) = std::fs::metadata(&path) else { continue };
        let k = (path.clone(), meta.len(), meta.modified().unwrap_or(std::time::UNIX_EPOCH));
        let cache = BY_FILE.get_or_init(Default::default);
        let known = cache.lock().unwrap().get(&k).cloned();
        let hash = match known {
            Some(h) => h,
            None => {
                use sha2::{Digest, Sha256};
                let Ok(mut f) = std::fs::File::open(&path) else { continue };
                let mut h = Sha256::new();
                if std::io::copy(&mut f, &mut h).is_err() {
                    continue;
                }
                let h = format!("{:x}", h.finalize());
                cache.lock().unwrap().insert(k, h.clone());
                h
            }
        };
        media_keys().lock().unwrap().insert(t.id.clone(), hash);
    }
}

/// A source's ingest folder: by its hash, or by the hash of the file a named media row points at.
pub(crate) fn ingest_dir(key: &str) -> Option<std::path::PathBuf> {
    let hash = media_keys().lock().unwrap().get(key).cloned().unwrap_or_else(|| key.to_string());
    Some(ingest_store().join(hash.get(..16)?))
}

/// A source's SigLIP 2 column from its ingest: (rows a second, dim, the floats).
fn siglip_column(hash: &str) -> Option<(f64, usize, Vec<f32>)> {
    let dir = ingest_dir(&hash)?;
    let meta: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(dir.join("siglip.json")).ok()?).ok()?;
    let bytes = std::fs::read(dir.join("siglip.f32")).ok()?;
    let data: Vec<f32> = bytes.chunks_exact(4).map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]])).collect();
    Some((meta["fps"].as_f64()?, meta["dim"].as_u64()? as usize, data))
}

pub fn content(prog: &Program, states: &[Vec<Tup>]) -> Content {
    let mut cols: HashMap<String, Option<(f64, usize, Vec<f32>)>> = HashMap::new();
    let proj = k::Projection::new(768, BITS, 0x5eed_c0_47e7);
    let (mut vectors, mut hvs, mut missing) = (Vec::new(), Vec::new(), Vec::new());
    for (f, st) in states.iter().enumerate() {
        // the clip drawn last (by z, then id) is the one on top
        let top = st
            .iter()
            .filter(|t| t.kind == "clip" && showing(t, f as i64))
            .max_by(|a, b| num(&a.vals, "z", 0.0).total_cmp(&num(&b.vals, "z", 0.0)).then(a.id.cmp(&b.id)));
        let row = top.and_then(|t| {
            let Some(V::S(src)) = t.vals.get("src") else { return None };
            let hash = src.strip_prefix("media:").unwrap_or(src).to_string();
            let col = cols.entry(hash.clone()).or_insert_with(|| siglip_column(&hash));
            if col.is_none() && !missing.contains(&hash) {
                missing.push(hash.clone());
            }
            let (fps, d, data) = col.as_ref()?;
            let at = num(&t.vals, "src_in", 0.0) + (f as f64 - num(&t.vals, "from", 0.0)) / prog.fps;
            let n = data.len() / d;
            let i = ((at * fps).floor().max(0.0) as usize).min(n.saturating_sub(1));
            Some(data[i * d..(i + 1) * d].to_vec())
        });
        hvs.push(row.as_ref().map(|v| proj.hv(v)));
        vectors.push(row);
    }
    Content { vectors, hvs, missing }
}

/// A source's faces through time from its ingest (faces.json): (samples a second, the source's width and height from
/// facts.json, each sample's faces and person box, 0..1 of the source).
pub struct FaceTrack {
    fps: f64,
    sw: f64,
    sh: f64,
    frames: Vec<(Vec<[f64; 4]>, Option<[f64; 4]>)>,
}

fn face_track(hash: &str) -> Option<FaceTrack> {
    let dir = ingest_dir(&hash)?;
    let j: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(dir.join("faces.json")).ok()?).ok()?;
    let facts: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(dir.join("facts.json")).ok()?).ok()?;
    let b = |v: &serde_json::Value| -> Option<[f64; 4]> { let a = v.as_array()?; Some([a.first()?.as_f64()?, a.get(1)?.as_f64()?, a.get(2)?.as_f64()?, a.get(3)?.as_f64()?]) };
    let raw = j["frames"].as_array()?;
    // the confirmed faces when the ingest has them (perception's confirm_faces: on after two confident sightings, held
    // through short gaps, the box smoothed), so a one-sample detection or dropout doesn't move what follows a face; the
    // raw detections otherwise (an older ingest)
    let confirmed: Option<Vec<Vec<[f64; 4]>>> = j["tracks"].as_array().map(|tracks| {
        (0..raw.len()).map(|k| tracks.iter().filter_map(|t| {
            let s = &t["samples"][k];
            if s["visible"].as_bool() == Some(true) { b(&s["box"]) } else { None }
        }).collect()).collect()
    });
    let frames = raw.iter().enumerate().map(|(k, f)| {
        let faces = match &confirmed {
            Some(c) => c[k].clone(),
            None => f["faces"].as_array().map(|a| a.iter().filter_map(b).collect()).unwrap_or_default(),
        };
        (faces, b(&f["person"]))
    }).collect();
    Some(FaceTrack { fps: j["fps"].as_f64()?, sw: facts["width"].as_f64()?, sh: facts["height"].as_f64()?, frames })
}

/// What a frame holds for the layout scorer, in canvas fractions: each visible face (with the person around it, when
/// the mask found one) and each text box with its word count. Faces come from the sources' face tracks through each
/// clip's cover, zoom and pan; a face under a clip drawn above it is hidden.
#[derive(Clone, Debug, Default)]
pub struct Zones {
    pub faces: Vec<([f64; 4], Option<[f64; 4]>)>,
    pub texts: Vec<([f64; 4], u32)>,
}

pub fn zones(prog: &Program, st: &[Tup], f: i64, tracks: &mut HashMap<String, Option<FaceTrack>>) -> Zones {
    let (cwid, chei) = (prog.width as f64, prog.height as f64);
    let mut clips: Vec<&Tup> = st.iter().filter(|t| t.kind == "clip" && showing(t, f)).collect();
    clips.sort_by(|a, b| num(&a.vals, "z", 0.0).total_cmp(&num(&b.vals, "z", 0.0)).then(a.id.cmp(&b.id)));
    let boxes: Vec<[f64; 4]> = clips.iter().map(|t| [num(&t.vals, "x", 0.0), num(&t.vals, "y", 0.0), num(&t.vals, "w", cwid), num(&t.vals, "h", chei)]).collect();
    let mut out = Zones::default();
    for (ci, t) in clips.iter().enumerate() {
        let v = &t.vals;
        let Some(V::S(src)) = v.get("src") else { continue };
        let hash = src.strip_prefix("media:").unwrap_or(src).to_string();
        let Some(tr) = tracks.entry(hash.clone()).or_insert_with(|| face_track(&hash)).as_ref() else { continue };
        let at = num(v, "src_in", 0.0) + (f as f64 - num(v, "from", 0.0)) / prog.fps;
        let i = ((at * tr.fps).round().max(0.0) as usize).min(tr.frames.len().saturating_sub(1));
        let Some((faces, person)) = tr.frames.get(i) else { continue };
        let [bx, by, bw, bh] = boxes[ci];
        let zoom = num(v, "zoom", 1.0).max(0.01);
        let (cx, cy, cw, ch) = crate::engine::cover(tr.sw, tr.sh, bw, bh, zoom, (num(v, "pan_x", 0.0), num(v, "pan_y", 0.0)));
        // a box 0..1 of the source as canvas fractions (not clamped: a face cut by the frame reaches past its edge)
        let map = |r: &[f64; 4]| -> [f64; 4] {
            let x0 = bx + (r[0] * tr.sw - (cx - cw / 2.0)) / cw * bw;
            let y0 = by + (r[1] * tr.sh - (cy - ch / 2.0)) / ch * bh;
            [x0 / cwid, y0 / chei, r[2] * tr.sw / cw * bw / cwid, r[3] * tr.sh / ch * bh / chei]
        };
        for fb in faces {
            let m = map(fb);
            let (mx, my) = ((m[0] + m[2] / 2.0) * cwid, (m[1] + m[3] / 2.0) * chei);
            // shown only inside its own clip's box, and not under a clip drawn above it
            let inside = |b: &[f64; 4]| mx >= b[0] && mx <= b[0] + b[2] && my >= b[1] && my <= b[1] + b[3];
            if !inside(&boxes[ci]) || boxes[ci + 1..].iter().any(inside) {
                continue;
            }
            out.faces.push((m, person.as_ref().map(map)));
        }
    }
    for t in st.iter().filter(|t| matches!(t.kind.as_str(), "caption" | "title" | "text") && showing(t, f)) {
        let v = &t.vals;
        let text = match v.get("text") { Some(V::S(x)) => x.clone(), _ => String::new() };
        if text.trim().is_empty() {
            continue;
        }
        let (size, pad) = (num(v, "size", 48.0), num(v, "padding", 0.0));
        let (x, y, w) = (num(v, "x", 0.0), num(v, "y", 0.0), num(v, "w", cwid));
        // the words' own extent, line by line as the sketch draws them (a centred caption is narrower than its box)
        let per_line = ((w - 2.0 * pad) / (size * 0.5)).max(1.0) as usize;
        let n = text.chars().count();
        let lines = n.div_ceil(per_line).max(1);
        let (mut x0, mut x1) = (f64::MAX, f64::MIN);
        for l in 0..lines {
            let lw = (n.saturating_sub(l * per_line).min(per_line) as f64 * size * 0.5).min(w - 2.0 * pad);
            let lx = if matches!(v.get("align"), Some(V::S(a)) if a == "center") { x + (w - lw) / 2.0 } else { x + pad };
            (x0, x1) = (x0.min(lx), x1.max(lx + lw));
        }
        let (y0, y1) = (y + pad + size * 0.3, y + pad + (lines - 1) as f64 * size * 1.2 + size * 0.92);
        out.texts.push(([x0 / cwid, y0 / chei, (x1 - x0) / cwid, (y1 - y0) / chei], text.split_whitespace().count() as u32));
    }
    out
}

/// A frame's zones as the layout scorer's layout (engine/html/src/previz.rs): each face a subject (its person box, else a
/// box around the face) and each text box a text zone.
pub fn layout_of(prog: &Program, z: &Zones) -> raea::previz::Layout {
    let ratio = if prog.width * 16 == prog.height * 9 { "9:16" } else if prog.width == prog.height { "1:1" } else if prog.width * 5 == prog.height * 4 { "4:5" } else { "16:9" };
    let rect = |r: &[f64; 4]| serde_json::json!({"x": r[0], "y": r[1], "w": r[2], "h": r[3]});
    let mut zones = Vec::new();
    for (face, person) in &z.faces {
        // the person mask's box when this face is in it (the mask is one person), else a box around the face
        let (fx, fy) = (face[0] + face[2] / 2.0, face[1] + face[3] / 2.0);
        let around = [face[0] - 1.2 * face[2], face[1] - 0.6 * face[3], 3.4 * face[2], 4.6 * face[3]];
        let body = person
            .filter(|p| fx >= p[0] && fx <= p[0] + p[2] && fy >= p[1] && fy <= p[1] + p[3])
            .map(|p| {
                // the person's box, grown to hold the whole face
                let (x0, y0) = (p[0].min(face[0]), p[1].min(face[1]));
                [x0, y0, (p[0] + p[2]).max(face[0] + face[2]) - x0, (p[1] + p[3]).max(face[1] + face[3]) - y0]
            })
            .unwrap_or(around);
        let mut zone = rect(&body);
        zone["role"] = "subject".into();
        zone["face"] = rect(face);
        zones.push(zone);
    }
    for (r, words) in &z.texts {
        let mut zone = rect(r);
        zone["role"] = "text".into();
        zone["words"] = (*words).into();
        zones.push(zone);
    }
    serde_json::from_value(serde_json::json!({"ratio": ratio, "zones": zones})).expect("a layout")
}

/// Outlines a box (canvas fractions) on a sketch.
pub fn outline(p: &mut Pic, r: &[f64; 4], c: [u8; 3]) {
    let (x0, y0) = ((r[0] * p.w as f64).round() as i64, (r[1] * p.h as f64).round() as i64);
    let (x1, y1) = (((r[0] + r[2]) * p.w as f64).round() as i64, ((r[1] + r[3]) * p.h as f64).round() as i64);
    let mut put = |x: i64, y: i64| {
        if x >= 0 && y >= 0 && (x as usize) < p.w && (y as usize) < p.h {
            let i = (y as usize * p.w + x as usize) * 3;
            p.rgb[i..i + 3].copy_from_slice(&c);
        }
    };
    for x in x0..=x1 {
        put(x, y0);
        put(x, y1);
    }
    for y in y0..=y1 {
        put(x0, y);
        put(x1, y);
    }
}

/// A source's attention through time from its ingest (attention.json): (samples a second, the source's size, each
/// sample's centre, peak and focus).
pub struct AttentionTrack {
    fps: f64,
    sw: f64,
    sh: f64,
    frames: Vec<([f64; 2], [f64; 2], f64)>,
    /// each sample's rival (the catalogue's focal point): the strongest place 0.25 or more from the strongest, and its
    /// mass over that one's; None in tracks ingested before 2026-10-15
    rivals: Vec<Option<([f64; 2], f64)>>,
}

fn attention_track(hash: &str) -> Option<AttentionTrack> {
    let dir = ingest_dir(&hash)?;
    let j: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(dir.join("attention.json")).ok()?).ok()?;
    let facts: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(dir.join("facts.json")).ok()?).ok()?;
    let p = |v: &serde_json::Value| -> Option<[f64; 2]> { Some([v.get(0)?.as_f64()?, v.get(1)?.as_f64()?]) };
    let kept: Vec<&serde_json::Value> = j["frames"].as_array()?.iter().filter(|f| p(&f["centre"]).is_some() && p(&f["peak"]).is_some() && f["focus"].is_f64()).collect();
    let frames = kept.iter().filter_map(|f| Some((p(&f["centre"])?, p(&f["peak"])?, f["focus"].as_f64()?))).collect();
    let rivals = kept.iter().map(|f| Some((p(&f["rival"])?, f["rival_ratio"].as_f64()?))).collect();
    Some(AttentionTrack { fps: j["fps"].as_f64()?, sw: facts["width"].as_f64()?, sh: facts["height"].as_f64()?, frames, rivals })
}

/// Where the eye goes in a frame (docs/the roadmap G1): the top clip's attention centre and peak mapped onto the canvas
/// (fractions) through its cover, zoom and pan, and the source's focus there; None without an attention track.
pub fn attention(prog: &Program, st: &[Tup], f: i64, tracks: &mut HashMap<String, Option<AttentionTrack>>) -> Option<([f64; 2], [f64; 2], f64)> {
    let (cwid, chei) = (prog.width as f64, prog.height as f64);
    let t = st
        .iter()
        .filter(|t| t.kind == "clip" && showing(t, f))
        .max_by(|a, b| num(&a.vals, "z", 0.0).total_cmp(&num(&b.vals, "z", 0.0)).then(a.id.cmp(&b.id)))?;
    let v = &t.vals;
    let Some(V::S(src)) = v.get("src") else { return None };
    let hash = src.strip_prefix("media:").unwrap_or(src).to_string();
    let tr = tracks.entry(hash.clone()).or_insert_with(|| attention_track(&hash)).as_ref()?;
    let at = num(v, "src_in", 0.0) + (f as f64 - num(v, "from", 0.0)) / prog.fps;
    let (centre, peak, focus) = *tr.frames.get(((at * tr.fps).round().max(0.0) as usize).min(tr.frames.len().checked_sub(1)?))?;
    let (bx, by, bw, bh) = (num(v, "x", 0.0), num(v, "y", 0.0), num(v, "w", cwid), num(v, "h", chei));
    let (cx, cy, cw, ch) = crate::engine::cover(tr.sw, tr.sh, bw, bh, num(v, "zoom", 1.0).max(0.01), (num(v, "pan_x", 0.0), num(v, "pan_y", 0.0)));
    let map = |p: [f64; 2]| [(bx + (p[0] * tr.sw - (cx - cw / 2.0)) / cw * bw) / cwid, (by + (p[1] * tr.sh - (cy - ch / 2.0)) / ch * bh) / chei];
    Some((map(centre), map(peak), focus))
}

/// The focal point's rival in a frame (docs/the roadmap G's catalogue): the top clip's rival place mapped onto the canvas
/// the way `attention` maps the peak, and its strength over the peak's; None without a track that has rivals.
pub fn rival(prog: &Program, st: &[Tup], f: i64, tracks: &mut HashMap<String, Option<AttentionTrack>>) -> Option<([f64; 2], f64)> {
    let (cwid, chei) = (prog.width as f64, prog.height as f64);
    let t = st
        .iter()
        .filter(|t| t.kind == "clip" && showing(t, f))
        .max_by(|a, b| num(&a.vals, "z", 0.0).total_cmp(&num(&b.vals, "z", 0.0)).then(a.id.cmp(&b.id)))?;
    let v = &t.vals;
    let Some(V::S(src)) = v.get("src") else { return None };
    let hash = src.strip_prefix("media:").unwrap_or(src).to_string();
    let tr = tracks.entry(hash.clone()).or_insert_with(|| attention_track(&hash)).as_ref()?;
    let at = num(v, "src_in", 0.0) + (f as f64 - num(v, "from", 0.0)) / prog.fps;
    let (p, ratio) = (*tr.rivals.get(((at * tr.fps).round().max(0.0) as usize).min(tr.rivals.len().checked_sub(1)?))?)?;
    let (bx, by, bw, bh) = (num(v, "x", 0.0), num(v, "y", 0.0), num(v, "w", cwid), num(v, "h", chei));
    let (cx, cy, cw, ch) = crate::engine::cover(tr.sw, tr.sh, bw, bh, num(v, "zoom", 1.0).max(0.01), (num(v, "pan_x", 0.0), num(v, "pan_y", 0.0)));
    Some(([(bx + (p[0] * tr.sw - (cx - cw / 2.0)) / cw * bw) / cwid, (by + (p[1] * tr.sh - (cy - ch / 2.0)) / ch * bh) / chei], ratio))
}

/// Each platform's button bands at 1080 x 1920 (top, right, bottom, left, px), with where the numbers come from
/// (docs/the roadmap G2). No platform publishes these; they are practitioners' measurements and they disagree, so they are
/// reported side by side and averaged; the house band (engine/html/src/previz.rs › safe_bands) stays the hard rule.
pub const PLATFORM_BANDS: [(&str, [f64; 4], &str); 7] = [
    ("house", [220.0, 140.0, 420.0, 0.0], "engine/html/src/previz.rs safe_bands (advice platforms/safe-top-bottom-default)"),
    ("tiktok", [108.0, 120.0, 320.0, 60.0], "postplanify.com/blog/social-media-safe-zones-2026-complete-guide"),
    ("reels", [210.0, 84.0, 310.0, 0.0], "postplanify.com/blog/social-media-safe-zones-2026-complete-guide"),
    ("shorts", [120.0, 96.0, 300.0, 0.0], "postplanify.com/blog/social-media-safe-zones-2026-complete-guide"),
    ("tiktok-described", [75.0, 110.0, 340.0, 0.0], "daohuy.com/en/blog/post-vertical-subtitle-safe-zones-9-16 (with the description text)"),
    ("reels-described", [75.0, 110.0, 275.0, 0.0], "daohuy.com/en/blog/post-vertical-subtitle-safe-zones-9-16 (with the description text)"),
    ("shorts-described", [80.0, 100.0, 220.0, 0.0], "daohuy.com/en/blog/post-vertical-subtitle-safe-zones-9-16 (with the description text)"),
];

/// Whether a box (canvas fractions) reaches into a platform's bands (px at 1080 x 1920).
pub fn in_bands(r: &[f64; 4], bands: &[f64; 4]) -> bool {
    let [top, right, bottom, left] = [bands[0] / 1920.0, bands[1] / 1080.0, bands[2] / 1920.0, bands[3] / 1080.0];
    r[1] < top || r[0] + r[2] > 1.0 - right || r[1] + r[3] > 1.0 - bottom || r[0] < left
}

/// The thirds and the golden lines a layout's eye line and text like to sit on (the layout scorer's POWER_LINES).
pub const POWER_LINES: [f64; 4] = [1.0 / 3.0, 2.0 / 3.0, 0.382, 0.618];

/// How far a position (0..1) is from the nearest power line.
pub fn to_power_line(v: f64) -> f64 {
    POWER_LINES.iter().map(|l| (v - l).abs()).fold(f64::MAX, f64::min)
}

/// A source's light through time from its ingest (light.json): samples a second and each sample's JSON.
pub struct LightTrack {
    fps: f64,
    frames: Vec<serde_json::Value>,
}

/// The light a frame shows (docs/the roadmap G4): the top clip's light sample at its place in its source (luma, warmth,
/// the coach's light on the face); None without a light track.
pub fn light(prog: &Program, st: &[Tup], f: i64, tracks: &mut HashMap<String, Option<LightTrack>>) -> Option<serde_json::Value> {
    let t = st
        .iter()
        .filter(|t| t.kind == "clip" && showing(t, f))
        .max_by(|a, b| num(&a.vals, "z", 0.0).total_cmp(&num(&b.vals, "z", 0.0)).then(a.id.cmp(&b.id)))?;
    let Some(V::S(src)) = t.vals.get("src") else { return None };
    let hash = src.strip_prefix("media:").unwrap_or(src).to_string();
    let tr = tracks
        .entry(hash.clone())
        .or_insert_with(|| {
            let j: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(ingest_dir(&hash)?.join("light.json")).ok()?).ok()?;
            Some(LightTrack { fps: j["fps"].as_f64()?, frames: j["frames"].as_array()?.clone() })
        })
        .as_ref()?;
    let at = num(&t.vals, "src_in", 0.0) + (f as f64 - num(&t.vals, "from", 0.0)) / prog.fps;
    tr.frames.get(((at * tr.fps).round().max(0.0) as usize).min(tr.frames.len().checked_sub(1)?)).cloned()
}

/// A source's mouth through time from its ingest (mesh.json): samples a second, each sample's mouth openness (None
/// without a face).
pub fn mouth_track(hash: &str) -> Option<(f64, Vec<Option<f64>>)> {
    let j: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(ingest_dir(&hash)?.join("mesh.json")).ok()?).ok()?;
    Some((j["fps"].as_f64()?, j["frames"].as_array()?.iter().map(|f| f["mouth"].as_f64()).collect()))
}

/// A source's gaze through time from its ingest (mesh.json): samples a second, each sample's gaze yaw (None without
/// a face).
pub fn gaze_track(hash: &str) -> Option<(f64, Vec<Option<f64>>)> {
    let j: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(ingest_dir(&hash)?.join("mesh.json")).ok()?).ok()?;
    Some((j["fps"].as_f64()?, j["frames"].as_array()?.iter().map(|f| f["gaze_yaw"].as_f64()).collect()))
}

/// The clip on top at a frame: its source hash and the source time shown.
pub fn top_source(prog: &Program, st: &[Tup], f: i64) -> Option<(String, f64)> {
    let t = st
        .iter()
        .filter(|t| t.kind == "clip" && showing(t, f))
        .max_by(|a, b| num(&a.vals, "z", 0.0).total_cmp(&num(&b.vals, "z", 0.0)).then(a.id.cmp(&b.id)))?;
    let Some(V::S(src)) = t.vals.get("src") else { return None };
    Some((src.strip_prefix("media:").unwrap_or(src).to_string(), num(&t.vals, "src_in", 0.0) + (f as f64 - num(&t.vals, "from", 0.0)) / prog.fps))
}

/// k frames that cover the edit's looks: farthest-point picks (each next pick is the frame least like any picked),
/// and the worst coverage left (the largest distance from any frame to its nearest pick, in Hamming bits).
pub fn storyboard(hvs: &[Hv], kpicks: usize) -> (Vec<usize>, u32) {
    storyboard_by(hvs.len(), kpicks, &|a, b| k::hamming(&hvs[a], &hvs[b]))
}

/// Farthest-point picks under any distance between frames (e.g. what the rows say plus what the picture shows).
pub fn storyboard_by(n: usize, kpicks: usize, dist: &dyn Fn(usize, usize) -> u32) -> (Vec<usize>, u32) {
    let mut picks = vec![0usize];
    let mut nearest: Vec<u32> = (0..n).map(|j| dist(j, 0)).collect();
    while picks.len() < kpicks.min(n) {
        let (i, _) = nearest.iter().enumerate().max_by_key(|(i, d)| (**d, usize::MAX - *i)).unwrap();
        picks.push(i);
        for j in 0..n {
            nearest[j] = nearest[j].min(dist(j, i));
        }
    }
    picks.sort();
    (picks, *nearest.iter().max().unwrap())
}

/// The worst coverage of any fixed picks.
pub fn coverage(hvs: &[Hv], picks: &[usize]) -> u32 {
    hvs.iter().map(|h| picks.iter().map(|&p| k::hamming(h, &hvs[p])).min().unwrap()).max().unwrap()
}

pub fn save(p: &Pic, path: &Path) -> Result<(), String> {
    image::save_buffer(path, &p.rgb, p.w as u32, p.h as u32, image::ColorType::Rgb8).map_err(|e| format!("{}: {e}", path.display()))
}

/// Tiles in a row-major grid of `cols` columns.
pub fn sheet(pics: &[Pic], cols: usize) -> Pic {
    let (w, h) = (pics[0].w, pics[0].h);
    let rows = pics.len().div_ceil(cols);
    let mut out = Pic { w: w * cols + (cols - 1) * 4, h: h * rows + (rows - 1) * 4, rgb: vec![255; 0] };
    out.rgb = vec![255; out.w * out.h * 3];
    for (i, p) in pics.iter().enumerate() {
        let (ox, oy) = ((i % cols) * (w + 4), (i / cols) * (h + 4));
        for y in 0..h {
            let d = ((oy + y) * out.w + ox) * 3;
            out.rgb[d..d + w * 3].copy_from_slice(&p.rgb[y * w * 3..(y + 1) * w * 3]);
        }
    }
    out
}
