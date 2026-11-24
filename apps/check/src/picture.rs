use super::*;

/// Encoder blotching in dark areas (found 10-24: VideoToolbox smears block-shaped patches over near-black where the
/// picture is flat). Per 8x8 block of luma, at one frame every two seconds: among dark blocks that are flat inside
/// (a range under `FLAT` levels), the share whose mean steps from a flat dark neighbour's by more than `STEP` and less
/// than `MOST` levels (bigger steps are the picture's own edges). Of those, only a patch's edges count: not steps that
/// run straight on for `RUN` blocks (the picture's own faint edges: a lossless night scene of dark panels scored 0.053
/// before, more than VideoToolbox's blotches), and only steps the level comes back from within `BACK` blocks (a glow's
/// 8-bit rings keep stepping one way: a lossless one scored 0.043 without). Calibrated 10-25 on 16 frames each: three
/// VideoToolbox exports (the tutorial, two night-market films) scored 0.012-0.014 (median), every x264 or lossless
/// encode at most 0.007.
pub(crate) const FLAT: u8 = 2;
pub(crate) const STEP: f32 = 0.5;
pub(crate) const MOST: f32 = 4.0;
pub(crate) const RUN: usize = 4;
pub(crate) const BACK: usize = 4;
pub(crate) const BLOTCHY: f64 = 0.009;

pub(crate) fn blotch_score(y: &[u8], w: usize, h: usize) -> Option<f64> {
    let (bw, bh) = (w / 8, h / 8);
    let mut mean = vec![0f32; bw * bh];
    let mut dark = vec![false; bw * bh];
    for by in 0..bh {
        for bx in 0..bw {
            let (mut sum, mut lo, mut hi) = (0u32, 255u8, 0u8);
            for r in 0..8 {
                let row = &y[(by * 8 + r) * w + bx * 8..][..8];
                for &v in row {
                    sum += v as u32;
                    lo = lo.min(v);
                    hi = hi.max(v);
                }
            }
            let m = sum as f32 / 64.0;
            mean[by * bw + bx] = m;
            dark[by * bw + bx] = m < 40.0 && hi - lo < FLAT;
        }
    }
    if dark.iter().filter(|d| **d).count() < 400 {
        return None;
    }
    // each pair's step, signed (0 when it isn't one): across the boundary to the right (h) and the one below (v)
    let step = |i: usize, j: usize| {
        let d = mean[j] - mean[i];
        if dark[i] && dark[j] && d.abs() > STEP && d.abs() < MOST { d.signum() as i8 } else { 0 }
    };
    let mut h = vec![0i8; bw * bh];
    let mut v = vec![0i8; bw * bh];
    let mut pairs = 0u32;
    for by in 0..bh {
        for bx in 0..bw {
            let i = by * bw + bx;
            if bx + 1 < bw {
                pairs += (dark[i] && dark[i + 1]) as u32;
                h[i] = step(i, i + 1);
            }
            if by + 1 < bh {
                pairs += (dark[i] && dark[i + bw]) as u32;
                v[i] = step(i, i + bw);
            }
        }
    }
    // A step that runs on, the same way, for RUN blocks along its boundary is an edge or a gradient band the picture
    // draws (a flat panel a level off its ground); the encoder's patches are ragged. Only those count.
    let ragged = |s: &[i8], along: usize, len: usize, i: usize, at: usize| {
        let (mut back, mut on) = (0, 0);
        while back < RUN && back < at && s[i - (back + 1) * along] == s[i] {
            back += 1;
        }
        while on < RUN && at + on + 1 < len && s[i + (on + 1) * along] == s[i] {
            on += 1;
        }
        back + on + 1 < RUN
    };
    // and a patch closes: within `BACK` blocks across, the level steps back the other way. A gradient's bands (a glow
    // drawn in 8 bits shows rings) keep stepping the same way.
    let closes = |s: &[i8], across: usize, len: usize, i: usize, at: usize| {
        (1..=BACK).any(|k| (k <= at && s[i - k * across] == -s[i]) || (at + k < len && s[i + k * across] == -s[i]))
    };
    let mut steps = 0u32;
    for by in 0..bh {
        for bx in 0..bw {
            let i = by * bw + bx;
            steps += (h[i] != 0 && ragged(&h, bw, bh, i, by) && closes(&h, 1, bw - 1, i, bx)) as u32;
            steps += (v[i] != 0 && ragged(&v, 1, bw, i, bx) && closes(&v, bw, bh - 1, i, by)) as u32;
        }
    }
    (pairs > 0).then(|| steps as f64 / pairs as f64)
}

