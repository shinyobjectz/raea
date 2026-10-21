//! Ad stills: one moment of an edit at the formats paid media and organic social use (1:1, 4:5, 9:16, 1.91:1), from
//! the same rows. The frame's state is evaluated once (rules, keys, transitions: the moment as it plays), then each
//! format gets that state reflowed: 2D rows scaled to the new canvas (text by the smaller axis, so it stays inside),
//! clips frozen at their source moment and reframed on their subject with the pan (cover's slack), sound dropped.
//!
//! The subject: a clip's own `focus_x`/`focus_y` (0..1 of its source) when set; else `RAEA_FOCUS_CMD` (a command run as
//! `<cmd> face <video> <seconds>` printing `{"box": [x, y, w, h]}`, top-left and normalized, as ugc-perceive does);
//! else a talking head's eye line (centre, 38% down).

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::eval::Tup;
use crate::ir::{Program, V};

/// A still format: its name in file names and its pixel size.
#[derive(Clone, Copy, Debug)]
pub struct Format {
    pub name: &'static str,
    pub w: u32,
    pub h: u32,
}

/// Meta's and TikTok's common placements: feed square, feed portrait, stories/reels, link ads.
pub const FORMATS: [Format; 4] = [
    Format { name: "1x1", w: 1080, h: 1080 },
    Format { name: "4x5", w: 1080, h: 1350 },
    Format { name: "9x16", w: 1080, h: 1920 },
    Format { name: "1.91x1", w: 1200, h: 628 },
];

pub fn format(name: &str) -> Option<Format> {
    // 4:5 and 1.91:1 (how agents and ad specs write them) are 4x5 and 1.91x1
    let name = name.trim().to_lowercase().replace(':', "x");
    FORMATS.iter().copied().find(|f| f.name == name)
}

const KINDS_2D: &[&str] = &["title", "text", "box", "caption", "image"];

/// A 2D block's height: its own when it has one, else its text's lines at its width (an average glyph is about 0.55 em
/// wide), so a caption narrowed beside a face counts the lines it wraps to.
fn block_h(t: &Tup, width: f64) -> f64 {
    if let Some(h) = num(t, "h") {
        return h;
    }
    let size = num(t, "size").unwrap_or(48.0);
    let chars = match t.vals.get("text") {
        Some(V::S(x)) => x.chars().count() as f64,
        _ => 1.0,
    };
    let lines = (chars * size * 0.55 / width.max(size)).ceil().max(1.0);
    lines * size * 1.2
}

/// A row inside another (its x and y are in its parent's box, not the page's).
fn nested(t: &Tup) -> bool {
    matches!(t.vals.get("parent"), Some(V::S(p)) if !p.is_empty())
}

fn num(t: &Tup, k: &str) -> Option<f64> {
    match t.vals.get(k) {
        Some(V::N(x)) => Some(*x),
        _ => None,
    }
}

/// Where a clip's subject is in its source (fractions), at `seconds` into it, and its face box when one was found
/// (x, y, w, h as fractions, top-left).
fn focus(t: &Tup, path: &Path, seconds: f64) -> ((f64, f64), Option<[f64; 4]>) {
    if let (Some(x), Some(y)) = (num(t, "focus_x"), num(t, "focus_y")) {
        return ((x, y), None);
    }
    if let Ok(cmd) = std::env::var("RAEA_FOCUS_CMD") {
        // one detection per source moment: every format of a still asks for the same one (about 330 ms each)
        static SEEN: std::sync::Mutex<Option<std::collections::HashMap<(PathBuf, i64), Option<[f64; 4]>>>> = std::sync::Mutex::new(None);
        let key = (path.to_path_buf(), (seconds * 1000.0).round() as i64);
        if let Some(hit) = SEEN.lock().ok().and_then(|m| m.as_ref().and_then(|m| m.get(&key).cloned())) {
            return match hit {
                Some(b) => ((b[0] + b[2] / 2.0, b[1] + b[3] / 2.0), Some(b)),
                None => ((0.5, 0.38), None),
            };
        }
        let found = face_box(&cmd, path, seconds);
        if let Ok(mut m) = SEEN.lock() {
            m.get_or_insert_with(Default::default).insert(key, found);
        }
        if let Some(b) = found {
            return ((b[0] + b[2] / 2.0, b[1] + b[3] / 2.0), Some(b));
        }
    }
    ((0.5, 0.38), None)
}

