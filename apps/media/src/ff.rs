//! `raea ffmpeg` and `raea ffprobe`: the command lines models write by reflex, read as the media verbs.

use super::*;

/// `raea ffmpeg ARGS`: an ffmpeg command line run as the media verbs.
pub fn ffmpeg_cli(args: &[String]) -> i32 {
    match ffmpeg(args) {
        Ok(()) => 0,
        Err(e) => {
            eprintln!("raea ffmpeg: {e}");
            1
        }
    }
}

pub(crate) fn secs(s: &str) -> R<f64> {
    // 12.5, 00:01:02.5, 1:02
    let mut t = 0.0;
    for part in s.split(':') {
        t = t * 60.0 + part.parse::<f64>().map_err(|_| format!("not a time: {s}"))?;
    }
    Ok(t)
}

pub(crate) const VERBS_HINT: &str = "raea media probe/still/sheet/audio/loudness/motion/cuts/proxy/trim/concat, or a composition rendered with raea render";

#[derive(Default)]
pub(crate) struct FfInput {
    pub(crate) path: String,
    pub(crate) ss: Option<f64>,
    pub(crate) t: Option<f64>,
    pub(crate) to: Option<f64>,
    pub(crate) concat: bool,
}

pub(crate) fn ffmpeg(args: &[String]) -> R<()> {
    let mut inputs: Vec<FfInput> = Vec::new();
    let mut pending = FfInput::default();
    let (mut ss, mut t, mut to) = (None, None, None);
    let (mut vf, mut af): (Option<String>, Option<String>) = (None, None);
    // -maxrate: the rate the copy is held to, the one quality choice read (the skill's share copy, #219)
    let mut cap = None::<u32>;
    let (mut ar, mut ac, mut fmt, mut one_frame, mut outp) = (None, None, None, false, None::<String>);
    let mut i = 0;
    let next = |i: &mut usize| -> R<String> {
        *i += 1;
        args.get(*i).cloned().ok_or_else(|| format!("{} needs a value", args[*i - 1]))
    };
    while i < args.len() {
        let a = args[i].as_str();
        match a {
            "-i" => {
                pending.path = next(&mut i)?;
                inputs.push(std::mem::take(&mut pending));
            }
            "-ss" => {
                let v = secs(&next(&mut i)?)?;
                if inputs.is_empty() { pending.ss = Some(v) } else { ss = Some(v) }
            }
            "-t" => {
                let v = secs(&next(&mut i)?)?;
                if inputs.is_empty() { pending.t = Some(v) } else { t = Some(v) }
            }
            "-to" => {
                let v = secs(&next(&mut i)?)?;
                if inputs.is_empty() { pending.to = Some(v) } else { to = Some(v) }
            }
            "-f" => {
                let f = next(&mut i)?;
                if f == "concat" && i + 1 < args.len() {
                    pending.concat = true;
                } else {
                    fmt = Some(f);
                }
            }
            "-vf" | "-filter:v" => vf = Some(next(&mut i)?),
            "-af" | "-filter:a" => af = Some(next(&mut i)?),
            "-ar" => ar = Some(next(&mut i)?.parse::<u32>().map_err(|_| "-ar takes a rate")?),
            "-ac" => ac = Some(next(&mut i)?.parse::<u32>().map_err(|_| "-ac takes a channel count")?),
            "-frames:v" | "-vframes" => one_frame = next(&mut i)? == "1",
            "-maxrate" => {
                let v = next(&mut i)?.to_lowercase();
                let (n, k) = match v.strip_suffix('k').map(|n| (n, 1.0)).or(v.strip_suffix('m').map(|n| (n, 1000.0))) {
                    Some(nk) => nk,
                    None => (v.as_str(), 0.001),
                };
                cap = Some((n.parse::<f64>().map_err(|_| "-maxrate takes a rate (2600k)")? * k).round().max(1.0) as u32);
            }
            "-filter_complex" | "-lavfi" => {
                return Err(format!("-filter_complex isn't read here: overlays, wipes and layouts are a composition, rendered with raea render. Or {VERBS_HINT}"));
            }
            // codec, quality and container choices: the platform encoder decides (H.264 + AAC in MP4)
            "-c" | "-c:v" | "-c:a" | "-codec" | "-vcodec" | "-acodec" | "-crf" | "-preset" | "-b:v" | "-b:a" | "-q:v" | "-qscale:v" | "-pix_fmt" | "-movflags" | "-v" | "-loglevel" | "-r" | "-safe" | "-map" | "-tune" | "-profile:v" | "-g" | "-threads" | "-bufsize" => {
                next(&mut i)?;
            }
            "-y" | "-n" | "-hide_banner" | "-nostats" | "-nostdin" | "-an" | "-vn" | "-sn" | "-shortest" | "-accurate_seek" => {}
            o if o.starts_with('-') && o != "-" => return Err(format!("{o} isn't read here; {VERBS_HINT}")),
            o => outp = Some(o.to_string()),
        }
        i += 1;
    }
    let out = outp.ok_or("no output")?;
    let Some(inp) = inputs.first() else { return Err("no input (-i FILE)".into()) };
    let from = inp.ss.or(ss);
    let span_to = |from: Option<f64>| inp.to.or(to).or_else(|| inp.t.or(t).map(|d| from.unwrap_or(0.0) + d));
    let (from, until) = (from, span_to(from));
    let path = PathBuf::from(&inp.path);
    let to_null = out == "-" || fmt.as_deref() == Some("null");
    // measures: printed the way ffmpeg's filters print them, so a caller reading them keeps working
    if to_null && fmt.as_deref() != Some("f32le") && fmt.as_deref() != Some("s16le") {
        let mut said = false;
        if let Some(f) = vf.as_deref() {
            if f.contains("scene") {
                let th = f.split("gt(scene,").nth(1).and_then(|r| r.split(')').next()).and_then(|x| x.parse().ok()).unwrap_or(0.4);
                let (_, hits) = cuts(&path, th, from, until)?;
                for (n, pts, s) in hits {
                    eprintln!("[Parsed_showinfo_1 @ 0x0] n:{n} pts:{} pts_time:{pts:.6} scene:{s:.6}", (pts * 1000.0).round() as i64);
                }
                said = true;
            } else if let Some(spec) = f.strip_prefix("freezedetect") {
                let opt = |k: &str, dflt: f64| spec.trim_start_matches('=').split(':').find_map(|kv| kv.strip_prefix(&format!("{k}="))).map(|v| v.trim_end_matches("dB")).and_then(|v| v.parse::<f64>().ok()).unwrap_or(dflt);
                let (n, dur) = (opt("n", opt("noise", 0.001)), opt("d", opt("duration", 2.0)));
                for (start, end) in freezes(&path, n, dur)? {
                    eprintln!("[freezedetect @ 0x0] lavfi.freezedetect.freeze_start: {}", ff_time(start));
                    if let Some((d, e)) = end {
                        eprintln!("[freezedetect @ 0x0] lavfi.freezedetect.freeze_duration: {}\n[freezedetect @ 0x0] lavfi.freezedetect.freeze_end: {}", ff_time(d), ff_time(e));
                    }
                }
                said = true;
            } else if f.contains("signalstats") {
                let fps = f.split("fps=").nth(1).and_then(|r| r.split(',').next()).and_then(|x| x.parse().ok()).unwrap_or(6.0);
                let (_, ys) = motion(&path, from, until, fps)?;
                for (k, (t, y)) in ys.iter().enumerate() {
                    eprintln!("frame:{k} pts:{} pts_time:{t:.6}\nlavfi.signalstats.YDIF={y:.6}", (t * 1000.0).round() as i64);
                }
                said = true;
            } else {
                return Err(format!("-vf {f} to null isn't read here; {VERBS_HINT}"));
            }
        }
        if let Some(f) = af.as_deref() {
            let a = slice(&soft()?.decode_audio(&path)?, from, until);
            if f.contains("volumedetect") {
                let (mean, max) = levels(&a);
                eprintln!("[Parsed_volumedetect_0 @ 0x0] n_samples: {}\n[Parsed_volumedetect_0 @ 0x0] mean_volume: {mean:.1} dB\n[Parsed_volumedetect_0 @ 0x0] max_volume: {max:.1} dB", a.len() * a.channels.len());
            } else if f.contains("ebur128") {
                eprintln!("[Parsed_ebur128_0 @ 0x0] Summary:\n\n  Integrated loudness:\n    I:         {:.1} LUFS\n\n  True peak:\n    Peak:      {:.1} dBFS", raea_io_media::audio::lufs(&a), raea_io_media::audio::true_peak_db(&a));
            } else {
                return Err(format!("-af {f} isn't read here; {VERBS_HINT}"));
            }
            said = true;
        }
        if !said {
            return Err(format!("an output to null measures something (-vf select=scene, signalstats; -af volumedetect, ebur128); {VERBS_HINT}"));
        }
        return Ok(());
    }
    // raw or WAV audio
    let oext = ext(Path::new(&out));
    if matches!(fmt.as_deref(), Some("f32le")) || oext == "wav" {
        if vf.is_some() || af.is_some() {
            return Err(format!("audio filters aren't read here; {VERBS_HINT}"));
        }
        let rate = ar.unwrap_or(soft()?.decode_audio(&path).map(|a| a.rate).unwrap_or(raea_io_media::audio::RATE));
        let a = audio_folded(&path, rate, ac == Some(1), from, until, fmt.as_deref() != Some("f32le"))?;
        if out == "-" {
            return write_f32le(&a, &mut std::io::stdout().lock());
        }
        if fmt.as_deref() == Some("f32le") {
            let mut f = std::fs::File::create(&out).map_err(|e| format!("{out}: {e}"))?;
            return write_f32le(&a, &mut f);
        }
        return write_wav(&a, Path::new(&out));
    }
    if fmt.is_some() && fmt.as_deref() != Some("mp4") && fmt.as_deref() != Some("image2") {
        return Err(format!("-f {} isn't read here; {VERBS_HINT}", fmt.unwrap_or_default()));
    }
    // a still
    if one_frame || IMAGE_EXTS.contains(&oext.as_str()) {
        if vf.is_some() {
            return Err(format!("-vf on a still isn't read here: take the still (raea media still), then crop or scale it in an edit; {VERBS_HINT}"));
        }
        still(&path, from.unwrap_or(0.0), Path::new(&out), None)?;
        return Ok(());
    }
    // video: concat, trim, scale
    let height = match vf.as_deref() {
        None => None,
        Some(f) if f.starts_with("scale=") && !chained(f) => Some(scale_height(&f[6..], &path)?),
        Some(f) => return Err(format!("-vf {f} isn't read here (only scale=W:H); crops, pads and overlays are an edit: {VERBS_HINT}")),
    };
    if af.is_some() {
        return Err(format!("-af on a video isn't read here; {VERBS_HINT}"));
    }
    let srcs: Vec<(PathBuf, Option<f64>, Option<f64>)> = if inp.concat {
        let list = std::fs::read_to_string(&inp.path).map_err(|e| format!("{}: {e}", inp.path))?;
        let base = Path::new(&inp.path).parent().map(Path::to_path_buf).unwrap_or_default();
        list.lines()
            .filter_map(|l| l.trim().strip_prefix("file "))
            .map(|f| (base.join(f.trim().trim_matches('\'').trim_matches('"')), None, None))
            .collect()
    } else if inputs.len() > 1 {
        return Err(format!("several inputs: use -f concat with a list, or an edit for anything laid over; {VERBS_HINT}"));
    } else {
        vec![(path.clone(), from, until)]
    };
    match (cap, srcs.as_slice()) {
        (Some(cap), [(one, None, None)]) => share(one, height, Path::new(&out), cap)?,
        (cap, _) => encode(&srcs, height, Path::new(&out), cap)?,
    };
    Ok(())
}

