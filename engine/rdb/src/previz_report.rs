//! The previz report (`rdb previz`, and anything that has an edit's program): sketch every
//! frame, the hypervectors, storyboards, cuts, faces and the layout scorer's rules, attention, light, the mix, flash
//! safety, content; previz.json and the sheets in `out`. Moved here from main.rs so a caller with a Program can run it.

use std::path::Path;

use crate::eval::Tup;
use crate::ir::{self, Program, V};
use crate::{engine, inserted_of, k, previz};

/// The previz experiment: sketch every frame from the rows (proxies + boxes), hypervectors from the rows and from the
/// sketches, then: speed, a storyboard, the authored cuts that don't read as cuts, fidelity against a real render
/// (--vs), and which frames a snippet changes (--snippet).
pub fn run(edit: &str, prog: Program, rows: Vec<Tup>, dir: &Path, out: &Path, vs: Option<String>, snippet: Option<String>) -> Result<(), String> {
    crate::previz::register_media(&rows, dir);
    use crate::previz::*;
    let dir = dir.to_path_buf();
    std::fs::create_dir_all(out).map_err(|e| e.to_string())?;
    let t = std::time::Instant::now();
    let st = states(&prog, &rows)?;
    let states_s = t.elapsed().as_secs_f64();
    let t = std::time::Instant::now();
    let px = proxies(&prog, &st, &dir, &engine::media_backend())?;
    let proxy_s = t.elapsed().as_secs_f64();
    let t = std::time::Instant::now();
    let pics: Vec<Pic> = (0..prog.frames).map(|f| sketch(&prog, &st[f as usize], f, &px)).collect();
    let sketch_s = t.elapsed().as_secs_f64();
    let t = std::time::Instant::now();
    let look = Look::new();
    let looks: Vec<k::Hv> = pics.iter().map(|p| look.hv(p)).collect();
    let says: Vec<k::Hv> = (0..prog.frames).map(|f| row_hv(&prog, &st[f as usize], f)).collect();
    let hv_s = t.elapsed().as_secs_f64();
    let n = prog.frames as usize;

    // storyboard: 12 frames by coverage of looks, against 12 evenly spaced
    let (look_picks, look_worst) = storyboard(&looks, 12);
    // what the edit says and what it shows, together: a frame is far when its rows or its look are
    let both = |a: usize, b: usize| k::hamming(&says[a], &says[b]) + k::hamming(&looks[a], &looks[b]);
    let (picks, worst) = storyboard_by(n, 12, &both);
    let cover_both = |p: &[usize]| (0..n).map(|j| p.iter().map(|&i| both(j, i)).min().unwrap()).max().unwrap();
    let even: Vec<usize> = (0..12).map(|i| (i * n / 12 + n / 24).min(n - 1)).collect();
    save(&sheet(&picks.iter().map(|&i| pics[i].clone()).collect::<Vec<_>>(), 6), &out.join("storyboard.png"))?;
    save(&sheet(&even.iter().map(|&i| pics[i].clone()).collect::<Vec<_>>(), 6), &out.join("even.png"))?;

    // B3: each frame's faces and texts through the layout scorer's hard rules
    let mut tracks = std::collections::HashMap::new();
    let mut fails: std::collections::BTreeMap<String, Vec<usize>> = std::collections::BTreeMap::new();
    let (mut with_faces, mut scored) = (0usize, 0usize);
    let mut boxed = pics.clone();
    // G2: each platform's bands, and the eye line's and the text's distance to the power lines
    let mut band_frames: Vec<(usize, usize)> = vec![(0, 0); previz::PLATFORM_BANDS.len()];
    let (mut eye_lines, mut text_lines): (Vec<f64>, Vec<f64>) = (Vec::new(), Vec::new());
    for f in 0..n {
        let z = previz::zones(&prog, &st[f], f as i64, &mut tracks);
        for (i, (_, b, _)) in previz::PLATFORM_BANDS.iter().enumerate() {
            band_frames[i].0 += z.faces.iter().any(|(fb, _)| previz::in_bands(fb, b)) as usize;
            band_frames[i].1 += z.texts.iter().any(|(tb, _)| previz::in_bands(tb, b)) as usize;
        }
        // the biggest face's eye line (0.45 down the face box, as the coach reads it)
        if let Some((fb, _)) = z.faces.iter().max_by(|a, b| (a.0[2] * a.0[3]).total_cmp(&(b.0[2] * b.0[3]))) {
            eye_lines.push(previz::to_power_line(fb[1] + 0.45 * fb[3]));
        }
        for (tb, _) in &z.texts {
            text_lines.push(previz::to_power_line(tb[1] + tb[3] / 2.0));
        }
        for (face, _) in &z.faces {
            previz::outline(&mut boxed[f], face, [0, 255, 0]);
        }
        with_faces += !z.faces.is_empty() as usize;
        if z.faces.is_empty() && z.texts.is_empty() {
            continue;
        }
        scored += 1;
        for fail in raea::previz::score(&previz::layout_of(&prog, &z)).fails {
            // the rule, without the zone numbers that differ frame to frame
            // the rule without the zone numbers that differ frame to frame ("subject 2's face" -> "subject's face")
            let mut words: Vec<String> = Vec::new();
            for w in fail.split(' ') {
                let digits = w.trim_end_matches("'s");
                if !digits.is_empty() && digits.chars().all(|c| c.is_ascii_digit()) {
                    if w.ends_with("'s") {
                        if let Some(prev) = words.last_mut() {
                            prev.push_str("'s");
                        }
                    }
                    continue;
                }
                words.push(w.to_string());
            }
            let rule = words.join(" ");
            fails.entry(rule).or_default().push(f);
        }
    }
    let spans = |frames: &[usize]| {
        let mut out: Vec<(usize, usize)> = Vec::new();
        for &f in frames {
            match out.last_mut() { Some(l) if f == l.1 + 1 => l.1 = f, _ => out.push((f, f)) }
        }
        out
    };
    let rules_report: serde_json::Map<String, serde_json::Value> = fails.iter()
        .map(|(rule, fr)| { let mut fr = fr.clone(); fr.dedup(); (rule.clone(), serde_json::json!({"frames": fr.len(), "spans": spans(&fr)})) })
        .collect();
    save(&sheet(&look_picks.iter().map(|&i| boxed[i].clone()).collect::<Vec<_>>(), 6), &out.join("storyboard-faces.png"))?;
    let med = |v: &mut Vec<f64>| if v.is_empty() { None } else { v.sort_by(f64::total_cmp); Some((v[v.len() / 2] * 1000.0).round() / 1000.0) };
    let platforms: Vec<serde_json::Value> = previz::PLATFORM_BANDS.iter().zip(&band_frames)
        .map(|((name, b, src), (fc, tc))| serde_json::json!({"platform": name, "bands_px": b, "source": src, "frames_face_in_bands": fc, "frames_text_in_bands": tc}))
        .collect();
    let (outside_house_avg_face, outside_house_avg_text) = {
        let k = (band_frames.len() - 1) as f64;
        (band_frames[1..].iter().map(|x| x.0 as f64).sum::<f64>() / k, band_frames[1..].iter().map(|x| x.1 as f64).sum::<f64>() / k)
    };
    let composition_json = serde_json::json!({
        "eye_line_to_power_line_median": med(&mut eye_lines), "text_to_power_line_median": med(&mut text_lines),
        "platforms": platforms,
        "platform_average": {"frames_face_in_bands": (outside_house_avg_face * 10.0).round() / 10.0, "frames_text_in_bands": (outside_house_avg_text * 10.0).round() / 10.0},
    });
    // flash safety, the catalogue's hard rule (WCAG 2.3.1's general flash, on the sketches)
    let lum: Vec<Vec<f32>> = pics.iter().map(|p| crate::flash::luminance(&p.rgb)).collect();
    let flashes = crate::flash::unsafe_spans(&lum, prog.fps);
    let rules_json = serde_json::json!({"frames_with_faces": with_faces, "frames_scored": scored, "hard_rule_fails": rules_report, "composition": composition_json,
        "flash_safety": {"pass": flashes.is_empty(), "unsafe_spans": flashes,
            "flash_transitions": crate::flash::transitions(&lum).iter().filter(|&&d| d != 0).count()}});

    // B2: what the frames show by content (SigLIP 2 rows of their sources' ingest, through the source map)
    let content = previz::content(&prog, &st);
    let content_report = if content.hvs.iter().any(|h| h.is_some()) {
        let half = 1024u32;
        let cdist = |a: usize, b: usize| match (&content.hvs[a], &content.hvs[b]) {
            (Some(x), Some(y)) => k::hamming(x, y),
            _ => half,
        };
        // the two distances on one scale: each over its own median across frame pairs (a fixed stride of pairs)
        let median = |d: &dyn Fn(usize, usize) -> u32| {
            let mut v: Vec<u32> = (0..n).step_by(7).flat_map(|a| (0..n).step_by(11).map(move |b| (a, b))).filter(|(a, b)| a != b).map(|(a, b)| d(a, b)).collect();
            v.sort();
            v.get(v.len() / 2).copied().unwrap_or(1).max(1) as f64
        };
        let (ml, mc) = (median(&|a, b| k::hamming(&looks[a], &looks[b])), median(&cdist));
        let look_content = |a: usize, b: usize| ((k::hamming(&looks[a], &looks[b]) as f64 / ml + cdist(a, b) as f64 / mc) * 1000.0) as u32;
        let (cpicks, cworst) = storyboard_by(n, 12, &look_content);
        save(&sheet(&cpicks.iter().map(|&i| pics[i].clone()).collect::<Vec<_>>(), 6), &out.join("storyboard-content.png"))?;
        // near-duplicates among picks: pairs whose SigLIP 2 rows are 0.9 alike or more, and the closest pair
        let cos = |a: usize, b: usize| match (&content.vectors[a], &content.vectors[b]) {
            (Some(x), Some(y)) => Some(x.iter().zip(y).map(|(p, q)| p * q).sum::<f32>() as f64),
            _ => None,
        };
        let dupes = |p: &[usize]| {
            let mut pairs = 0;
            let mut closest = f64::MIN;
            for i in 0..p.len() {
                for j in i + 1..p.len() {
                    if let Some(c) = cos(p[i], p[j]) {
                        pairs += (c >= 0.9) as usize;
                        closest = closest.max(c);
                    }
                }
            }
            serde_json::json!({"pairs_over_0.9": pairs, "closest_cosine": (closest * 1000.0).round() / 1000.0})
        };
        serde_json::json!({
            "frames_with_content": content.hvs.iter().filter(|h| h.is_some()).count(), "missing_sources": content.missing,
            "by_look_and_content": {"picks": cpicks, "worst_scaled": cworst, "median_bits": {"look": ml, "content": mc}},
            "near_duplicates": {"even": dupes(&even), "by_look": dupes(&look_picks), "by_rows_and_look": dupes(&picks), "by_look_and_content": dupes(&cpicks)},
        })
    } else {
        serde_json::json!({"frames_with_content": 0, "missing_sources": content.missing})
    };

    // authored cuts: frames where the set of visible clips changes; how much the look and the rows move there,
    // against how much they move frame to frame inside shots
    let clips_at = |f: usize| -> Vec<String> {
        let mut v: Vec<String> = st[f].iter().filter(|t| t.kind == "clip" && {
            let g = |k: &str, d: f64| t.vals.get(k).map(V::num).unwrap_or(d);
            (f as f64) >= g("from", f64::MIN) && (f as f64) < g("to", f64::MAX)
        }).map(|t| t.id.clone()).collect();
        v.sort();
        v
    };
    let mut cuts = Vec::new();
    let mut within_look = Vec::new();
    let mut within_says = Vec::new();
    for f in 1..n {
        let (ls, ss) = (k::hv_sim(&looks[f - 1], &looks[f]) as f64, k::hv_sim(&says[f - 1], &says[f]) as f64);
        if clips_at(f) != clips_at(f - 1) {
            cuts.push(serde_json::json!({"frame": f, "from": clips_at(f - 1), "to": clips_at(f), "look_sim": ls, "rows_sim": ss}));
        } else {
            within_look.push(ls);
            within_says.push(ss);
        }
    }
    let pct = |v: &mut Vec<f64>, p: f64| {
        v.sort_by(f64::total_cmp);
        v[((v.len() - 1) as f64 * p) as usize]
    };
    let report_cuts: Vec<serde_json::Value> = cuts
        .iter()
        .map(|c| {
            let mut c = c.clone();
            // a cut that looks like a continuing shot: its look moves less than 1 in 20 within-shot frame steps
            c["reads_as_cut"] = serde_json::json!(c["look_sim"].as_f64().unwrap() < pct(&mut within_look.clone(), 0.05));
            c
        })
        .collect();

    // screen direction and matches at each cut (the catalogue): the way things move in the 0.3 s either side (the largest
    // face's centre, else the picture's shift), kept / reversed / still; a match on action when the motion carries on
    // at a like speed (a half to twice); a graphic match when the picture source changes but the cut doesn't read as one
    let direction_json = screen_direction(&prog, &st, &pics, &report_cuts, n);

    // the catalogue: repetition fatigue, text against its background, text over motion
    let catalogue_json = {
        // shots: the spans between authored cuts, each with its mean SigLIP 2 row (B2's content)
        let mut bounds: Vec<usize> = std::iter::once(0).chain(cuts.iter().filter_map(|c| c["frame"].as_u64().map(|f| f as usize))).collect();
        bounds.push(n);
        let shots: Vec<(usize, usize)> = bounds.windows(2).map(|w| (w[0], w[1])).filter(|(a, b)| b > a).collect();
        let mean_vec = |a: usize, b: usize| -> Option<Vec<f32>> {
            let vs: Vec<&Vec<f32>> = (a..b).filter_map(|f| content.vectors[f].as_ref()).collect();
            if vs.is_empty() { return None }
            let mut m = vec![0f32; vs[0].len()];
            for v in &vs { for (x, y) in m.iter_mut().zip(v.iter()) { *x += y } }
            let norm = m.iter().map(|x| x * x).sum::<f32>().sqrt().max(1e-6);
            Some(m.into_iter().map(|x| x / norm).collect())
        };
        let vecs: Vec<Option<Vec<f32>>> = shots.iter().map(|&(a, b)| mean_vec(a, b)).collect();
        let alike: Vec<serde_json::Value> = (1..shots.len()).filter_map(|i| {
            let (x, y) = (vecs[i - 1].as_ref()?, vecs[i].as_ref()?);
            let c: f32 = x.iter().zip(y).map(|(p, q)| p * q).sum();
            Some(serde_json::json!({"cut": shots[i].0, "cosine": (c as f64 * 1000.0).round() / 1000.0}))
        }).collect();
        let held: Vec<f64> = shots.iter().map(|(a, b)| ((b - a) as f64 / prog.fps * 10.0).round() / 10.0).collect();
        // text against what's behind it: the sketch without its texts, the text's colour against that, as WCAG's ratio
        let clips_only: Vec<Pic> = (0..prog.frames).map(|f| {
            let rows_f: Vec<Tup> = st[f as usize].iter().filter(|t| !matches!(t.kind.as_str(), "caption" | "title" | "text")).cloned().collect();
            sketch(&prog, &rows_f, f, &px)
        }).collect();
        let rel = |c: f64| { let c = c / 255.0; if c <= 0.04045 { c / 12.92 } else { ((c + 0.055) / 1.055).powf(2.4) } };
        let lum = |r: f64, g: f64, b: f64| 0.2126 * rel(r) + 0.7152 * rel(g) + 0.0722 * rel(b);
        let mut ztr = std::collections::HashMap::new();
        let (mut ratios, mut low, mut motion_under, mut busy) = (Vec::new(), 0usize, Vec::new(), 0usize);
        let mut motion_frames: Vec<(usize, f64)> = Vec::new(); // (frame, motion under the first text) for per-moment comparisons
        for f in 0..n {
            let z = previz::zones(&prog, &st[f], f as i64, &mut ztr);
            for t in st[f].iter().filter(|t| matches!(t.kind.as_str(), "caption" | "title" | "text") && crate::previz::showing(t, f as i64)) {
                let col = match t.vals.get("color") { Some(V::S(c)) => c.trim_start_matches('#').to_string(), _ => "ffffff".into() };
                let hex = |i: usize| u8::from_str_radix(col.get(i..i + 2).unwrap_or("ff"), 16).unwrap_or(255) as f64;
                let lt = lum(hex(0), hex(2), hex(4));
                let _ = &z;
                // the text's box from the zones (same geometry), in sketch pixels
                for (tb, _) in z.texts.iter().take(1) {
                    let p = &clips_only[f];
                    let (x0, y0) = ((tb[0] * p.w as f64).max(0.0) as usize, (tb[1] * p.h as f64).max(0.0) as usize);
                    let (x1, y1) = (((tb[0] + tb[2]) * p.w as f64).min(p.w as f64) as usize, ((tb[1] + tb[3]) * p.h as f64).min(p.h as f64) as usize);
                    let (mut sum, mut cnt, mut mot) = (0f64, 0usize, 0f64);
                    for y in y0..y1 {
                        for x in x0..x1 {
                            let i = (y * p.w + x) * 3;
                            sum += lum(p.rgb[i] as f64, p.rgb[i + 1] as f64, p.rgb[i + 2] as f64);
                            if f > 0 {
                                let q = &clips_only[f - 1];
                                mot += (0..3).map(|k| (p.rgb[i + k] as f64 - q.rgb[i + k] as f64).abs()).sum::<f64>() / 3.0;
                            }
                            cnt += 1;
                        }
                    }
                    if cnt == 0 { continue }
                    let lb = sum / cnt as f64;
                    let ratio = (lt.max(lb) + 0.05) / (lt.min(lb) + 0.05);
                    ratios.push(ratio);
                    low += (ratio < 3.0) as usize;
                    let m = mot / cnt as f64;
                    motion_under.push(m);
                    if f > 0 { motion_frames.push((f, (m * 100.0).round() / 100.0)); }
                    busy += (m > 8.0) as usize;
                }
                break;
            }
        }
        ratios.sort_by(f64::total_cmp);
        motion_under.sort_by(f64::total_cmp);
        let q = |v: &Vec<f64>, p: f64| v.get(((v.len().max(1) - 1) as f64 * p) as usize).map(|x| (x * 100.0).round() / 100.0);
        // orphaned mouths: at each shot's start and end, the mouth moving (a step over 0.03 between mesh samples in the
        // 0.3 s inside the edge) while the voice is silent there (20 dB under its median while talking)
        let (voice, talk) = crate::mixline::voice_levels(&prog, &rows, &dir).unwrap_or_default();
        let mut mouths: std::collections::HashMap<String, Option<(f64, Vec<Option<f64>>)>> = std::collections::HashMap::new();
        let mut orphaned = Vec::new();
        for &(a, b) in &shots {
            for (edge, range) in [("start", a..(a + (0.3 * prog.fps) as usize).min(b)), ("end", b.saturating_sub((0.3 * prog.fps) as usize).max(a)..b)] {
                let frames: Vec<usize> = range.collect();
                let Some(&mid) = frames.get(frames.len() / 2) else { continue };
                let Some((hash, _)) = previz::top_source(&prog, &st[mid], mid as i64) else { continue };
                let Some((mfps, track)) = mouths.entry(hash.clone()).or_insert_with(|| previz::mouth_track(&hash)).clone() else { continue };
                let ms: Vec<f64> = frames.iter().filter_map(|&f| previz::top_source(&prog, &st[f], f as i64))
                    .filter_map(|(_, t)| track.get(((t * mfps).round().max(0.0) as usize).min(track.len().saturating_sub(1))).copied().flatten()).collect();
                let moving = ms.windows(2).any(|w| (w[1] - w[0]).abs() > 0.03);
                let (t0, t1) = (frames[0] as f64 / prog.fps, (*frames.last().unwrap() + 1) as f64 / prog.fps);
                let silent = voice.iter().skip((t0 * 100.0) as usize).take(((t1 - t0) * 100.0) as usize).all(|l| *l < talk - 20.0);
                if moving && silent && !voice.is_empty() {
                    orphaned.push(serde_json::json!({"shot_from": a, "edge": edge}));
                }
            }
        }
        // glyph cutoff: a text whose words reach past the frame (more than 1% of it), in spans; a span under a reading
        // beat (0.3 s) is noted but is a flicker, not a cut-off line
        let mut gz = std::collections::HashMap::new();
        let cut_frames: Vec<usize> = (0..n).filter(|&f| previz::zones(&prog, &st[f], f as i64, &mut gz).texts.iter()
            .any(|(tb, _)| tb[0] < -0.01 || tb[1] < -0.01 || tb[0] + tb[2] > 1.01 || tb[1] + tb[3] > 1.01)).collect();
        let mut glyph_spans: Vec<(usize, usize)> = Vec::new();
        for f in cut_frames {
            match glyph_spans.last_mut() { Some(s) if f == s.1 + 1 => s.1 = f, _ => glyph_spans.push((f, f)) }
        }
        let beat = (0.3 * prog.fps) as usize;
        serde_json::json!({
            "glyph_cutoff": {"spans": glyph_spans.iter().filter(|s| s.1 - s.0 + 1 >= beat).collect::<Vec<_>>(), "flickers": glyph_spans.iter().filter(|s| s.1 - s.0 + 1 < beat).count()},
            "orphaned_mouths": orphaned,
            "shots": shots.len(), "shot_seconds": held, "longest_shot_s": held.iter().cloned().fold(0.0, f64::max),
            "consecutive_shots_alike": alike, "pairs_over_0.95": alike.iter().filter(|a| a["cosine"].as_f64().unwrap_or(0.0) >= 0.95).count(),
            "text_contrast": {"frames": ratios.len(), "median": q(&ratios, 0.5), "lowest": q(&ratios, 0.0), "frames_under_3_to_1": low},
            "text_over_motion": {"median": q(&motion_under, 0.5), "p90": q(&motion_under, 0.9), "frames_busy": busy, "frames": motion_frames},
        })
    };

    // G4: the light, frame by frame, and its jumps at each authored cut
    let mut light_tracks = std::collections::HashMap::new();
    let lights: Vec<Option<serde_json::Value>> = (0..n).map(|f| previz::light(&prog, &st[f], f as i64, &mut light_tracks)).collect();
    let light_json = if lights.iter().any(|l| l.is_some()) {
        let mut labels: std::collections::BTreeMap<String, usize> = std::collections::BTreeMap::new();
        for l in lights.iter().flatten() {
            *labels.entry(l["light"].as_str().unwrap_or("no face").to_string()).or_default() += 1;
        }
        let jumps: Vec<serde_json::Value> = cuts.iter().filter_map(|c| {
            let f = c["frame"].as_u64()? as usize;
            let (a, b) = (lights.get(f - 1)?.as_ref()?, lights.get(f)?.as_ref()?);
            let d = |k: &str| b[k].as_f64().zip(a[k].as_f64()).map(|(x, y)| ((x - y) * 1000.0).round() / 1000.0);
            Some(serde_json::json!({"frame": f, "luma": d("luma"), "warmth": d("warmth")}))
        }).collect();
        serde_json::json!({"frames": lights.iter().flatten().count(), "light_on_the_face": labels, "jumps_at_cuts": jumps})
    } else {
        serde_json::json!({"frames": 0})
    };

    // B4: the sketches at 48 x 27 RGB, for TransNetV2 (`raea-perceive transnet-frames`): does an authored cut read as one?
    {
        let mut raw = Vec::with_capacity(n * 48 * 27 * 3);
        for p in &pics {
            for y in 0..27 {
                let (y0, y1) = (y * p.h / 27, ((y + 1) * p.h / 27).max(y * p.h / 27 + 1));
                for x in 0..48 {
                    let (x0, x1) = (x * p.w / 48, ((x + 1) * p.w / 48).max(x * p.w / 48 + 1));
                    let mut acc = [0u32; 3];
                    for yy in y0..y1 {
                        for xx in x0..x1 {
                            for c in 0..3 {
                                acc[c] += p.rgb[(yy * p.w + xx) * 3 + c] as u32;
                            }
                        }
                    }
                    let cnt = ((y1 - y0) * (x1 - x0)) as u32;
                    raw.extend(acc.iter().map(|a| ((a + cnt / 2) / cnt) as u8));
                }
            }
        }
        std::fs::write(out.join("sketch-48x27.rgb"), raw).map_err(|e| e.to_string())?;
    }

    // G1: where the eye goes, frame by frame, and its jump at each authored cut
    let mut att_tracks = std::collections::HashMap::new();
    let att: Vec<Option<([f64; 2], [f64; 2], f64)>> = (0..n).map(|f| previz::attention(&prog, &st[f], f as i64, &mut att_tracks)).collect();
    let attention_json = if att.iter().any(|a| a.is_some()) {
        let focus: Vec<f64> = att.iter().flatten().map(|a| a.2).collect();
        let mut fsorted = focus.clone();
        fsorted.sort_by(f64::total_cmp);
        // the eye's jump at each authored cut: the attention centre's move (fractions of the canvas, aspect-true)
        let aspect = prog.height as f64 / prog.width as f64;
        let jumps: Vec<serde_json::Value> = cuts.iter().filter_map(|c| {
            let f = c["frame"].as_u64()? as usize;
            let (a, b) = (att.get(f - 1)?.as_ref()?, att.get(f)?.as_ref()?);
            let d = ((a.0[0] - b.0[0]).powi(2) + ((a.0[1] - b.0[1]) * aspect).powi(2)).sqrt();
            Some(serde_json::json!({"frame": f, "jump": (d * 1000.0).round() / 1000.0}))
        }).collect();
        // the first look of each shot: does the attention peak land on a face (the zones of B3)?
        let mut ztracks = std::collections::HashMap::new();
        let first_looks: Vec<serde_json::Value> = std::iter::once(0usize).chain(cuts.iter().filter_map(|c| c["frame"].as_u64().map(|f| f as usize))).filter_map(|f| {
            let a = att.get(f)?.as_ref()?;
            let z = previz::zones(&prog, &st[f], f as i64, &mut ztracks);
            let on_face = z.faces.iter().any(|(fb, _)| a.1[0] >= fb[0] && a.1[0] <= fb[0] + fb[2] && a.1[1] >= fb[1] && a.1[1] <= fb[1] + fb[3]);
            Some(serde_json::json!({"frame": f, "peak_on_face": on_face}))
        }).collect();
        // the creator's gaze: the share of frames with a face whose gaze points within the performance read's lens cone
        // (|gaze yaw - (-6)| <= 16 degrees: toward the camera)
        let mut gz: std::collections::HashMap<String, Option<(f64, Vec<Option<f64>>)>> = std::collections::HashMap::new();
        let gazes: Vec<f64> = (0..n).filter_map(|f| {
            let (hash, t) = previz::top_source(&prog, &st[f], f as i64)?;
            let (gfps, tr) = gz.entry(hash.clone()).or_insert_with(|| previz::gaze_track(&hash)).clone()?;
            tr.get(((t * gfps).round().max(0.0) as usize).min(tr.len().saturating_sub(1))).copied().flatten()
        }).collect();
        // the focal point (the catalogue): on frames with a rival, the share where the rival has 0.8 of the peak's mass
        // or more and isn't on the peak's face (two places compete for the eye), and where the two land
        let mut ztr2 = std::collections::HashMap::new();
        let mut rtracks = std::collections::HashMap::new();
        let (mut rframes, mut competing, mut rr) = (0usize, 0usize, Vec::new());
        let mut lands: std::collections::BTreeMap<String, usize> = std::collections::BTreeMap::new();
        for f in 0..n {
            let (Some(a), Some((rp, ratio))) = (att[f].as_ref(), previz::rival(&prog, &st[f], f as i64, &mut rtracks)) else { continue };
            rframes += 1;
            rr.push(ratio);
            if ratio < 0.8 { continue }
            let z = previz::zones(&prog, &st[f], f as i64, &mut ztr2);
            let inside = |p: [f64; 2], b: &[f64; 4]| p[0] >= b[0] && p[0] <= b[0] + b[2] && p[1] >= b[1] && p[1] <= b[1] + b[3];
            // both on one face is one subject (eyes and mouth), not two places
            if z.faces.iter().any(|(b, _)| inside(a.1, b) && inside(rp, b)) { continue }
            competing += 1;
            let place = |p: [f64; 2]| if z.faces.iter().any(|(b, _)| inside(p, b)) { "face" } else if z.texts.iter().any(|(b, _)| inside(p, b)) { "text" }
                else if p[0] < 0.0 || p[0] > 1.0 || p[1] < 0.0 || p[1] > 1.0 { "off the canvas" } else { "elsewhere" };
            *lands.entry(format!("{} / {}", place(a.1), place(rp))).or_default() += 1;
        }
        rr.sort_by(f64::total_cmp);
        let focal = serde_json::json!({"frames": rframes, "rival_ratio_median": rr.get(rr.len() / 2).map(|x| (x * 1000.0).round() / 1000.0),
            "competing_share": if rframes == 0 { None } else { Some((competing as f64 / rframes as f64 * 100.0).round() / 100.0) },
            "competing_peak_and_rival_on": lands});
        let toward = gazes.iter().filter(|g| (*g + 6.0).abs() <= 16.0).count();
        serde_json::json!({"gaze_frames": gazes.len(), "looking_toward_camera_share": if gazes.is_empty() { None } else { Some((toward as f64 / gazes.len() as f64 * 100.0).round() / 100.0) },
            "frames": focus.len(), "focus_median": (fsorted[fsorted.len() / 2] * 1000.0).round() / 1000.0,
            "focus_by_second": (0..n).step_by(prog.fps as usize).map(|f| att[f].as_ref().map(|a| (a.2 * 1000.0).round() / 1000.0)).collect::<Vec<_>>(),
            "eye_jumps_at_cuts": jumps, "first_looks": first_looks, "focal_point": focal})
    } else {
        serde_json::json!({"frames": 0})
    };

    let mut j = serde_json::json!({
        "frames": n, "proxy_size": [PW, PH],
        "seconds": {"states": states_s, "proxies": proxy_s, "sketch": sketch_s, "hypervectors": hv_s},
        "proxies_decoded": px.decoded, "sketch_fps": n as f64 / sketch_s,
        "previz_fps_with_states": n as f64 / (states_s + sketch_s),
        "storyboard": {"by_look": {"picks": look_picks, "worst_look_bits": look_worst, "even_worst_look_bits": coverage(&looks, &even)},
            "by_rows_and_look": {"picks": picks, "worst_bits": worst, "even_worst_bits": cover_both(&even), "look_only_picks_worst_bits": cover_both(&look_picks)}},
        "within_shot_look_sim_p05": pct(&mut within_look.clone(), 0.05), "within_shot_look_sim_p50": pct(&mut within_look.clone(), 0.5),
        "within_shot_rows_sim_p05": pct(&mut within_says.clone(), 0.05),
        "cuts": report_cuts,
        "content": content_report,
        "rules": rules_json,
        "attention": attention_json,
        "light": light_json,
        "catalogue": catalogue_json,
        "animation": crate::animation::report(&prog, &st),
        "diegetic": crate::mixline::diegetic(&prog, &rows, &dir, &st, &cuts.iter().filter_map(|c| c["frame"].as_u64().map(|f| f as usize)).collect::<Vec<_>>())
            .map(|v| serde_json::json!({"cuts": v.len(), "mismatches": v.iter().filter(|c| c["mismatch"] == true).count(), "at_cuts": v}))
            .unwrap_or_else(|e| serde_json::json!({"error": e})),
        "mix": crate::mixline::timeline_at(&prog, &rows, &dir, &cuts.iter().filter_map(|c| c["frame"].as_u64().map(|f| f as usize)).collect::<Vec<_>>())
            .unwrap_or_else(|e| serde_json::json!({"error": e})),
    });

    j["screen_direction"] = direction_json;
    j["catalogue"]["silent_mouths"] = silent_mouths(&prog, &st, &rows, &dir);
    j["catalogue"]["depth_touches"] = depth_touches(&prog, &st);
    j["cut_timing"] = crate::mixline::cut_timing(&prog, &rows, &dir, &cuts.iter().filter_map(|c| c["frame"].as_u64().map(|f| f as usize)).collect::<Vec<_>>())
        .unwrap_or_else(|e| serde_json::json!({"error": e}));

    // fidelity: the sketch against the real render, shrunk to the proxy size
    if let Some(v) = vs {
        let mut d = raea_media::backend("ffmpeg")?.open_video(Path::new(&v))?;
        let mut vals = Vec::new();
        let mut pairs = Vec::new();
        for f in (0..n).step_by(30) {
            let fr = d.frame_at(f as f64 / prog.fps)?;
            let real = previz::shrink_pub(&fr.rgba, fr.width as usize, fr.height as usize);
            let rgba = |p: &Pic| p.rgb.chunks(3).flat_map(|c| [c[0], c[1], c[2], 255]).collect::<Vec<u8>>();
            vals.push(raea_media::ssim(&rgba(&pics[f]), &rgba(&real), PW, PH));
            let (lr, lp) = (look.hv(&real), &looks[f]);
            pairs.push(k::hv_sim(&lr, lp) as f64);
            if f % 120 == 0 {
                pairs.len();
                save(&sheet(&[pics[f].clone(), real], 2), &out.join(format!("vs-{f:05}.png")))?;
            }
        }
        j["vs_render"] = serde_json::json!({"ssim_mean": vals.iter().sum::<f64>() / vals.len() as f64, "ssim_min": vals.iter().cloned().fold(1.0, f64::min),
            "look_sim_mean": pairs.iter().sum::<f64>() / pairs.len() as f64});
    }

    // a snippet: which frames it changes (rows), found by comparing what the edit says, and the cost to re-sketch them
    if let Some(sql) = snippet {
        let src = std::fs::read_to_string(edit).map_err(|e| e.to_string())?;
        let p2 = ir::compile(&format!("{src}\n{sql}"))?;
        let rows2 = inserted_of(&p2);
        let t = std::time::Instant::now();
        let st2 = states(&p2, &rows2)?;
        let changed: Vec<usize> = (0..n).filter(|&f| row_hv(&p2, &st2[f], f as i64).0 != says[f].0).collect();
        let mut px2 = px;
        let before = px2.decoded;
        previz::extend(&mut px2, &p2, &st2, &dir, &engine::media_backend())?;
        let newly = px2.decoded - before;
        let re: Vec<Pic> = changed.iter().map(|&f| sketch(&p2, &st2[f], f as i64, &px2)).collect();
        let s2 = t.elapsed().as_secs_f64();
        let first = changed.first().copied();
        if let Some(f) = first {
            save(&sheet(&[pics[f].clone(), re[0].clone()], 2), &out.join("snippet-before-after.png"))?;
        }
        j["snippet"] = serde_json::json!({"sql": sql, "frames_changed": changed.len(), "first": first, "last": changed.last(), "seconds_to_find_and_resketch": s2, "new_proxies_decoded": newly});
    }
    std::fs::write(out.join("previz.json"), serde_json::to_string_pretty(&j).unwrap()).map_err(|e| e.to_string())?;
    println!("{}", serde_json::to_string_pretty(&j).unwrap());
    Ok(())
}