/// The crf an x264 encode was made at, from the settings x264 writes into its stream ("x264 - core ... crf=12.0").
pub(crate) fn x264_crf(head: &[u8]) -> Option<f64> {
    let at = head.windows(11).position(|w| w == b"x264 - core")?;
    let opts = &head[at..(at + 4096).min(head.len())];
    let c = opts.windows(4).position(|w| w == b"crf=")?;
    let n: String = opts[c + 4..].iter().take_while(|b| b.is_ascii_digit() || **b == b'.').map(|b| *b as char).collect();
    n.parse().ok()
}

/// x264 at this crf or under keeps near-blacks (every one measured 10-24 and 10-25 was clean to the eye); what steps
/// in its darks is the picture's own (8-bit glows and faint panels), which blotch_score can't always tell apart
pub(crate) const X264_KEEPS: f64 = 16.0;

/// How far a clean encode moves a sample's blotching (x264 crf 12 over the walkaround's unencoded frames, 11-19: at most 0.005)
pub(crate) const BLOTCH_NOISE: f64 = 0.005;

/// Blotching's samples against the same frames of the composition drawn without encoding (raea-app-check-cause): None
/// when the video isn't a render in a raea project's record
fn blotching_cause(video: &str, probe: &Probe, scores: &[Option<f64>], w: usize, h: usize) -> Option<R<(cause::Against, Vec<Option<f64>>)>> {
    let (project, comp) = cause::composition_of(std::path::Path::new(video))?;
    Some((|| {
        if probe.fps <= 0.0 {
            return Err("the video doesn't say its frame rate".to_string());
        }
        let frames = cause::sampled(scores.len(), 2.0, probe.fps, (probe.picture * probe.fps).round() as i64 - 1);
        let me = std::env::current_exe().map_err(|e| e.to_string())?;
        let before: Vec<Option<f64>> = cause::unencoded(&me, &project, &comp, &frames, w, h)?.iter().map(|y| blotch_score(y, w, h)).collect();
        Ok((cause::against(scores, &before, BLOTCHY, BLOTCH_NOISE), before))
    })())
}

/// The crf an x264 encode that keeps the darks was made at: blotching isn't read for it
pub(crate) fn x264_kept(video: &str) -> Option<f64> {
    let mut head = Vec::new();
    if let Ok(f) = std::fs::File::open(video) {
        let _ = std::io::Read::read_to_end(&mut std::io::Read::take(f, 8 << 20), &mut head);
    }
    x264_crf(&head).filter(|c| *c <= X264_KEEPS)
}

pub(crate) fn blotching(video: &str, probe: &Probe, issues: &mut Vec<Issue>, passed: &mut Vec<String>) -> R<()> {
    if let Some(crf) = x264_kept(video) {
        passed.push(format!("blotching: encoded by x264 at crf {crf}, which keeps the darks"));
        return Ok(());
    }
    let (w, h) = (probe.w as usize / 8 * 8, probe.h as usize / 8 * 8);
    let mut scores: Vec<Option<f64>> = Vec::new();
    let mut frame = scorer(&mut scores, w, h);
    if crate::read::native() {
        crate::read::crops(video, w, h, &mut frame)?;
    } else {
        let mut child = Command::new("ffmpeg")
            .args(["-v", "error", "-i", video, "-vf", &format!("fps=0.5,crop={w}:{h}:0:0,format=gray"), "-f", "rawvideo", "-"])
            .stdout(std::process::Stdio::piped())
            .spawn()
            .map_err(|e| format!("ffmpeg: {e}"))?;
        let mut out = child.stdout.take().unwrap();
        let mut buf = vec![0u8; w * h];
        while std::io::Read::read_exact(&mut out, &mut buf).is_ok() {
            frame(&buf);
        }
        let _ = child.wait();
    }
    drop(frame);
    blotching_from(video, probe, &scores, issues, passed);
    Ok(())
}

/// Each grey crop's blotching, kept in `scores`
pub(crate) fn scorer(scores: &mut Vec<Option<f64>>, w: usize, h: usize) -> impl FnMut(&[u8]) + Send + '_ {
    move |buf| scores.push(blotch_score(buf, w, h))
}

