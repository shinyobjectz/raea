//! The energy map from a song's features: per-beat energy, sections, builds, drops, stops, hits, dips and pacing.

use super::*;

/// The downbeats without the model's strays: one or two downbeats off the bar's phase, between two that are whole
/// bars apart, are that stretch's bars counted from the beats. Beat This! marks a downbeat mid-bar here and there, and
/// where it does moves with a 12 ms decoder offset, so the same song had 78 bars on a Mac and 75 elsewhere (#49). A
/// phase that holds for three downbeats is the music's (a dropped beat, a change of meter) and stays.
pub(super) fn steady(down: &[usize], bpb: usize) -> Vec<usize> {
    const HOLD: usize = 3;
    let mut out: Vec<usize> = Vec::new();
    let mut i = 0;
    while i < down.len() {
        let Some(&a) = out.last() else {
            out.push(down[i]);
            i += 1;
            continue;
        };
        if (down[i] - a) % bpb == 0 {
            out.push(down[i]);
            i += 1;
        } else if let Some(j) = (i..down.len().min(i + HOLD)).find(|j| (down[*j] - a) % bpb == 0) {
            out.extend((a + bpb..down[j]).step_by(bpb));
            i = j;
        } else {
            out.push(down[i]);
            i += 1;
        }
    }
    out
}