/// A time as ffmpeg's av_ts2timestr writes it: up to 6 decimals, trailing zeros dropped (44.233333, 55).
pub(crate) fn ff_time(t: f64) -> String {
    let s = format!("{t:.6}");
    s.trim_end_matches('0').trim_end_matches('.').to_string()
}

/// A filter chain (a comma outside parentheses and quotes): scale=-2:'min(720,ih)' is one filter.
pub(crate) fn chained(f: &str) -> bool {
    let (mut depth, mut quoted) = (0i32, false);
    for c in f.chars() {
        match c {
            '\'' => quoted = !quoted,
            '(' if !quoted => depth += 1,
            ')' if !quoted => depth -= 1,
            ',' if !quoted && depth == 0 => return true,
            _ => {}
        }
    }
    false
}

/// scale=W:H's height: -1/-2 keep the aspect, min(N,ih) caps it.
pub(crate) fn scale_height(spec: &str, path: &Path) -> R<u32> {
    let spec = spec.trim_matches(['\'', '"']);
    let (w, h) = spec.split_once(':').ok_or_else(|| format!("scale={spec}: write scale=W:H"))?;
    let (w, h) = (w.trim_matches(['\'', '"']), h.trim_matches(['\'', '"']));
    let p = probe_of(path)?;
    let i = p.video.ok_or("not a video")?;
    let n = |s: &str| -> Option<f64> {
        if let Some(r) = s.strip_prefix("min(") {
            let (a, b) = r.trim_end_matches(')').split_once(',')?;
            let v = |x: &str| if x == "ih" { Some(i.height as f64) } else if x == "iw" { Some(i.width as f64) } else { x.parse().ok() };
            return Some(v(a)?.min(v(b)?));
        }
        s.parse().ok()
    };
    match (n(w), n(h)) {
        (_, Some(h)) if h > 0.0 => Ok(h as u32),
        (Some(w), _) if w > 0.0 => Ok((i.height as f64 * w / i.width as f64).round() as u32),
        _ => Err(format!("scale={spec}: a width or a height in pixels")),
    }
}