/// Screen direction and matches at each cut (the catalogue; see the call in `run`). Its own function: inline, it
/// pushed `run`'s frame over the main thread's stack.
#[inline(never)]
fn screen_direction(prog: &Program, st: &[Vec<Tup>], pics: &[previz::Pic], report_cuts: &[serde_json::Value], n: usize) -> serde_json::Value {
        let mut ztr = std::collections::HashMap::new();
        let mut face_x = |f: usize| -> Option<f64> {
            previz::zones(prog, &st[f], f as i64, &mut ztr).faces.iter().max_by(|a, b| (a.0[2] * a.0[3]).total_cmp(&(b.0[2] * b.0[3]))).map(|(b, _)| b[0] + b[2] / 2.0)
        };
        let win = (0.3 * prog.fps).round() as usize;
        let top_src = |f: usize| previz::top_source(prog, &st[f], f as i64).map(|x| x.0);
        let bounds: Vec<usize> = std::iter::once(0).chain(report_cuts.iter().filter_map(|c| c["frame"].as_u64().map(|f| f as usize))).chain(std::iter::once(n)).collect();
        let mut rows_out = Vec::new();
        for (i, c) in report_cuts.iter().enumerate() {
            let Some(cf) = c["frame"].as_u64().map(|f| f as usize) else { continue };
            let (a, b) = (cf.saturating_sub(win).max(bounds[i]), (cf + win).min(bounds[i + 2]));
            if cf - a < 3 || b - cf < 3 { continue }
            let fo: Vec<Option<f64>> = (a..cf).map(&mut face_x).collect();
            let fi: Vec<Option<f64>> = (cf..b).map(&mut face_x).collect();
            let po: Vec<&previz::Pic> = (a..cf).map(|f| &pics[f]).collect();
            let pi: Vec<&previz::Pic> = (cf..b).map(|f| &pics[f]).collect();
            let (vo, bo) = crate::direction::way(&fo, &po, prog.fps);
            let (vi, bi) = crate::direction::way(&fi, &pi, prog.fps);
            let acr = crate::direction::across(vo, vi);
            let ratio = if vo.abs() > 1e-9 { vi / vo } else { 0.0 };
            let new_source = top_src(cf - 1) != top_src(cf);
            rows_out.push(serde_json::json!({"frame": cf, "out": (vo * 100.0).round() / 100.0, "in": (vi * 100.0).round() / 100.0, "basis": [bo, bi], "across": acr,
                "match_on_action": acr == "kept" && (0.5..=2.0).contains(&ratio), "graphic_match": new_source && c["reads_as_cut"] == false}));
        }
        let count = |k: &str| rows_out.iter().filter(|r| r["across"] == k).count();
        serde_json::json!({"cuts": rows_out.len(), "kept": count("kept"), "reversed": count("reversed"), "still": count("still"),
            "from_still": count("from still"), "to_still": count("to still"),
            "matches_on_action": rows_out.iter().filter(|r| r["match_on_action"] == true).count(),
            "graphic_matches": rows_out.iter().filter(|r| r["graphic_match"] == true).count(), "at_cuts": rows_out})
    }

