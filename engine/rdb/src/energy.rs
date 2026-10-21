//! An edit's energy, as docs/energy.feature defines it (D5a): descriptor columns per frame, a blend, and the scans
//! (kgpu/kernels/energy_*.k) that turn the curve into payoffs, breaths, flatlines and the picture-music relation.
//! The descriptors come from what rdb already makes: motion is the look's Hamming step frame to frame (the GPU sketch),
//! luma and contrast are the look grid's cells, the cut rate is the edit's own rows, loudness is the mix. Until
//! labelled high and low moments give a measured energy direction, the blend is the mean of the descriptors each
//! normalised over its own 5th-95th percentile, and the answer says so ("provisional").

use std::collections::HashMap;
use std::path::Path;

use serde_json::{json, Value};

use crate::eval::Tup;
use crate::ir::{Program, V};

const SMOOTH_K: &str = include_str!("../../kgpu/kernels/energy_smooth.k");
const NODES_K: &str = include_str!("../../kgpu/kernels/energy_nodes.k");
const CONTRAST_K: &str = include_str!("../../kgpu/kernels/energy_contrast.k");
const FLAT_K: &str = include_str!("../../kgpu/kernels/energy_flat.k");
const COV_K: &str = include_str!("../../kgpu/kernels/energy_cov.k");

/// A column scaled to 0..1 over its own 5th-95th percentile (clamped).
fn normalise(x: &[f64]) -> Vec<f64> {
    let mut s: Vec<f64> = x.iter().copied().filter(|v| v.is_finite()).collect();
    if s.is_empty() {
        return vec![0.0; x.len()];
    }
    s.sort_by(f64::total_cmp);
    let (lo, hi) = (s[s.len() * 5 / 100], s[(s.len() * 95 / 100).min(s.len() - 1)]);
    x.iter().map(|v| if hi > lo { ((v - lo) / (hi - lo)).clamp(0.0, 1.0) } else { 0.0 }).collect()
}

fn q16(x: &[f64]) -> Vec<u32> {
    x.iter().map(|v| ((v * 65536.0).round() as i32) as u32).collect()
}

/// Spans (in seconds) where `on` holds for at least `min_rows` rows.
fn spans(on: &[bool], fps: f64, min_rows: usize) -> Vec<[f64; 2]> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < on.len() {
        if !on[i] {
            i += 1;
            continue;
        }
        let a = i;
        while i < on.len() && on[i] {
            i += 1;
        }
        if i - a >= min_rows {
            out.push([(a as f64 / fps * 10.0).round() / 10.0, (i as f64 / fps * 10.0).round() / 10.0]);
        }
    }
    out
}

/// Motion from the footage itself, through the source map: the top clip's source and moment each frame, read in that
/// source's CV delta norms (kgpu's cv writer, in its ingest), so a held shot of a moving subject reads as motion and
/// a still frame doesn't. None when a source has no CV columns (then the sketch's look steps stand in).
fn source_motion(prog: &Program, rows: &[Tup]) -> Result<Option<Vec<f64>>, String> {
    let tl = crate::timeline::Timeline::new(prog, rows)?;
    let mut cols: HashMap<String, Option<(f64, Vec<i32>)>> = HashMap::new();
    let mut out = Vec::new();
    for f in 0..prog.frames.max(1) {
        let st = tl.tups(f);
        let num = |t: &Tup, k: &str, d: f64| match t.vals.get(k) { Some(V::N(x)) => *x, _ => d };
        let top = st.iter().filter(|t| t.kind == "clip" && crate::previz::showing(t, f))
            .max_by(|a, b| num(a, "z", 0.0).total_cmp(&num(b, "z", 0.0)).then(a.id.cmp(&b.id)));
        let Some(t) = top else { out.push(0.0); continue };
        let Some(V::S(src)) = t.vals.get("src") else { out.push(0.0); continue };
        let hash = src.strip_prefix("media:").unwrap_or(src).to_string();
        let col = cols.entry(hash.clone()).or_insert_with(|| {
            let dir = crate::previz::ingest_dir(&hash)?;
            let meta: Value = serde_json::from_str(&std::fs::read_to_string(dir.join("siglip.json")).ok()?).ok()?;
            let norm: Vec<i32> = std::fs::read(dir.join("cv/norm.i32")).ok()?.chunks_exact(4).map(|b| i32::from_le_bytes([b[0], b[1], b[2], b[3]])).collect();
            Some((meta["fps"].as_f64()?, norm))
        });
        let Some((fps, norm)) = col.as_ref() else { return Ok(None) };
        let at = num(t, "src_in", 0.0) + (f as f64 - num(t, "from", 0.0)) / prog.fps;
        let i = ((at * fps).floor().max(0.0) as usize).min(norm.len().saturating_sub(1));
        out.push((norm.get(i).copied().unwrap_or(0).max(0) as f64).sqrt());
    }
    Ok(Some(out))
}