/// What blotching's samples (one every 2 s) found
pub(crate) fn blotching_from(video: &str, probe: &Probe, scores: &[Option<f64>], issues: &mut Vec<Issue>, passed: &mut Vec<String>) {
    let (w, h) = (probe.w as usize / 8 * 8, probe.h as usize / 8 * 8);
    let (mut bad, mut seen, mut worst) = (Vec::new(), 0, 0f64);
    for (k, s) in scores.iter().enumerate() {
        let Some(s) = *s else { continue };
        let t = k as f64 * 2.0;
        seen += 1;
        worst = worst.max(s);
        if raea_io_trace::stat("RAEA_CHECK_DEBUG") {
            eprintln!("blotch {t:6.1}s {s:.3}");
        }
        if s > BLOTCHY {
            bad.push((t, s));
        }
    }
    if !bad.is_empty() {
        let at = bad[0].0;
        let share = bad.len() as f64 / seen.max(1) as f64;
        let found = format!("blocky patches in dark areas in {} of {seen} sampled frames (worst {:.0}% of dark blocks stepping)", bad.len(), worst * 100.0);
        let (p, at, says) = match blotching_cause(video, probe, scores, w, h) {
            // what the composition draws before any encoding is the picture's own (a phone's compression): no render
            // setting removes it, so it's said, not worked on
            Some(Ok(a)) if !a.0.by_encode(BLOTCH_NOISE) => (3, Some(at), format!("{found}: the picture's own, there as the composition draws them before any encoding (the footage's compression); the encode added none, so no render setting removes them")),
            Some(Ok(a)) => {
                let first = cause::Against::first(scores, &a.1, BLOTCHY, BLOTCH_NOISE).map(|i| i as f64 * 2.0);
                let own = if a.0.before > 0 { format!("; {} of the samples show the footage's own too, which stays", a.0.before) } else { String::new() };
                (if a.0.worse as f64 / a.0.of.max(1) as f64 > 0.2 { 1 } else { 2 }, first.or(Some(at)), format!("the encode added blocky patches in dark areas: {} of {} samples are blotchier than the composition draws them unencoded; render with --quality master, which gives the darks the bits they need{own}", a.0.worse, a.0.of))
            }
            Some(Err(why)) => (if share > 0.2 { 1 } else { 2 }, Some(at), format!("{found}; whether the encode made them or the picture holds them couldn't be told ({why}): render with --quality master if the encode starved the darks")),
            None => (if share > 0.2 { 1 } else { 2 }, Some(at), format!("{found}: the encode starved the near-blacks, or the footage holds them; render with --quality master, and check a render made in a raea project to tell which")),
        };
        issues.push(Issue { rule: "blotching", p, at, says });
    } else if seen > 0 {
        passed.push(format!("blotching: dark areas clean in {seen} sampled frames"));
    }
}

/// Caption contrast: at each text's middle frame, the pixels behind it against its colour, at least 3:1. The pixels
/// behind are the half of the box least like the text.
pub(crate) fn contrast(video: &str, runs: &[Run], w: f64, h: f64, probe: &Probe, issues: &mut Vec<Issue>, passed: &mut Vec<String>) {
    let (sx, sy) = (probe.w / w, probe.h / h);
    let mut low = 0;
    let mut measured = 0;
    for r in runs.iter().filter(|r| r.size >= 20.0).take(80) {
        let [x, y, bw, bh] = r.box_;
        let crop = [(x * sx).max(0.0) as u32, (y * sy).max(0.0) as u32, (bw * sx) as u32, (bh * sy) as u32];
        if crop[0] + crop[2] > probe.w as u32 || crop[1] + crop[3] > probe.h as u32 {
            continue;
        }
        let t = (r.from + r.to) / 2.0;
        // the same measure `raea world` joins to a text from its render (raea-sys-measure, DR2)
        let ratio = if crate::read::native() { crate::read::contrast(video, t, crop, &r.color) } else { raea_sys_measure::contrast(std::path::Path::new(video), t, crop, &r.color) };
        let Some(ratio) = ratio else { continue };
        measured += 1;
        if ratio < 3.0 {
            low += 1;
            issues.push(Issue { rule: "contrast", p: 1, at: Some(t), says: format!("{} is {ratio:.1}:1 against what's behind it; make it at least 3:1 (a band, a shadow for separation, or another colour)", quote(&r.text)) });
        }
    }
    if measured > 0 && low == 0 {
        passed.push(format!("contrast: {measured} texts at 3:1 or better against their pixels"));
    }
}