/// Diegetic source mismatch, the mouth (the catalogue): spans of 0.6 s or more where the top clip's mouth moves (a step
/// over 0.03 in openness against the mesh sample 1/8 s before) while the voice is silent (20 dB under its talking
/// median), anywhere in a shot, not only at its edges; gaps of up to 4 frames are bridged. Shorter spans are a breath
/// before a phrase (the UGC take's run 0.3-0.5 s, the mouth opening to 0.11-0.14 with no sound), not a mismatch. The
/// voice is mixline::voice_levels' (the clip lanes and audio rows playing a clip's sound); a voice playing the wrong
/// moment is the diegetic check's "out of step". Impacts and instruments need
/// a sound-event match (CLAP) and aren't read yet.
#[inline(never)]
pub fn silent_mouths(prog: &Program, st: &[Vec<Tup>], rows: &[Tup], dir: &Path) -> serde_json::Value {
    let n = st.len();
    let Ok((voice, talk)) = crate::mixline::voice_levels(prog, rows, dir) else { return serde_json::json!({"error": "no voice"}) };
    let mut tracks: std::collections::HashMap<String, Option<(f64, Vec<Option<f64>>)>> = std::collections::HashMap::new();
    let mut mouth_at = |f: usize, back: f64| -> Option<f64> {
        let (hash, t) = previz::top_source(prog, &st[f], f as i64)?;
        let (fps, tr) = tracks.entry(hash.clone()).or_insert_with(|| previz::mouth_track(&hash)).clone()?;
        tr.get((((t - back) * fps).round().max(0.0) as usize).min(tr.len().checked_sub(1)?)).copied().flatten()
    };
    let flagged: Vec<bool> = (0..n).map(|f| {
        let moving = matches!((mouth_at(f, 0.0), mouth_at(f, 0.125)), (Some(a), Some(b)) if (a - b).abs() > 0.03);
        let (i0, i1) = ((f as f64 / prog.fps * 100.0) as usize, (((f + 1) as f64 / prog.fps) * 100.0).ceil() as usize);
        let silent = !voice.is_empty() && voice.get(i0..i1.min(voice.len())).is_some_and(|w| !w.is_empty() && w.iter().all(|l| *l < talk - 20.0));
        moving && silent
    }).collect();
    let mut spans: Vec<(usize, usize)> = Vec::new();
    for (f, on) in flagged.iter().enumerate() {
        if !on { continue }
        match spans.last_mut() {
            Some(s) if f <= s.1 + 5 => s.1 = f,
            _ => spans.push((f, f)),
        }
    }
    let beat = (0.6 * prog.fps).round() as usize;
    let long: Vec<[usize; 2]> = spans.iter().filter(|s| s.1 - s.0 + 1 >= beat).map(|s| [s.0, s.1]).collect();
    serde_json::json!({"spans": long, "frames": long.iter().map(|s| s[1] - s[0] + 1).sum::<usize>(), "shorter": spans.len() - long.len()})
}