/// `raea ffprobe ARGS`: an ffprobe command line answered from `media probe`, in the format it asks for.
pub fn ffprobe_cli(args: &[String]) -> i32 {
    match ffprobe(args) {
        Ok(s) => {
            print!("{s}");
            0
        }
        Err(e) => {
            eprintln!("raea ffprobe: {e}");
            1
        }
    }
}

pub(crate) fn ffprobe(args: &[String]) -> R<String> {
    let (mut entries, mut of, mut sel, mut file) = (None::<String>, String::from("default"), None::<String>, None::<String>);
    let (mut show_format, mut show_streams) = (false, false);
    let mut i = 0;
    while i < args.len() {
        let a = args[i].as_str();
        let mut val = || {
            i += 1;
            args.get(i).cloned().ok_or_else(|| format!("{a} needs a value"))
        };
        match a {
            "-show_entries" => entries = Some(val()?),
            "-of" | "-print_format" | "-output_format" => of = val()?,
            "-select_streams" => sel = Some(val()?),
            "-v" | "-loglevel" | "-i" => {
                let v = val()?;
                if a == "-i" {
                    file = Some(v);
                }
            }
            "-show_format" => show_format = true,
            "-show_streams" => show_streams = true,
            "-hide_banner" | "-count_frames" => {}
            o if o.starts_with('-') => return Err(format!("{o} isn't read here; raea media probe FILE gives everything as JSON")),
            f => file = Some(f.to_string()),
        }
        i += 1;
    }
    let path = PathBuf::from(file.ok_or("which file?")?);
    let p = probe_of(&path)?;
    // the streams: video, then audio (an image is one video stream)
    let mut streams: Vec<(char, Vec<(&str, Value)>)> = Vec::new();
    if let Some(v) = &p.video {
        // ffprobe reports the stored size (a phone's portrait take is stored landscape and rotated on show)
        let turned = raea_io_media::soft::rotation_of(&path).rem_euclid(180) == 90;
        let (w, h) = if turned { (v.height, v.width) } else { (v.width, v.height) };
        streams.push(('v', vec![
            ("codec_name", json!(v.codec)), ("codec_type", json!("video")), ("width", json!(w)), ("height", json!(h)),
            ("r_frame_rate", json!(rate_fraction(p.track.as_ref().map(|t| t.grid_fps).unwrap_or(v.fps)))), ("avg_frame_rate", json!(rate_fraction(v.fps))),
            ("duration", json!(format!("{:.6}", v.duration))), ("nb_frames", json!(v.frames.to_string())),
        ]));
    }
    if let Some((w, h)) = p.image {
        streams.push(('v', vec![("codec_type", json!("video")), ("width", json!(w)), ("height", json!(h))]));
    }
    if let Some((rate, ch, d)) = p.audio {
        streams.push(('a', vec![("codec_type", json!("audio")), ("sample_rate", json!(rate.to_string())), ("channels", json!(ch)), ("duration", json!(format!("{d:.6}")))]));
    }
    if let Some(s) = &sel {
        let (kind, idx) = s.split_once(':').map(|(k, n)| (k, n.parse::<usize>().ok())).unwrap_or((s.as_str(), None));
        let kind = kind.chars().next().unwrap_or('v');
        let picked: Vec<_> = streams.into_iter().filter(|(k, _)| *k == kind).collect();
        streams = match idx {
            Some(n) => picked.into_iter().nth(n).into_iter().collect(),
            None => picked,
        };
    }
    let format: Vec<(&str, Value)> = vec![
        ("filename", json!(path)), ("nb_streams", json!(streams.len())), ("duration", json!(format!("{:.6}", p.duration))), ("size", json!(p.bytes.to_string())),
    ];
    // which fields of which section
    let mut want_s: Option<Vec<String>> = None;
    let mut want_f: Option<Vec<String>> = None;
    if let Some(e) = &entries {
        for sec in e.split(':') {
            let (name, fields) = sec.split_once('=').unwrap_or((sec, ""));
            let fields: Vec<String> = fields.split(',').filter(|f| !f.is_empty()).map(str::to_string).collect();
            match name {
                "stream" => want_s = Some(fields),
                "format" => want_f = Some(fields),
                o => return Err(format!("-show_entries {o} isn't read here (stream=..., format=...)")),
            }
        }
    } else {
        if show_streams {
            want_s = Some(Vec::new());
        }
        if show_format {
            want_f = Some(Vec::new());
        }
    }
    let pick = |all: &[(&str, Value)], want: &[String]| -> Vec<(String, Value)> {
        if want.is_empty() {
            return all.iter().map(|(k, v)| (k.to_string(), v.clone())).collect();
        }
        want.iter().filter_map(|w| all.iter().find(|(k, _)| k == w).map(|(k, v)| (k.to_string(), v.clone()))).collect()
    };
    let mut sections: Vec<(&str, Vec<(String, Value)>)> = Vec::new();
    if let Some(w) = &want_s {
        for (_, s) in &streams {
            sections.push(("STREAM", pick(s, w)));
        }
    }
    if let Some(w) = &want_f {
        sections.push(("FORMAT", pick(&format, w)));
    }
    let plain = |v: &Value| match v {
        Value::String(s) => s.clone(),
        o => o.to_string(),
    };
    let (kind, opts) = of.split_once('=').unwrap_or((of.as_str(), ""));
    let mut s = String::new();
    match kind {
        "csv" | "compact" => {
            let no_prefix = opts.split(':').any(|o| o == "p=0" || o == "print_section=0");
            for (name, fields) in &sections {
                let vals: Vec<String> = fields.iter().map(|(k, v)| if kind == "compact" { format!("{k}={}", plain(v)) } else { plain(v) }).collect();
                let sep = if kind == "compact" { "|" } else { "," };
                if no_prefix {
                    s.push_str(&vals.join(sep));
                } else {
                    s.push_str(&format!("{}{sep}{}", name.to_lowercase(), vals.join(sep)));
                }
                s.push('\n');
            }
        }
        "json" => {
            let obj = |f: &Vec<(String, Value)>| Value::Object(f.iter().cloned().collect());
            let mut root = serde_json::Map::new();
            if want_s.is_some() {
                root.insert("streams".into(), Value::Array(sections.iter().filter(|(n, _)| *n == "STREAM").map(|(_, f)| obj(f)).collect()));
            }
            if let Some((_, f)) = sections.iter().find(|(n, _)| *n == "FORMAT") {
                root.insert("format".into(), obj(f));
            }
            s = serde_json::to_string_pretty(&Value::Object(root)).unwrap_or_default();
            s.push('\n');
        }
        "default" | "flat" | "ini" => {
            let nw = opts.split(':').any(|o| o == "nw=1" || o == "noprint_wrappers=1");
            let nk = opts.split(':').any(|o| o == "nk=1" || o == "nokey=1");
            for (name, fields) in &sections {
                if !nw {
                    s.push_str(&format!("[{name}]\n"));
                }
                for (k, v) in fields {
                    if nk {
                        s.push_str(&format!("{}\n", plain(v)));
                    } else {
                        s.push_str(&format!("{k}={}\n", plain(v)));
                    }
                }
                if !nw {
                    s.push_str(&format!("[/{name}]\n"));
                }
            }
        }
        o => return Err(format!("-of {o} isn't read here (csv, compact, json, default)")),
    }
    Ok(s)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn times_and_rates_read_as_ffmpeg_writes_them() {
        assert_eq!(secs("12.5").unwrap(), 12.5);
        assert_eq!(secs("00:01:02.5").unwrap(), 62.5);
        assert_eq!(rate_fraction(30.0), "30/1");
        assert_eq!(rate_fraction(29.97), "30000/1001");
        // a measured rate snaps to the nearest standard one within 1%
        assert_eq!(rate_fraction(24.03), "24/1");
        assert_eq!(rate_fraction(23.95), "24000/1001");
        assert_eq!(rate_fraction(12.5), "12500/1000");
    }

    #[test]
    fn one_scale_filter_is_not_a_chain() {
        assert!(!chained("scale=-2:'min(720,ih)'"));
        assert!(!chained("scale=-2:min(720\\,ih)"));
        assert!(chained("scale=-2:720,fps=30"));
    }

    #[test]
    fn ffmpeg_samples_the_middle_of_each_tick() {
        // fps=6 over 30 fps footage takes source frames 2, 7, 12 (the last before each tick's middle)
        let info = Info { codec: "h264".into(), width: 2, height: 2, fps: 30.0, duration: 1.0, frames: 30 };
        let t = times(info.duration, None, None, 6.0);
        assert_eq!(t.len(), 6);
        let frame = |s: f64| (s * 30.0).floor() as i64;
        assert_eq!(t.iter().map(|(_, at)| frame(*at)).collect::<Vec<_>>(), vec![2, 7, 12, 17, 22, 27]);
        assert_eq!(t[1].0, 1.0 / 6.0);
    }

    #[test]
    fn wav_is_16_bit_pcm() {
        let dir = std::env::temp_dir().join(format!("render-media-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join("a.wav");
        write_wav(&Audio { rate: 16000, channels: vec![vec![0.0, 0.5, -1.0]] }, &p).unwrap();
        let b = std::fs::read(&p).unwrap();
        assert_eq!(&b[..4], b"RIFF");
        assert_eq!(b.len(), 44 + 6);
        assert_eq!(u32::from_le_bytes(b[24..28].try_into().unwrap()), 16000);
        assert_eq!(i16::from_le_bytes(b[46..48].try_into().unwrap()), 16384);
        assert_eq!(i16::from_le_bytes(b[48..50].try_into().unwrap()), -32767);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn what_isnt_read_is_refused_with_the_way_to_do_it() {
        let a = |s: &str| s.split(' ').map(str::to_string).collect::<Vec<_>>();
        let e = ffmpeg(&a("-i x.mp4 -filter_complex overlay out.mp4")).unwrap_err();
        assert!(e.contains("a composition"), "{e}");
        let e = ffmpeg(&a("-i x.mp4 -vf hue=s=0 out.mp4")).unwrap_err();
        assert!(e.contains("isn't read here"), "{e}");
    }
}