/// The face box a focus command reports (top-left, normalized), if any.
fn face_box(cmd: &str, path: &Path, seconds: f64) -> Option<[f64; 4]> {
    {
        let out = std::process::Command::new(cmd).arg("face").arg(path).arg(format!("{seconds:.3}")).output();
        if let Some(b) = out.ok().and_then(|o| serde_json::from_slice::<serde_json::Value>(&o.stdout).ok()).and_then(|j| j["box"].as_array().cloned()) {
            let v: Vec<f64> = b.iter().filter_map(|x| x.as_f64()).collect();
            if v.len() == 4 {
                return Some([v[0], v[1], v[2], v[3]]);
            }
        }
    }
    None
}

/// The pan that puts a source point at the box's centre (as far as cover's slack allows).
fn pan_for(sw: f64, sh: f64, w: f64, h: f64, zoom: f64, at: (f64, f64)) -> (f64, f64) {
    let (_, _, cw, ch) = crate::engine::cover(sw, sh, w, h, zoom, (0.0, 0.0));
    let axis = |s: f64, c: f64, p: f64| if s - c > 1e-6 { ((p * s - s / 2.0) / ((s - c) / 2.0)).clamp(-1.0, 1.0) } else { 0.0 };
    (axis(sw, cw, at.0), axis(sh, ch, at.1))
}