/// A source's person, depth and sharpness grids from its ingest (matte.json, matte.bin, depth.bin, sharp.bin:
/// perception's matte step, docs/the roadmap G5), and its picture's size.
struct Matte {
    fps: f64,
    w: usize,
    h: usize,
    sw: f64,
    sh: f64,
    faces: Vec<Vec<[f64; 4]>>,
    matte: Vec<u8>,
    depth: Vec<u8>,
    sharp: Vec<u8>,
}

fn matte_track(hash: &str) -> Option<Matte> {
    let dir = previz::ingest_dir(hash)?;
    let j: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(dir.join("matte.json")).ok()?).ok()?;
    let facts: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(dir.join("facts.json")).ok()?).ok()?;
    let faces = j["frames"].as_array()?.iter().map(|f| f["faces"].as_array().into_iter().flatten().filter_map(|b| {
        Some([b.get(0)?.as_f64()?, b.get(1)?.as_f64()?, b.get(2)?.as_f64()?, b.get(3)?.as_f64()?])
    }).collect()).collect();
    Some(Matte { fps: j["fps"].as_f64()?, w: j["w"].as_u64()? as usize, h: j["h"].as_u64()? as usize, sw: facts["width"].as_f64()?, sh: facts["height"].as_f64()?, faces,
        matte: std::fs::read(dir.join("matte.bin")).ok()?, depth: std::fs::read(dir.join("depth.bin")).ok()?, sharp: std::fs::read(dir.join("sharp.bin")).ok()? })
}