/// The turning points that matter: peaks (1) and troughs (2) in order, a swing under `prominent` of the range merged
/// into its neighbours (a zigzag filter), so a turn is a rise and fall a viewer feels.
fn turns(smooth: &[i32], nodes: &[i32], prominent: f64) -> Vec<(usize, i32)> {
    let level = |f: usize| smooth[f] as f64 / 65536.0;
    let mut kept: Vec<(usize, i32)> = Vec::new();
    for (f, c) in nodes.iter().enumerate().filter(|(_, c)| **c == 1 || **c == 2) {
        match kept.last().copied() {
            Some((g, d)) if d == *c => {
                if (*c == 1 && level(f) > level(g)) || (*c == 2 && level(f) < level(g)) {
                    kept.pop();
                    kept.push((f, *c));
                }
            }
            Some((g, _)) if (level(f) - level(g)).abs() < prominent => {
                kept.pop();
                if let Some(&(h, e)) = kept.last() {
                    if e == *c && ((*c == 1 && level(f) > level(h)) || (*c == 2 && level(f) < level(h))) {
                        kept.pop();
                        kept.push((f, *c));
                    }
                }
            }
            _ => kept.push((f, *c)),
        }
    }
    kept
}

pub fn energy(prog: &Program, rows: &[Tup], dir: &Path) -> Result<Value, String> {
    crate::previz::register_media(rows, dir);
    let fps = prog.fps.max(1.0);
    let n = prog.frames.max(1) as usize;
    let frames: Vec<i64> = (0..n as i64).collect();
    let mut v = crate::gpuview::GpuView::new(prog, rows, dir)?;
    let (sketch, _) = v.sketch(&frames)?;
    let grid = v.grid(&sketch, n)?;
    let (hv, _) = v.looks(&sketch, n)?;
    let words: Vec<u32> = bytemuck::cast_slice(&v.gpu.read(&hv, n * 64 * 4)).to_vec();
    // motion: how much of the look changes from the frame before (0..1 of its 2048 bits)
    let motion: Vec<f64> = (0..n).map(|f| if f == 0 { 0.0 } else {
        (0..64).map(|w| (words[f * 64 + w] ^ words[(f - 1) * 64 + w]).count_ones()).sum::<u32>() as f64 / 2048.0
    }).collect();
    // luma: the cells' mean, and their spread (contrast)
    let (luma, spread): (Vec<f64>, Vec<f64>) = (0..n).map(|f| {
        let cells: Vec<f64> = (0..144).map(|c| grid[f * 432 + c * 3] as f64).collect();
        let m = cells.iter().sum::<f64>() / 144.0;
        let sd = (cells.iter().map(|x| (x - m).powi(2)).sum::<f64>() / 144.0).sqrt();
        (m, sd / (m.abs() + 1.0))
    }).unzip();
    // cuts per second: clips starting within half a second either side
    let starts: Vec<f64> = rows.iter().filter(|t| t.kind == "clip").filter_map(|t| match t.vals.get("from") { Some(V::N(x)) if *x > 0.0 => Some(*x), _ => None }).collect();
    let cut_rate: Vec<f64> = (0..n).map(|f| starts.iter().filter(|s| (**s - f as f64).abs() <= fps / 2.0).count() as f64).collect();
    // loudness: each frame's RMS of the mix, in dB
    let loud: Vec<f64> = match crate::sound(prog, rows, dir)? {
        Some(a) if a.len() > 0 => (0..n).map(|f| {
            let (s0, s1) = ((f as f64 / fps * a.rate as f64) as usize, (((f + 1) as f64) / fps * a.rate as f64) as usize);
            let mut e = 0.0f64;
            let mut k = 0usize;
            for ch in &a.channels {
                for x in ch.get(s0.min(ch.len())..s1.min(ch.len())).unwrap_or(&[]) {
                    e += (*x as f64).powi(2);
                    k += 1;
                }
            }
            if k == 0 { -90.0 } else { 10.0 * (e / k as f64 + 1e-12).log10() }
        }).collect(),
        _ => vec![-90.0; n],
    };
    // motion from the footage when every source has CV columns, else the sketch's look steps
    let (motion, motion_from) = match source_motion(prog, rows)? {
        Some(m) => (m, "source CV deltas (through the source map)"),
        None => (motion, "the sketch's look steps (a source has no CV columns: kgpu's cv on its ingest)"),
    };
    let (nm, nl, ns, nc, nd) = (normalise(&motion), normalise(&luma), normalise(&spread), normalise(&cut_rate), normalise(&loud));
    let picture: Vec<f64> = (0..n).map(|f| (nm[f] + ns[f] + nc[f]) / 3.0).collect();
    let blend: Vec<f64> = (0..n).map(|f| (nm[f] + ns[f] + nc[f] + nd[f]) / 4.0).collect();
    // the scans, on the GPU (each checked equal to its interpreter in kgpu's energytest)
    let gpu = &v.gpu;
    // about a second of window, twice: the jitter a viewer doesn't feel goes
    let r_smooth = (fps / 2.0).round().max(1.0) as i32;
    let r_wide = fps.round() as i32;
    let run = |src: &str, inputs: &[(&str, &[u32])], sc: &HashMap<&str, i32>, threads: usize| -> Result<Vec<i32>, String> {
        let names: Vec<&str> = inputs.iter().map(|x| x.0).collect();
        let c = gpu.compile(&raea_kgpu::parse(src)?, &names, sc, &["v"])?;
        let bufs: Vec<&[u32]> = inputs.iter().map(|x| x.1).collect();
        Ok(gpu.run(&c, &bufs, threads)?.0.remove(0).into_iter().map(|x| x as i32).collect())
    };
    let sc = |r: i32| -> HashMap<&'static str, i32> { [("n", n as i32), ("r", r), ("t", 20)].into() };
    let e = q16(&blend);
    let once = run(SMOOTH_K, &[("C", &e)], &sc(r_smooth), n)?;
    let ou: Vec<u32> = once.iter().map(|x| *x as u32).collect();
    let smooth = run(SMOOTH_K, &[("C", &ou)], &sc(r_smooth), n)?;
    let su: Vec<u32> = smooth.iter().map(|x| *x as u32).collect();
    let nodes = run(NODES_K, &[("S", &su)], &sc(r_smooth), n)?;
    let contrast = run(CONTRAST_K, &[("C", &su)], &sc(r_wide), n)?;
    let ku: Vec<u32> = contrast.iter().map(|x| *x as u32).collect();
    let flat = run(FLAT_K, &[("K", &ku)], &sc(r_wide), n)?;
    let cov = run(COV_K, &[("A", &q16(&picture)), ("B", &q16(&nd))], &sc(r_wide), n * 3)?;
    let r_pm: Vec<f64> = (0..n).map(|f| { let (c, a, b) = (cov[f * 3] as f64, cov[f * 3 + 1] as f64, cov[f * 3 + 2] as f64); if a > 0.0 && b > 0.0 { c / (a * b).sqrt() } else { 0.0 } }).collect();
    let at = |f: usize| (f as f64 / fps * 10.0).round() / 10.0;
    let kept = turns(&smooth, &nodes, 0.06);
    let marks = |code: i32| -> Vec<f64> { kept.iter().filter(|(_, c)| *c == code).map(|(f, _)| at(*f)).collect() };
    let mean_r = r_pm.iter().sum::<f64>() / n as f64;
    // aperture (G1): the frame's focus through the source map (1 = one place to look), smoothed and turned like the
    // energy curve: "in" where the frame tightens to one focal point, "out" where it opens to many
    let tl = crate::timeline::Timeline::new(prog, rows)?;
    let mut tracks = HashMap::new();
    let seen: Vec<Option<([f64; 2], [f64; 2], f64)>> = (0..n as i64).map(|f| crate::previz::attention(prog, &tl.tups(f), f, &mut tracks)).collect();
    let aperture = if seen.iter().any(|x| x.is_some()) {
        let focus: Vec<f64> = seen.iter().map(|x| x.map(|a| a.2).unwrap_or(0.0)).collect();
        let fq = q16(&normalise(&focus));
        let a1 = run(SMOOTH_K, &[("C", &fq)], &sc(r_smooth), n)?;
        let a1u: Vec<u32> = a1.iter().map(|x| *x as u32).collect();
        let a2 = run(SMOOTH_K, &[("C", &a1u)], &sc(r_smooth), n)?;
        let a2u: Vec<u32> = a2.iter().map(|x| *x as u32).collect();
        let an = run(NODES_K, &[("S", &a2u)], &sc(r_smooth), n)?;
        let k = turns(&a2, &an, 0.06);
        // how far the eye's centre moves across each cut (fractions of the frame)
        let mut cuts = starts.clone();
        cuts.sort_by(f64::total_cmp);
        cuts.dedup();
        let jumps: Vec<Value> = cuts.iter().filter_map(|s| {
            let f = *s as usize;
            let (a, b) = (seen.get(f.checked_sub(1)?)?.as_ref()?, seen.get(f)?.as_ref()?);
            let d = ((a.0[0] - b.0[0]).powi(2) + (a.0[1] - b.0[1]).powi(2)).sqrt();
            Some(json!({"at": at(f), "jump": (d * 100.0).round() / 100.0}))
        }).collect();
        let mut fv: Vec<f64> = seen.iter().filter_map(|x| x.map(|a| a.2)).collect();
        fv.sort_by(f64::total_cmp);
        let focus_median = (fv.get(fv.len() / 2).copied().unwrap_or(0.0) * 1000.0).round() / 1000.0;
        // split focus: a second place nearly as strong as the focal point (the catalogue's rival, 0.75 of the peak or
        // more) for half a second or longer, where a crop, a punch-in or a cut would give the eye one place to go
        let split: Vec<bool> = (0..n as i64).map(|f| crate::previz::rival(prog, &tl.tups(f), f, &mut tracks).is_some_and(|r| r.1 >= 0.75)).collect();
        json!({
            "in": k.iter().filter(|(_, c)| *c == 1).map(|(f, _)| at(*f)).collect::<Vec<_>>(),
            "split_focus": spans(&split, fps, (fps / 2.0) as usize),
            "out": k.iter().filter(|(_, c)| *c == 2).map(|(f, _)| at(*f)).collect::<Vec<_>>(),
            "eye_jumps_at_cuts": jumps,
            "focus_median": focus_median,
        })
    } else {
        json!("no attention tracks for these sources yet (raea-perceive ingest writes attention.json)")
    };
    // the curve at 4 a second, for plotting and reading
    let step = (fps / 4.0).round().max(1.0) as usize;
    Ok(json!({
        "provisional": "the blend is the mean of normalised descriptors until labelled high and low moments give an energy direction",
        "seconds": (n as f64 / fps * 100.0).round() / 100.0,
        "motion_from": motion_from,
        "payoffs": marks(1),
        "breaths": marks(2),
        "turns": nodes.iter().filter(|c| **c == 3).count(),
        "flatlines": spans(&flat.iter().map(|x| *x != 0).collect::<Vec<_>>(), fps, fps as usize),
        "picture_music_r": (mean_r * 100.0).round() / 100.0,
        "aperture": aperture,
        "curve_4hz": (0..n).step_by(step).map(|f| (smooth[f] as f64 / 65536.0 * 100.0).round() / 100.0).collect::<Vec<_>>(),
        "descriptors_4hz": {
            "motion": (0..n).step_by(step).map(|f| (nm[f] * 100.0).round() / 100.0).collect::<Vec<_>>(),
            "luma": (0..n).step_by(step).map(|f| (nl[f] * 100.0).round() / 100.0).collect::<Vec<_>>(),
            "contrast": (0..n).step_by(step).map(|f| (ns[f] * 100.0).round() / 100.0).collect::<Vec<_>>(),
            "cuts": (0..n).step_by(step).map(|f| (nc[f] * 100.0).round() / 100.0).collect::<Vec<_>>(),
            "loudness": (0..n).step_by(step).map(|f| (nd[f] * 100.0).round() / 100.0).collect::<Vec<_>>(),
        },
    }))
}