/// The frame's rows reflowed for a format, as a one-frame edit (frame 0 is the moment).
fn reflow(prog: &Program, state: &[Tup], f: i64, fm: Format, dir: &Path) -> Result<(Program, Vec<Tup>), String> {
    let (sx, sy) = (fm.w as f64 / prog.width as f64, fm.h as f64 / prog.height as f64);
    // text by the geometric mean of the axes: the smaller axis alone left a 1.91:1 caption at a third of its size
    let s = (sx * sy).sqrt().min(sx.max(sy));
    // faces in the output (pixels): text keeps off them
    let mut faces: Vec<[f64; 4]> = Vec::new();
    let backend = raea_media::backend(&crate::engine::media_backend())?;
    let mut rows = Vec::new();
    // the edit's own shape: the frame as the render draws it, nothing moved (the keep-off passes below are for other
    // shapes; at 9:16 they moved a CJ card onto the face and lost the take's push-in, an agent session 2026-10-16)
    if fm.w == prog.width && fm.h == prog.height {
        for t in state {
            if matches!(t.kind.as_str(), "audio" | "master" | "speech") {
                continue;
            }
            let on = num(t, "from").map(|a| f as f64 >= a).unwrap_or(true) && num(t, "to").map(|b| (f as f64) < b).unwrap_or(true);
            if !on {
                continue;
            }
            let mut t = t.clone();
            if t.kind == "clip" {
                let at = num(&t, "src_in").unwrap_or(0.0) + (f as f64 - num(&t, "from").unwrap_or(0.0)) / prog.fps;
                t.vals.insert("src_in".into(), V::N(at));
                t.vals.insert("sound".into(), V::S(String::new()));
                t.vals.remove("gain");
            }
            if t.vals.contains_key("from") || t.vals.contains_key("to") {
                t.vals.insert("from".into(), V::N(0.0));
                t.vals.insert("to".into(), V::N(1.0));
            }
            rows.push(t);
        }
        let p = Program { fps: prog.fps, width: fm.w, height: fm.h, frames: 1, ..Default::default() };
        return Ok((p, rows));
    }
    for t in state {
        let mut t = t.clone();
        let on = num(&t, "from").map(|a| f as f64 >= a).unwrap_or(true) && num(&t, "to").map(|b| (f as f64) < b).unwrap_or(true);
        let timed = t.vals.contains_key("from") || t.vals.contains_key("to");
        match t.kind.as_str() {
            // a still has no sound
            "audio" | "master" | "speech" => continue,
            "clip" => {
                if !on {
                    continue;
                }
                let at = num(&t, "src_in").unwrap_or(0.0) + (f as f64 - num(&t, "from").unwrap_or(0.0)) / prog.fps;
                t.vals.insert("src_in".into(), V::N(at));
                t.vals.insert("sound".into(), V::S(String::new()));
                t.vals.remove("gain");
                let full = !t.vals.contains_key("w") && !t.vals.contains_key("h");
                for (k, sc) in [("x", sx), ("w", sx), ("y", sy), ("h", sy)] {
                    if let Some(v) = num(&t, k) {
                        t.vals.insert(k.into(), V::N(v * sc));
                    }
                }
                // reframe on the subject (a pan set by hand stays)
                if !t.vals.contains_key("pan_x") && !t.vals.contains_key("pan_y") {
                    if let Some(V::S(src)) = t.vals.get("src").cloned() {
                        let path = crate::engine::resolve_src(state, &src, dir)?;
                        let info = backend.open_video(&path)?.info().clone();
                        let (bw, bh) = if full { (fm.w as f64, fm.h as f64) } else { (num(&t, "w").unwrap_or(fm.w as f64), num(&t, "h").unwrap_or(fm.h as f64)) };
                        let zoom = num(&t, "zoom").unwrap_or(1.0).max(0.01);
                        let (sw, sh) = (info.width as f64, info.height as f64);
                        let (at_pt, face) = focus(&t, &path, at);
                        let (px, py) = pan_for(sw, sh, bw, bh, zoom, at_pt);
                        t.vals.insert("pan_x".into(), V::N(px));
                        t.vals.insert("pan_y".into(), V::N(py));
                        // where that face lands in the output (the crop maps source to the box)
                        if let Some(b) = face {
                            let (cx, cy, cw, ch) = crate::engine::cover(sw, sh, bw, bh, zoom, (px, py));
                            let (ox, oy) = (num(&t, "x").unwrap_or(0.0), num(&t, "y").unwrap_or(0.0));
                            let map_x = |u: f64| ox + (u * sw - (cx - cw / 2.0)) / cw * bw;
                            let map_y = |v: f64| oy + (v * sh - (cy - ch / 2.0)) / ch * bh;
                            let (x0, y0, x1, y1) = (map_x(b[0]), map_y(b[1]), map_x(b[0] + b[2]), map_y(b[1] + b[3]));
                            faces.push([x0, y0, x1 - x0, y1 - y0]);
                        }
                    }
                }
            }
            k if KINDS_2D.contains(&k) => {
                if !on {
                    continue;
                }
                // a row inside another (`text t in card`) is placed in its parent's box: it scales with the parent and
                // stays where it is in it (the page passes below would treat its parent-relative y as the page's)
                if nested(&t) {
                    for k in ["x", "y", "w", "h", "size", "radius"] {
                        if let Some(v) = num(&t, k) {
                            t.vals.insert(k.into(), V::N(v * s));
                        }
                    }
                    if timed {
                        t.vals.insert("from".into(), V::N(0.0));
                        t.vals.insert("to".into(), V::N(1.0));
                    }
                    rows.push(t);
                    continue;
                }
                // its height (a box's own, else a line of its text): the block keeps its centre's place on the page
                let tall = num(&t, "h").unwrap_or(num(&t, "size").unwrap_or(48.0) * 1.2);
                let mid = num(&t, "y").unwrap_or(0.0) + tall / 2.0;
                for (k, sc) in [("x", sx), ("w", sx), ("h", s), ("size", s), ("radius", s)] {
                    if let Some(v) = num(&t, k) {
                        t.vals.insert(k.into(), V::N(v * sc));
                    }
                }
                if t.vals.contains_key("y") {
                    t.vals.insert("y".into(), V::N(mid * sy - tall * s / 2.0));
                }
            }
            _ => {}
        }
        if timed {
            t.vals.insert("from".into(), V::N(0.0));
            t.vals.insert("to".into(), V::N(1.0));
        }
        rows.push(t);
    }
    // text that would sit on a face moves below it (above when there's no room below)
    for t in rows.iter_mut().filter(|t| KINDS_2D.contains(&t.kind.as_str()) && t.vals.contains_key("y") && !nested(t)) {
        let (x, y) = (num(t, "x").unwrap_or(0.0), num(t, "y").unwrap_or(0.0));
        let w = num(t, "w").unwrap_or(fm.w as f64 - x);
        let tall = block_h(t, w);
        for f in &faces {
            let hits = x < f[0] + f[2] && x + w > f[0] && y < f[1] + f[3] && y + tall > f[1];
            if hits {
                let (fw, fh) = (fm.w as f64, fm.h as f64);
                let gap = 0.03 * fh;
                let below = f[1] + f[3] + gap;
                let above = f[1] - gap - tall;
                if below + tall <= fh * 0.97 {
                    t.vals.insert("y".into(), V::N(below));
                } else if above >= fh * 0.03 {
                    t.vals.insert("y".into(), V::N(above));
                } else {
                    // no room above or below (a landscape frame the face fills): beside it, in the wider side,
                    // centred on the face's middle
                    let (left, right) = (f[0], fw - (f[0] + f[2]));
                    let margin = 0.03 * fw;
                    let (nx, nw) = if left >= right { (margin, (left - 2.0 * margin).max(fw * 0.2)) } else { (f[0] + f[2] + margin, (right - 2.0 * margin).max(fw * 0.2)) };
                    t.vals.insert("x".into(), V::N(nx));
                    t.vals.insert("w".into(), V::N(nw));
                    let tall = block_h(t, nw);
                    t.vals.insert("y".into(), V::N((f[1] + f[3] / 2.0 - tall / 2.0).clamp(fh * 0.03, (fh * 0.97 - tall).max(fh * 0.03))));
                    t.vals.insert("align".into(), V::S("center".into()));
                }
            }
        }
    }
    // text blocks keep off each other too (a title and a caption moved to the same side would overlap): in edit order,
    // a block that lands on one already placed goes below it, else above
    let mut placed: Vec<[f64; 4]> = Vec::new();
    for t in rows.iter_mut().filter(|t| KINDS_2D.contains(&t.kind.as_str()) && t.vals.contains_key("y") && !nested(t)) {
        let x = num(t, "x").unwrap_or(0.0);
        let w = num(t, "w").unwrap_or(fm.w as f64 - x);
        let tall = block_h(t, w);
        let mut y = num(t, "y").unwrap_or(0.0);
        let gap = 0.015 * fm.h as f64;
        for _ in 0..placed.len() {
            let Some(p) = placed.iter().find(|p| x < p[0] + p[2] && x + w > p[0] && y < p[1] + p[3] && y + tall > p[1]) else { break };
            let below = p[1] + p[3] + gap;
            y = if below + tall <= fm.h as f64 * 0.97 { below } else { (p[1] - gap - tall).max(0.0) };
        }
        t.vals.insert("y".into(), V::N(y));
        placed.push([x, y, w, tall]);
    }
    let p = Program { fps: prog.fps, width: fm.w, height: fm.h, frames: 1, ..Default::default() };
    Ok((p, rows))
}