/// The map from a song's 22 050 Hz mono samples, its log-mel frames, beats and downbeats (seconds) and any
/// `[Section]` markers.
pub fn analyse(x: &[f32], spect: &[[f32; MELS]], beats: &[f64], downs: &[f64], marks: &[(f64, String)]) -> Value {
    let duration = x.len() as f64 / SR;
    let n = beats.len();
    if n < 8 {
        return json!({"version": 1, "duration": r3(duration), "tempo": null, "beatsPerBar": 4, "beats": beats, "downbeats": downs,
            "features": {}, "sections": [], "events": [], "bars": [], "note": "too few beats for a map"});
    }
    let mut gaps: Vec<f64> = beats.windows(2).map(|w| w[1] - w[0]).collect();
    gaps.sort_by(f64::total_cmp);
    let median = gaps[gaps.len() / 2];
    // a beat closer than 0.6 of the usual gap to the one before is a stray (sparse intros): dropped
    let mut grid: Vec<f64> = Vec::with_capacity(n);
    for &t in beats {
        if grid.last().is_none_or(|l| t - l >= 0.6 * median) {
            grid.push(t);
        }
    }
    let beats = &grid[..];
    let n = beats.len();

    // each downbeat's beat index, and the bar length most bars have
    let mut down_i: Vec<usize> = downs.iter().map(|d| nearest(beats, *d)).collect();
    down_i.dedup();
    // a downbeat closer than two beats to the one before is a stray too
    let mut kept_d: Vec<usize> = Vec::new();
    for d in down_i {
        if kept_d.last().is_none_or(|l| d >= l + 2) {
            kept_d.push(d);
        }
    }
    let down_i = kept_d;
    // the tempo: a line fitted through the beats against their count (a missed beat counted by its gap). Never a
    // median of single gaps, which 20 ms frames quantise (0.46 / 0.44 s read as one tempo or the other); nor a line
    // through the downbeats against their bar count, which a song whose downbeats come two beats apart in places
    // counts as whole bars: 99 BPM read as 112.8 or 116.6, by where a 12 ms decoder offset put three downbeats (#49)
    let fit = |pts: &[(f64, f64)]| -> Option<f64> {
        if pts.len() < 3 {
            return None;
        }
        let (mx, my) = (mean(&pts.iter().map(|p| p.0).collect::<Vec<_>>()), mean(&pts.iter().map(|p| p.1).collect::<Vec<_>>()));
        let (sxy, sxx) = pts.iter().fold((0.0, 0.0), |(a, c), (x, y)| (a + (x - mx) * (y - my), c + (x - mx) * (x - mx)));
        (sxx > 0.0).then(|| sxy / sxx)
    };
    let gap = {
        let mut k = 0.0;
        let pts: Vec<(f64, f64)> = (0..n).map(|i| { if i > 0 { k += ((beats[i] - beats[i - 1]) / median).round().max(1.0); } (k, beats[i]) }).collect();
        fit(&pts).unwrap_or(median)
    };
    let tempo = 60.0 / gap;
    let mut counts = [0usize; 9];
    for w in down_i.windows(2) {
        let c = w[1] - w[0];
        if c < counts.len() {
            counts[c] += 1;
        }
    }
    let bpb = (2..counts.len()).max_by_key(|c| counts[*c]).filter(|c| counts[*c] > 0).unwrap_or(4);
    let down_i = steady(&down_i, bpb);
    // bars: one per downbeat, and a pickup before the first
    let mut bar_starts: Vec<usize> = Vec::new();
    if let Some(&first) = down_i.first() {
        if first > 0 {
            bar_starts.push(0);
        }
    }
    bar_starts.extend(&down_i);
    if bar_starts.is_empty() {
        bar_starts = (0..n).step_by(bpb).collect();
    }

    let f = frames(x, spect);
    let b = per_beat(&f, beats, duration);
    let time = |i: usize| if i < n { beats[i] } else { duration };
    let bar_of = |i: usize| bar_starts.iter().rposition(|s| *s <= i).unwrap_or(0);

    // ---- bars and pacing
    let bar_range = |j: usize| (bar_starts[j], if j + 1 < bar_starts.len() { bar_starts[j + 1] } else { n });
    let bars: Vec<(usize, usize, f64, f64, f64)> = (0..bar_starts.len())
        .map(|j| {
            let (s, e) = bar_range(j);
            let (en, on) = (mean(&b.energy[s..e]), mean(&b.onsets[s..e]));
            (s, e, en, on, rate(en, on))
        })
        .collect();

    // ---- sections: novelty boundaries, and the markers when there are any
    let nov = novelty(spect, beats, &b, 16);
    let dn: Vec<f64> = down_i.iter().map(|i| nov[*i]).collect();
    let thr = mean(&dn) + 0.25 * (dn.iter().map(|v| (v - mean(&dn)).powi(2)).sum::<f64>() / dn.len().max(1) as f64).sqrt();
    let mut cand: Vec<(usize, f64)> = down_i
        .iter()
        .enumerate()
        .filter(|(k, i)| {
            let v = nov[**i];
            v > thr && (k.saturating_sub(2)..(k + 3).min(down_i.len())).all(|q| nov[down_i[q]] <= v)
        })
        .map(|(_, i)| (*i, nov[*i]))
        .collect();
    cand.sort_by(|a, c| c.1.total_cmp(&a.1));
    let mut cuts: Vec<(usize, f64)> = Vec::new();
    for (i, v) in cand {
        if i >= 2 * bpb && i + 2 * bpb < n && cuts.iter().all(|(c, _)| c.abs_diff(i) >= 4 * bpb) {
            cuts.push((i, v));
        }
    }
    cuts.sort_by_key(|c| c.0);
    let nmax = nov.iter().copied().fold(1e-9, f64::max);
    let boundaries: Vec<Value> = cuts.iter().map(|(i, v)| json!({"time": r3(beats[*i]), "beat": i, "bar": bar_of(*i), "strength": r3(v / nmax)})).collect();

    // a section's start beat: a marker snaps to the nearest downbeat within half a bar, else the nearest beat
    let snap_start = |t: f64| -> usize {
        let bi = nearest(beats, t);
        let di = down_i.iter().copied().min_by(|a, c| (beats[*a] - t).abs().total_cmp(&(beats[*c] - t).abs()));
        match di {
            Some(d) if (beats[d] - t).abs() <= gap * bpb as f64 / 2.0 => d,
            _ => bi,
        }
    };
    let mut starts: Vec<(usize, Option<String>)> = if marks.is_empty() {
        std::iter::once((0, None)).chain(cuts.iter().map(|(i, _)| (*i, None))).collect()
    } else {
        let mut s: Vec<(usize, Option<String>)> = marks.iter().map(|(t, name)| (snap_start(*t), Some(name.clone()))).collect();
        if s[0].0 > bpb {
            s.insert(0, (0, Some("Intro".into())));
        } else {
            s[0].0 = 0;
        }
        s
    };
    starts.dedup_by(|a, c| a.0 == c.0);
    let song_mean = mean(&b.energy);
    let sections: Vec<(usize, usize, Option<String>, f64, &str, f64)> = (0..starts.len())
        .map(|k| {
            let (s, e) = (starts[k].0, if k + 1 < starts.len() { starts[k + 1].0 } else { n });
            let en = mean(&b.energy[s..e.max(s + 1).min(n)]);
            let level = if k == 0 && en < song_mean && starts.len() > 2 {
                "intro"
            } else if k + 1 == starts.len() && en < song_mean && starts.len() > 2 {
                "outro"
            } else if en < 0.35 {
                "low"
            } else if en < 0.6 {
                "mid"
            } else {
                "high"
            };
            // the section's pace: the median of its bars' rates
            let mut rs: Vec<f64> = bars.iter().filter(|bar| bar.0 >= s && bar.0 < e).map(|bar| bar.4).collect();
            rs.sort_by(f64::total_cmp);
            let pace = rs.get(rs.len() / 2).copied().unwrap_or(rate(en, 0.5));
            (s, e, starts[k].1.clone(), en, level, pace)
        })
        .collect();

    // ---- events
    let raw = &b.raw;
    let low = &b.low;
    let win = |v: &[f64], a: i64, c: i64| -> f64 {
        let (a, c) = (a.clamp(0, n as i64) as usize, c.clamp(0, n as i64) as usize);
        if c > a { mean(&v[a..c]) } else { 0.0 }
    };
    let bp = bpb as i64;
    let mut events: Vec<Value> = Vec::new();

    // the frames' onsets: peaks (the maximum within 80 ms either side)
    let nf = f.flux.len();
    let flux_hi = percentile(&f.flux, 99.0).max(1e-9);
    let flux_p95 = percentile(&f.flux, 95.0);
    let is_peak = |t: usize| {
        let (lo, hi) = (t.saturating_sub(4), (t + 5).min(nf));
        f.flux[t] > 0.0 && f.flux[lo..hi].iter().all(|v| *v <= f.flux[t])
    };
    // the strongest onset within 3 frames of a time
    let flux_near = |t: f64| -> f64 {
        let c = (t * FPS).round() as i64;
        (c - 3..=c + 3).filter(|k| *k >= 0 && (*k as usize) < nf).map(|k| f.flux[k as usize]).fold(0.0, f64::max)
    };
    // a time on the beat grid: the nearest beat, or the half beat when that's nearer
    let snap = |t: f64| -> f64 {
        let i = nearest(beats, t);
        let half = if beats[i] <= t && i + 1 < n { (beats[i] + beats[i + 1]) / 2.0 } else if i > 0 { (beats[i - 1] + beats[i]) / 2.0 } else { beats[i] };
        if (half - t).abs() < (beats[i] - t).abs() { half } else { beats[i] }
    };
    let hit_json = |t: f64, v: f64| json!({"time": r3(snap(t)), "at": r3(t), "strength": r3((v / flux_hi).min(1.0))});

    // stops: the full beat drops out (the low end, kick and bass, gone; the energy down) for one to eight beats and
    // comes back, leaving sparse hits
    let mut stops: Vec<(usize, usize, f64)> = Vec::new();
    let mut i = bpb;
    while i < n {
        let (rl, re) = (win(low, i as i64 - bp, i as i64), win(raw, i as i64 - bp, i as i64));
        let out = |k: usize| low[k] < 0.45 * rl || raw[k] < 0.5 * re;
        if rl > 0.35 && re > 0.25 && out(i) && !out(i - 1) {
            let mut e = i;
            while e < n && e - i < 2 * bpb && out(e) {
                e += 1;
            }
            let back = e < n && (low[e] >= 0.6 * rl || raw[e] >= 0.7 * re);
            let lands = down_i.contains(&e) && (raw[e] > 1.2 * re || low[e] > 0.9 * rl);
            if back && e - i <= 2 * bpb && (e - i >= 2 || lands) {
                stops.push((i, e, (re - win(raw, i as i64, e as i64)).max(0.0)));
                i = e;
                continue;
            }
        }
        i += 1;
    }

    // drops: on a downbeat, the bar after against the bar before (a slam), and the downbeat's own impact: the low end's
    // and the loudness's peak in its first 160 ms against the beats' peaks in the bar before (a chorus that lands on
    // one big hit and then cuts the band is a drop too: it doesn't hold)
    let db = |v: f64| 10.0 * (v + 1e-10).log10();
    let peak_at = |v: &[f64], t: f64| -> f64 {
        let c = (t * FPS).round() as i64;
        (c - 2..=c + 8).filter(|k| *k >= 0 && (*k as usize) < nf).map(|k| db(v[k as usize])).fold(-200.0, f64::max)
    };
    // against the beats of the bar before `from` (a drop after a stop is measured against the bar before the stop:
    // the beat only coming back isn't a drop)
    let impact_at = |p: usize, from: usize| -> f64 {
        // over the loudest beat of that bar: a verse's downbeat kick over its other beats isn't an impact
        let refs = |v: &[f64]| (from.saturating_sub(bpb)..from).map(|i| peak_at(v, beats[i])).fold(-200.0, f64::max);
        let (dl, dr) = (peak_at(&f.low, beats[p]) - refs(&f.low), peak_at(&f.rms, beats[p]) - refs(&f.rms));
        0.6 * (dl / 9.0).clamp(0.0, 1.0) + 0.4 * (dr / 4.0).clamp(0.0, 1.0)
    };
    let stop_into = |p: usize| stops.iter().find(|s| s.1 == p || s.1 + 1 == p).copied();
    // the breath before a drop: the quietest 40 ms of the 200 ms before the downbeat under the bar's usual level, 0..1
    // over 6 dB
    let gap_before = |p: usize| -> f64 {
        let c = (beats[p] * FPS).round() as usize;
        if c < 10 {
            return 0.0;
        }
        let quiet = (c - 10..c - 1).map(|k| db(mean(&f.rms[k..k + 2]))).fold(f64::INFINITY, f64::min);
        let a = ((beats[p.saturating_sub(bpb)] * FPS).round() as usize).min(c - 10);
        let usual = percentile(&f.rms[a..c - 10].iter().map(|v| db(*v)).collect::<Vec<_>>(), 50.0);
        ((usual - quiet) / 6.0).clamp(0.0, 1.0)
    };
    // a downbeat where the music changes (novelty, 0..1) lands harder
    let drop_base: Vec<(usize, f64, f64, f64)> = down_i
        .iter()
        .filter(|p| **p >= bpb && **p + 1 < n)
        .map(|&p| {
            let from = stop_into(p).map(|s| s.0).unwrap_or(p);
            let fi = from as i64;
            let jump = win(raw, p as i64, p as i64 + bp) - win(raw, fi - bp, fi);
            let impact = impact_at(p, from);
            (p, 0.45 * jump.max(0.0) + 0.3 * impact + 0.15 * nov[p] / nmax + 0.1 * gap_before(p), jump, impact)
        })
        .collect();

    // builds: what leads into a drop, a section or a stop, rising for two bars or more: the energy with the high band
    // and brightness (a riser's sweep), bar by bar
    let combo: Vec<f64> = (0..n).map(|i| 0.45 * raw[i] + 0.3 * b.high[i] + 0.25 * b.bright[i]).collect();
    let c = smooth(&combo, 1.5);
    let mut arrivals: Vec<usize> = drop_base.iter().filter(|d| d.1 > 0.06).map(|d| d.0).chain(sections.iter().map(|s| s.0)).chain(stops.iter().map(|s| s.0)).collect();
    arrivals.sort();
    arrivals.dedup();
    // every arrival's best build, then the strongest kept where they overlap (one that ends on a section's start or a
    // likely drop first)
    let bar_c = |s: usize| win(&c, s as i64, s as i64 + bp);
    let mut cands: Vec<(usize, usize, f64, f64)> = Vec::new();
    for &a in &arrivals {
        if a < 2 * bpb {
            continue;
        }
        // a build runs into the stop before its drop, not through it
        let e = stop_into(a).map(|s| s.0).unwrap_or(a);
        let end_c = win(&c, e as i64 - bp, e as i64);
        // a build lives in the section it leads out of (a pre-chorus, a bridge's second half)
        let floor = sections.iter().map(|x| x.0).filter(|x| *x < e).max().unwrap_or(0);
        // a gentle ramp counts when it lands on a new section (a loud pre-chorus has little headroom left)
        let min_rise = if sections.iter().any(|x| x.0 == a) { 0.05 } else { 0.1 };
        let mut best: Option<(usize, f64)> = None;
        for &s in down_i.iter().filter(|d| **d >= floor && **d + 2 * bpb <= e && **d + 16 * bpb >= e) {
            let rise = end_c - bar_c(s);
            // most bar-to-bar steps rise, and the build ends at its top
            let steps: Vec<f64> = (s..e.saturating_sub(bpb)).step_by(bpb).map(|k| bar_c(k + bpb) - bar_c(k)).collect();
            let up = steps.iter().filter(|d| **d > -0.02).count() as f64 / steps.len().max(1) as f64;
            let top = (s..e.saturating_sub(bpb)).step_by(bpb).map(bar_c).fold(0.0, f64::max);
            if up >= 0.7 && rise > min_rise && end_c >= top - 0.03 && best.is_none_or(|b| rise > b.1 + 0.02) {
                best = Some((s, rise));
            }
        }
        // a build doesn't run through a drop
        if let Some((s, rise)) = best
            && drop_base.iter().all(|d| d.0 <= s || d.0 >= e || d.1 < 0.4)
        {
            let lands = if sections.iter().any(|x| x.0 == a) { 0.15 } else if drop_base.iter().any(|d| d.0 == a && d.1 > 0.15) { 0.05 } else { 0.0 };
            cands.push((s, e, rise, rise + lands));
        }
    }
    cands.sort_by(|a, c| c.3.total_cmp(&a.3));
    let mut builds: Vec<(usize, usize, f64)> = Vec::new();
    for (s, e, rise, _) in cands {
        if builds.iter().all(|b| e <= b.0 || s >= b.1) {
            builds.push((s, e, rise));
        }
    }
    builds.sort_by_key(|b| b.0);
    if raea_io_trace::stat("RAEA_SONG_DEBUG") {
        for &(p, base, jump, impact) in &drop_base {
            eprintln!("drop? {:7.2} base {base:.2} jump {jump:.2} impact {impact:.2} nov {:.2}", beats[p], nov[p] / nmax);
        }
    }
    // the drops: the base score, and what it ends (a build's rise, a stop)
    let mut drops: Vec<(usize, f64, f64)> = Vec::new();
    for &(p, base, jump, impact) in &drop_base {
        let build = builds.iter().filter(|b| b.1 <= p && b.1 + bpb >= p).map(|b| b.2).fold(0.0, f64::max);
        let section = sections.iter().any(|x| x.0 == p);
        let score = base + 0.25 * (2.0 * build).min(1.0) + if section { 0.1 } else { 0.0 };
        if score > 0.25 && raw[p] > 0.35 && (jump > 0.05 || impact > 0.1) {
            drops.push((p, score, jump));
        }
    }
    drops.sort_by(|a, c| c.1.total_cmp(&a.1));
    let mut kept: Vec<(usize, f64, f64)> = Vec::new();
    for d in drops {
        if kept.iter().all(|k| k.0.abs_diff(d.0) >= 4 * bpb) {
            kept.push(d);
        }
    }
    kept.sort_by_key(|d| d.0);
    let drops = kept;
    for &(s, e, rise) in &builds {
        let into = drops.iter().find(|d| d.0 >= e && d.0 <= e + 2 * bpb).map(|d| r3(beats[d.0]));
        events.push(json!({"kind": "build", "time": r3(beats[s]), "end": r3(time(e)), "beat": s, "bar": bar_of(s), "beats": e - s,
            "strength": r3(rise.min(1.0)), "into": into}));
    }
    for &(s, e, depth) in &stops {
        let mut hits: Vec<Value> = Vec::new();
        for t in ((time(s) - 0.05) * FPS) as usize..(((time(e) - 0.05) * FPS) as usize).min(nf) {
            if is_peak(t) && f.flux[t] > 0.7 * flux_p95 {
                let h = hit_json(t as f64 / FPS, f.flux[t]);
                match hits.last_mut() {
                    Some(l) if l["time"] == h["time"] => {
                        if h["strength"].as_f64() > l["strength"].as_f64() {
                            *l = h;
                        }
                    }
                    _ => hits.push(h),
                }
            }
        }
        let into = drops.iter().find(|d| d.0 == e || d.0 == e + 1).map(|d| r3(beats[d.0]));
        events.push(json!({"kind": "stop", "time": r3(time(s)), "end": r3(time(e)), "beat": s, "bar": bar_of(s), "beats": e - s,
            "strength": r3(depth.min(1.0)), "into": into, "hits": hits}));
    }
    for &(p, score, jump) in &drops {
        let after_stop = stops.iter().any(|s| s.1 == p || s.1 + 1 == p);
        let after_build = builds.iter().any(|b| b.1 <= p && b.1 + 2 * bpb >= p);
        // it holds when the bar after stays up; a drop that doesn't is one hit and then the band cuts
        let holds = jump > -0.05 && stops.iter().all(|s| s.0 > p + 1 || s.1 <= p);
        events.push(json!({"kind": "drop", "time": r3(beats[p]), "beat": p, "bar": bar_of(p), "strength": r3(score.min(1.0)),
            "rise": r3(jump), "holds": holds, "afterStop": after_stop, "afterBuild": after_build, "gapBefore": gap_before(p) > 0.5}));
    }

    // dips and breaks: a fall on a downbeat that holds two bars or more
    let mut dips: Vec<(usize, f64, f64)> = Vec::new();
    for &p in &down_i {
        let pi = p as i64;
        if pi < 2 * bp || p + 2 * bpb > n {
            continue;
        }
        let before = win(raw, pi - 2 * bp, pi);
        let after = win(raw, pi, pi + 2 * bp);
        let held = win(raw, pi, pi + 4 * bp);
        let fall = before - after;
        if fall > 0.18 && before - held > 0.12 && stops.iter().all(|s| p < s.0 || p >= s.1) {
            dips.push((p, fall, after));
        }
    }
    dips.sort_by(|a, c| c.1.total_cmp(&a.1));
    let mut dk: Vec<(usize, f64, f64)> = Vec::new();
    for d in dips {
        if dk.iter().all(|k| k.0.abs_diff(d.0) >= 4 * bpb) && drops.iter().all(|p| p.0.abs_diff(d.0) >= 2 * bpb) {
            dk.push(d);
        }
    }
    for (p, fall, after) in dk {
        // how long the fall holds: until a bar is back halfway, the next drop, or the next section
        let before = win(raw, p as i64 - 2 * bp, p as i64);
        let next = drops.iter().map(|d| d.0).chain(sections.iter().map(|x| x.0)).filter(|x| *x >= p + 2 * bpb).min().unwrap_or(n);
        let mut e = p + bpb;
        while e < next && win(raw, e as i64, e as i64 + bp) <= after + (before - after) / 2.0 {
            e += bpb;
        }
        let e = e.min(next).min(n);
        let kind = if after < 0.35 { "break" } else { "dip" };
        events.push(json!({"kind": kind, "time": r3(beats[p]), "end": r3(time(e)), "beat": p, "bar": bar_of(p), "beats": e - p, "strength": r3(fall.min(1.0))}));
    }

    // hits: accents that break the groove (a stab, a crash, a fill's last hit): an onset well over the same place in
    // the bars either side, a beat apart at least
    let bar_s = gap * bpb as f64;
    let mut hits: Vec<(f64, f64, f64)> = Vec::new();
    for t in 0..nf {
        if !is_peak(t) || f.flux[t] < flux_p95 {
            continue;
        }
        let ts = t as f64 / FPS;
        let mut around: Vec<f64> = [-2.0, -1.0, 1.0, 2.0].iter().map(|k| ts + k * bar_s).filter(|x| *x >= 0.0 && *x < duration).map(flux_near).collect();
        if around.len() < 2 {
            continue;
        }
        around.sort_by(f64::total_cmp);
        let usual = around[around.len() / 2].max(1e-9);
        let over = f.flux[t] / usual;
        let in_stop = stops.iter().any(|s| ts >= time(s.0) - 0.05 && ts < time(s.1) - 0.05);
        if over > 2.5 && !in_stop {
            hits.push((ts, f.flux[t], over));
        }
    }
    hits.sort_by(|a, c| (c.1 * c.2).total_cmp(&(a.1 * a.2)));
    let mut hk: Vec<(f64, f64, f64)> = Vec::new();
    for h in hits {
        if hk.iter().all(|k| (k.0 - h.0).abs() >= gap) {
            hk.push(h);
        }
    }
    hk.sort_by(|a, c| a.0.total_cmp(&c.0));
    for (t, v, over) in hk {
        let i = nearest(beats, t);
        let mut h = hit_json(t, v);
        h["kind"] = json!("hit");
        h["beat"] = json!(i);
        h["bar"] = json!(bar_of(i));
        h["over"] = json!(r3(over));
        events.push(h);
    }
    events.sort_by(|a, c| a["time"].as_f64().unwrap_or(0.0).total_cmp(&c["time"].as_f64().unwrap_or(0.0)));

    if let Some(p) = std::env::var_os("RAEA_SONG_FRAMES") {
        let mut csv = String::from("t,rms,low,high,centroid,flux\n");
        for t in 0..nf {
            csv += &format!("{:.2},{:.6},{:.6},{:.6},{:.3},{:.3}\n", t as f64 / FPS, f.rms[t], f.low[t], f.high[t], f.centroid[t], f.flux[t]);
        }
        let _ = std::fs::write(p, csv);
    }
    if raea_io_trace::stat("RAEA_SONG_DEBUG") {
        for (j, bar) in bars.iter().enumerate() {
            let (s, e) = (bar.0, bar.1);
            eprintln!("bar {j:3} {:7.2}s  E {:.2} raw {:.2} loud {:.2} low {:.2} on {:.2} br {:.2} hi {:.2} nov {:.2}  rate {}", beats[s],
                bar.2, mean(&raw[s..e]), mean(&b.loud[s..e]), mean(&b.low[s..e]), bar.3, mean(&b.bright[s..e]), mean(&b.high[s..e]), nov[s] / nmax, bar.4);
        }
    }

    let q = |v: &[f64]| v.iter().map(|x| r3(*x)).collect::<Vec<_>>();
    json!({
        "version": 1,
        "duration": r3(duration),
        "tempo": (tempo * 100.0).round() / 100.0,
        "beatsPerBar": bpb,
        "beats": q(beats),
        "downbeats": down_i.iter().map(|i| r3(beats[*i])).collect::<Vec<_>>(),
        "features": {"loudness": q(&b.loud), "onsets": q(&b.onsets), "brightness": q(&b.bright), "low": q(&b.low), "high": q(&b.high), "energy": q(&b.energy)},
        "bars": bars.iter().map(|bar| json!({"time": r3(beats[bar.0]), "beat": bar.0, "beats": bar.1 - bar.0, "energy": r3(bar.2), "cuts": bar.4})).collect::<Vec<_>>(),
        "sections": sections.iter().map(|s| json!({"name": s.2.clone().unwrap_or_else(|| s.4.to_string()), "level": s.4, "time": r3(time(s.0)), "end": r3(time(s.1)),
            "beat": s.0, "bar": bar_of(s.0), "energy": r3(s.3), "cuts": s.5})).collect::<Vec<_>>(),
        "sectionsFrom": if marks.is_empty() { "novelty" } else { "markers" },
        "boundaries": boundaries,
        "events": events,
    })
}