fn val(v: &std::collections::BTreeMap<String, V>, k: &str, d: f64) -> f64 {
    match v.get(k) {
        Some(V::N(x)) => *x,
        _ => d,
    }
}

/// G5's depth touches, measured against bounds before anything renders:
/// - text behind the head (a text row with `behind = 'person'`): the share of the text's box the person matte covers,
///   at its worst frame; under 0.03 the touch does nothing ("none"), up to 0.25 it reads as a look ("ok"), up to 0.5
///   it's "borderline", past that it hides the words ("fail")
/// - a focus blur (a clip with `focus_blur` set): the face's sharpness over the background's (the person matte's
///   outside), the median over the clip's samples; 2 or more sets the subject apart ("ok"), 1 to 2 is "borderline"
///   (the take as shot already reads 1.7: a blur that doesn't beat that adds nothing, or its plane missed), under 1 the
///   focus missed the face ("fail"); and how far the sharpest tenth of the picture sits from the face in depth (0-1 of
///   the nearest)
pub fn depth_touches(prog: &Program, st: &[Vec<Tup>]) -> serde_json::Value {
    let (cwid, chei) = (prog.width as f64, prog.height as f64);
    let mut tracks: std::collections::HashMap<String, Option<Matte>> = std::collections::HashMap::new();
    // (the top clip's matte sample index and its canvas -> source-cell map) at frame f
    let mut texts: std::collections::BTreeMap<String, Vec<f64>> = std::collections::BTreeMap::new();
    let mut focus: std::collections::BTreeMap<String, Vec<(f64, f64)>> = std::collections::BTreeMap::new();
    for f in 0..st.len() {
        let Some(clip) = st[f].iter().filter(|t| t.kind == "clip" && previz::showing(t, f as i64)).max_by(|a, b| val(&a.vals, "z", 0.0).total_cmp(&val(&b.vals, "z", 0.0)).then(a.id.cmp(&b.id))) else { continue };
        let v = &clip.vals;
        let Some(V::S(src)) = v.get("src") else { continue };
        let hash = src.strip_prefix("media:").unwrap_or(src).to_string();
        let Some(m) = tracks.entry(hash.clone()).or_insert_with(|| matte_track(&hash)).as_ref() else { continue };
        let at = val(v, "src_in", 0.0) + (f as f64 - val(v, "from", 0.0)) / prog.fps;
        let samples = m.faces.len();
        if samples == 0 {
            continue;
        }
        let i = ((at * m.fps).round().max(0.0) as usize).min(samples - 1);
        let cell = |gx: usize, gy: usize| i * m.w * m.h + gy * m.w + gx;
        let (bx, by, bw, bh) = (val(v, "x", 0.0), val(v, "y", 0.0), val(v, "w", cwid), val(v, "h", chei));
        let (cx, cy, cw, ch) = engine::cover(m.sw, m.sh, bw, bh, val(v, "zoom", 1.0).max(0.01), (val(v, "pan_x", 0.0), val(v, "pan_y", 0.0)));
        // canvas px -> the source's grid cell
        let to_cell = |x: f64, y: f64| -> Option<(usize, usize)> {
            let (sx, sy) = ((cx - cw / 2.0 + (x - bx) / bw * cw) / m.sw, (cy - ch / 2.0 + (y - by) / bh * ch) / m.sh);
            ((0.0..1.0).contains(&sx) && (0.0..1.0).contains(&sy)).then(|| ((sx * m.w as f64) as usize, (sy * m.h as f64) as usize))
        };
        for t in st[f].iter().filter(|t| matches!(t.kind.as_str(), "caption" | "title" | "text") && previz::showing(t, f as i64)) {
            if !matches!(t.vals.get("behind"), Some(V::S(b)) if b == "person") {
                continue;
            }
            let tv = &t.vals;
            let (size, pad) = (val(tv, "size", 48.0), val(tv, "padding", 0.0));
            let (x, y, w) = (val(tv, "x", 0.0), val(tv, "y", 0.0), val(tv, "w", cwid));
            let n = match tv.get("text") { Some(V::S(s)) => s.chars().count(), _ => 0 };
            let per_line = ((w - 2.0 * pad) / (size * 0.5)).max(1.0) as usize;
            let lines = n.div_ceil(per_line).max(1);
            let lw = ((n.min(per_line)) as f64 * size * 0.5).min(w - 2.0 * pad);
            let x0 = if matches!(tv.get("align"), Some(V::S(a)) if a == "center") { x + (w - lw) / 2.0 } else { x + pad };
            let (y0, y1) = (y + pad + size * 0.3, y + pad + (lines - 1) as f64 * size * 1.2 + size * 0.92);
            let (mut covered, mut all) = (0usize, 0usize);
            let mut yy = y0;
            while yy < y1 {
                let mut xx = x0;
                while xx < x0 + lw {
                    if let Some((gx, gy)) = to_cell(xx, yy) {
                        all += 1;
                        covered += (m.matte[cell(gx, gy)] > 127) as usize;
                    }
                    xx += 4.0;
                }
                yy += 4.0;
            }
            if all > 0 {
                texts.entry(t.id.clone()).or_default().push(covered as f64 / all as f64);
            }
        }
        if v.contains_key("focus_blur") {
            let Some(face) = m.faces[i].iter().max_by(|a, b| (a[2] * a[3]).total_cmp(&(b[2] * b[3]))) else { continue };
            let in_face = |gx: usize, gy: usize| {
                let (fx, fy) = ((gx as f64 + 0.5) / m.w as f64, (gy as f64 + 0.5) / m.h as f64);
                fx >= face[0] && fx <= face[0] + face[2] && fy >= face[1] && fy <= face[1] + face[3]
            };
            let (mut fs, mut fnn, mut bs, mut bn, mut fd) = (0f64, 0f64, 0f64, 0f64, 0f64);
            let mut by_sharp: Vec<(u8, u8)> = Vec::new();
            for gy in 0..m.h {
                for gx in 0..m.w {
                    let c = cell(gx, gy);
                    by_sharp.push((m.sharp[c], m.depth[c]));
                    if in_face(gx, gy) {
                        fs += m.sharp[c] as f64;
                        fd += m.depth[c] as f64;
                        fnn += 1.0;
                    } else if m.matte[c] < 64 {
                        bs += m.sharp[c] as f64;
                        bn += 1.0;
                    }
                }
            }
            if fnn > 0.0 && bn > 0.0 {
                by_sharp.sort_by(|a, b| b.0.cmp(&a.0));
                let top = &by_sharp[..(by_sharp.len() / 10).max(1)];
                let sharp_depth = top.iter().map(|q| q.1 as f64).sum::<f64>() / top.len() as f64;
                focus.entry(clip.id.clone()).or_default().push(((fs / fnn) / (bs / bn).max(0.5), ((sharp_depth - fd / fnn) / 255.0).abs()));
            }
        }
    }
    let r3 = |x: f64| (x * 1000.0).round() / 1000.0;
    let median = |mut v: Vec<f64>| { v.sort_by(|a, b| a.total_cmp(b)); v.get(v.len() / 2).copied().unwrap_or(0.0) };
    let text_rows: Vec<serde_json::Value> = texts.into_iter().map(|(id, s)| {
        let worst = s.iter().cloned().fold(0.0, f64::max);
        let verdict = if worst < 0.03 { "none" } else if worst <= 0.25 { "ok" } else if worst <= 0.5 { "borderline" } else { "fail" };
        serde_json::json!({"id": id, "frames": s.len(), "worst_share": r3(worst), "mean_share": r3(s.iter().sum::<f64>() / s.len() as f64), "verdict": verdict})
    }).collect();
    let focus_rows: Vec<serde_json::Value> = focus.into_iter().map(|(id, s)| {
        let ratio = median(s.iter().map(|q| q.0).collect());
        let off = median(s.iter().map(|q| q.1).collect());
        let verdict = if ratio >= 2.0 { "ok" } else if ratio >= 1.0 { "borderline" } else { "fail" };
        serde_json::json!({"id": id, "frames": s.len(), "face_over_background": r3(ratio), "sharpest_depth_from_face": r3(off), "verdict": verdict})
    }).collect();
    serde_json::json!({"text_behind": text_rows, "focus_blur": focus_rows})
}