/// Draws one frame (frame 0) of a one-frame edit to a PNG: the CLI renders afresh, `rdb serve` keeps engines warm.
pub type RenderOne<'a> = dyn FnMut(&Program, &[Tup], &Path, &Path) -> Result<(), String> + 'a;

/// Stills of `frames` at `formats` into `out` (a folder): `<stem>-f<frame>-<format>.png` each, returned in order.
pub fn stills(prog: &Program, rows: &[Tup], dir: &Path, stem: &str, frames: &[i64], formats: &[Format], out: &Path) -> Result<Vec<PathBuf>, String> {
    stills_with(prog, rows, dir, stem, frames, formats, out, &mut |p, r, dir, dest| {
        let tmp = dest.with_extension("tmp");
        let _ = std::fs::remove_dir_all(&tmp);
        let opts = crate::RenderOpts { frames: Some(vec![0]), ..Default::default() };
        crate::render(p, r, dir, &tmp, &opts)?;
        let png = std::fs::read_dir(&tmp).map_err(|e| e.to_string())?.filter_map(|e| e.ok()).map(|e| e.path()).find(|p| p.extension().is_some_and(|x| x == "png"))
            .ok_or("no frame rendered")?;
        std::fs::rename(&png, dest).map_err(|e| e.to_string())?;
        let _ = std::fs::remove_dir_all(&tmp);
        Ok(())
    })
}

/// As `stills`, drawing each with `render`.
#[allow(clippy::too_many_arguments)]
pub fn stills_with(prog: &Program, rows: &[Tup], dir: &Path, stem: &str, frames: &[i64], formats: &[Format], out: &Path, render: &mut RenderOne) -> Result<Vec<PathBuf>, String> {
    std::fs::create_dir_all(out).map_err(|e| format!("{}: {e}", out.display()))?;
    let tl = crate::timeline::Timeline::new(prog, rows)?;
    let mut made = Vec::new();
    for &f in frames {
        let state = tl.tups(f.clamp(0, prog.frames.max(1) - 1));
        for fm in formats {
            let (p, r) = reflow(prog, &state, f, *fm, dir)?;
            let dest = out.join(format!("{stem}-f{f}-{}.png", fm.name));
            render(&p, &r, dir, &dest)?;
            made.push(dest);
        }
    }
    Ok(made)
}

/// A deck's page stills joined into one PDF at `out`, a page each in order, sized to the image (96 px to the inch).
/// Each page is the still as a JPEG (quality 92), so the file is a few hundred KB a page rather than raw pixels.
pub fn pdf(pages: &[PathBuf], out: &Path) -> Result<(), String> {
    use std::io::Write;
    let mut objs: Vec<Vec<u8>> = vec![Vec::new(), Vec::new()]; // 1 catalog, 2 page tree; filled in below
    let mut kids = Vec::new();
    for page in pages {
        let img = image::open(page).map_err(|e| format!("{}: {e}", page.display()))?.to_rgb8();
        let (w, h) = img.dimensions();
        let mut jpg = Vec::new();
        image::codecs::jpeg::JpegEncoder::new_with_quality(&mut jpg, 92).encode_image(&img).map_err(|e| e.to_string())?;
        let (pw, ph) = (w as f64 * 0.75, h as f64 * 0.75);
        let n = objs.len() + 1; // this page's image object; its content stream and page follow
        let mut im = format!("<< /Type /XObject /Subtype /Image /Width {w} /Height {h} /ColorSpace /DeviceRGB /BitsPerComponent 8 /Filter /DCTDecode /Length {} >>\nstream\n", jpg.len()).into_bytes();
        im.extend_from_slice(&jpg);
        im.extend_from_slice(b"\nendstream");
        objs.push(im);
        let draw = format!("q {pw} 0 0 {ph} 0 0 cm /Im Do Q");
        objs.push(format!("<< /Length {} >>\nstream\n{draw}\nendstream", draw.len()).into_bytes());
        objs.push(format!("<< /Type /Page /Parent 2 0 R /MediaBox [0 0 {pw} {ph}] /Resources << /XObject << /Im {n} 0 R >> >> /Contents {} 0 R >>", n + 1).into_bytes());
        kids.push(format!("{} 0 R", n + 2));
    }
    if kids.is_empty() {
        return Err("no pages to join".into());
    }
    objs[0] = b"<< /Type /Catalog /Pages 2 0 R >>".to_vec();
    objs[1] = format!("<< /Type /Pages /Kids [{}] /Count {} >>", kids.join(" "), kids.len()).into_bytes();
    let mut buf: Vec<u8> = b"%PDF-1.4\n%\xe2\xe3\xcf\xd3\n".to_vec();
    let mut offsets = Vec::new();
    for (i, o) in objs.iter().enumerate() {
        offsets.push(buf.len());
        write!(buf, "{} 0 obj\n", i + 1).unwrap();
        buf.extend_from_slice(o);
        buf.extend_from_slice(b"\nendobj\n");
    }
    let xref = buf.len();
    write!(buf, "xref\n0 {}\n0000000000 65535 f \n", objs.len() + 1).unwrap();
    for off in offsets {
        write!(buf, "{off:010} 00000 n \n").unwrap();
    }
    write!(buf, "trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n", objs.len() + 1).unwrap();
    std::fs::write(out, buf).map_err(|e| format!("{}: {e}", out.display()))
}

/// The moments worth a still, best first: frames of the edit whose source shows a face looking into the lens, eyes
/// open, mid-expression and between words, with the eye drawn to one place (the take's ingest: faces, performance,
/// attention), at least 1.5 s apart. Each with its score and why. Empty when the sources have no ingest.
pub fn best_frames(prog: &Program, rows: &[Tup], dir: &Path, n: usize) -> Result<Vec<(i64, f64, String)>, String> {
    crate::previz::register_media(rows, dir);
    let tl = crate::timeline::Timeline::new(prog, rows)?;
    let mut ingest: HashMap<String, Option<(serde_json::Value, serde_json::Value, serde_json::Value)>> = HashMap::new();
    let read = |hash: &str| -> Option<(serde_json::Value, serde_json::Value, serde_json::Value)> {
        let d = crate::previz::ingest_dir(hash)?;
        let j = |f: &str| std::fs::read_to_string(d.join(f)).ok().and_then(|t| serde_json::from_str::<serde_json::Value>(&t).ok());
        Some((j("faces.json")?, j("performance.json")?, j("attention.json").unwrap_or(serde_json::Value::Null)))
    };
    let at = |j: &serde_json::Value, key: &str, t: f64| -> Option<serde_json::Value> {
        let fps = j["fps"].as_f64().unwrap_or(1.0);
        let a = j[key].as_array()?;
        a.get(((t * fps).round().max(0.0) as usize).min(a.len().checked_sub(1)?)).cloned()
    };
    let step = (prog.fps / 4.0).round().max(1.0) as i64;
    let mut scored = Vec::new();
    for f in (0..prog.frames as i64).step_by(step as usize) {
        let st = tl.tups(f);
        let Some((hash, t)) = crate::previz::top_source(prog, &st, f) else { continue };
        let Some((faces, perf, attn)) = ingest.entry(hash.clone()).or_insert_with(|| read(&hash)).clone() else { continue };
        let Some(fa) = at(&faces, "frames", t) else { continue };
        let Some(face) = fa["faces"].as_array().and_then(|a| a.first()).and_then(|b| b.as_array()).map(|b| b.iter().filter_map(|x| x.as_f64()).collect::<Vec<_>>()) else { continue };
        // performance.json is one entry a second (its "fps" is the rate it was measured at, not the list's)
        let Some(p) = perf["seconds"].as_array().and_then(|a| a.get((t.max(0.0) as usize).min(a.len().saturating_sub(1)))).cloned() else { continue };
        if p["blink"].as_bool() == Some(true) {
            continue;
        }
        let lens = p["lens_share"].as_f64().unwrap_or(0.0);
        let energy = p["energy"].as_f64().unwrap_or(0.0);
        let speaking = p["speaking"].as_f64().unwrap_or(0.5);
        let focus = at(&attn, "frames", t).and_then(|a| a["focus"].as_f64()).unwrap_or(0.0);
        // a face big enough to read in a feed thumbnail (a fifth of the frame's height or more counts in full)
        let size = (face.get(3).copied().unwrap_or(0.0) / 0.2).min(1.0);
        // expression energy runs about 0.5 (flat) to 1.6 (big): scaled to 0..1 over that
        let expr = ((energy - 0.5) / 1.1).clamp(0.0, 1.0);
        let score = 0.3 * lens + 0.3 * expr + 0.2 * (1.0 - speaking) + 0.1 * (focus / 0.3).min(1.0) + 0.1 * size;
        let why = format!("eye contact {:.0}%, expression {:.2}, {}, face {:.0}% of the height", lens * 100.0, energy, if speaking < 0.3 { "between words" } else { "mid-word" }, face.get(3).copied().unwrap_or(0.0) * 100.0);
        scored.push((f, score, why));
    }
    scored.sort_by(|a, b| b.1.total_cmp(&a.1));
    let gap = (1.5 * prog.fps) as i64;
    let mut picks: Vec<(i64, f64, String)> = Vec::new();
    for s in scored {
        if picks.len() >= n {
            break;
        }
        if picks.iter().all(|p| (p.0 - s.0).abs() >= gap) {
            picks.push(s);
        }
    }
    Ok(picks)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tup(kind: &str, id: &str, vals: &[(&str, V)]) -> Tup {
        Tup { kind: kind.into(), id: id.into(), vals: vals.iter().map(|(k, v)| (k.to_string(), v.clone())).collect() }
    }

    // an agent session's stat card (2026-10-16): a 220 px number inside a 320 px card was "kept off" its own card by the text
    // pass, which read its parent-relative y as the page's, and drew ~550 px below it
    #[test]
    fn nested_rows_stay_in_their_parent() {
        let prog = Program { fps: 30.0, width: 1080, height: 1920, frames: 1, ..Default::default() };
        let state = vec![
            tup("box", "b", &[("x", V::N(60.0)), ("y", V::N(200.0)), ("w", V::N(960.0)), ("h", V::N(320.0))]),
            tup("text", "t", &[("parent", V::S("b".into())), ("text", V::S("30".into())), ("x", V::N(40.0)), ("y", V::N(50.0)), ("w", V::N(304.6)), ("size", V::N(220.0))]),
        ];
        let (_, rows) = reflow(&prog, &state, 0, format("9x16").unwrap(), Path::new(".")).unwrap();
        let t = rows.iter().find(|r| r.id == "t").unwrap();
        assert_eq!((num(t, "x"), num(t, "y"), num(t, "size")), (Some(40.0), Some(50.0), Some(220.0)));
        // at 1:1 it scales with the page, still in its parent's box
        let (_, rows) = reflow(&prog, &state, 0, format("1x1").unwrap(), Path::new(".")).unwrap();
        let t = rows.iter().find(|r| r.id == "t").unwrap();
        let s = num(t, "size").unwrap() / 220.0;
        assert!((num(t, "y").unwrap() - 50.0 * s).abs() < 1e-9, "{:?}", t.vals);
    }
}
