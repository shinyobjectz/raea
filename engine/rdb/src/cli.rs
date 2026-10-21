//! rdb's command line (src/main.rs is `rdb::cli::cli`): the edit as live SQL tables, edited by snippets, run on Bevy's World (docs/relational-plan.feature).
//!
//!   rdb new EDIT.rdb                         an empty edit
//!   rdb apply EDIT.rdb "SQL" [--preview F -o FRAME.png]   one snippet: checked, applied, recorded, saved
//!   rdb undo EDIT.rdb [N]                    undo the last N moves (default 1)
//!   rdb moves EDIT.rdb                       the move history
//!   rdb query EDIT.rdb "SELECT ..."          read the edit (one SELECT on its columns)
//!   rdb energy EDIT   its energy curve: payoffs, breaths, flatlines, picture-music relation (docs/energy.feature)
//!   rdb captions EDIT.rdb --words takes/<take>/words.json [--per N] [--style k=v,...] [--highlight #22d3ee]   word captions through the source map
//!   rdb serve   a resident renderer: JSON requests (look, stills) on stdin, answers on stdout; engines stay warm
//!   rdb compare EDIT EDIT...   which variations are near-copies, and in which facets (hook, sources, rhythm, style, look)
//!   rdb stills EDIT [--at F,F] [--formats 1x1,4x5,9x16,1.91x1] -o DIR   ad stills of a moment, reflowed per format
//!   rdb render EDIT.rdb|EDIT.sql -o OUT.mp4|DIR|HASHES.txt [--at F,F] [--codec apple|soft|av1|ffmpeg] [--kbps K]
//!   rdb media probe|still|sheet|audio|loudness|motion|cuts|freezes|proxy|trim|concat FILE ...   media work, native (rdb media help)
//!   rdb ffmpeg ARGS | rdb ffprobe ARGS        ffmpeg and ffprobe command lines read as the media verbs (no ffmpeg involved)
//!   rdb g1 DIR                                  G1's pass test on DIR/hook.sql and DIR/snippets.sql

use crate::{db, dir_of, engine, frame_digest, gpuview, inserted_of, ir, open, previz, save_png, sound};

use std::path::{Path, PathBuf};

/// A refusal: said on stderr, and the command ends with exit 1 (`cli` catches it; the process goes on).
struct Fail;
/// A command's own exit code (the media verbs answer with one).
struct Exit(i32);

fn fail(e: impl std::fmt::Display) -> ! {
    eprintln!("rdb: {e}");
    std::panic::panic_any(Fail)
}

/// rdb's command line as a function: the exit code, in this process (raea's shell runs it on the iPhone, where nothing
/// can be spawned). `serve` reads stdin until it closes, so it's refused here; run the binary for it.
pub fn cli(args: &[String]) -> i32 {
    if args.first().map(String::as_str) == Some("serve") {
        eprintln!("rdb: serve is a resident process; run the rdb binary for it");
        return 2;
    }
    static HOOK: std::sync::Once = std::sync::Once::new();
    HOOK.call_once(|| {
        let prev = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |info| {
            if info.payload().downcast_ref::<Fail>().is_none() && info.payload().downcast_ref::<Exit>().is_none() {
                prev(info);
            }
        }));
    });
    let a = args.to_vec();
    match std::panic::catch_unwind(move || run(a)) {
        Ok(()) => 0,
        Err(p) if p.downcast_ref::<Fail>().is_some() => 1,
        Err(p) => match p.downcast_ref::<Exit>() {
            Some(Exit(c)) => *c,
            None => std::panic::resume_unwind(p),
        },
    }
}

fn run(args: Vec<String>) {
    let flag = |n: &str| args.iter().position(|a| a == n).and_then(|i| args.get(i + 1)).cloned();
    let flag_on = |n: &str| args.iter().any(|a| a == n);
    let arg = |i: usize| args.get(i).cloned().unwrap_or_else(|| fail("missing argument (see src/main.rs)"));
    match args.first().map(String::as_str).unwrap_or("") {
        "new" => {
            let p = arg(1);
            db::Db::default().save(Path::new(&p)).unwrap_or_else(|e| fail(e));
            println!("{p}: an empty edit");
        }
        "apply" => {
            let (p, sql) = (arg(1), arg(2));
            let mut d = db::Db::load(Path::new(&p)).unwrap_or_else(|e| fail(e));
            match d.apply(&sql) {
                Ok(ns) => {
                    // a snippet that can't be saved changed nothing on disk
                    d.save(Path::new(&p)).unwrap_or_else(|e| fail(format!("refused, nothing changed: the edit couldn't be saved: {e}")));
                    for n in &ns {
                        let m = &d.moves[n - 1];
                        // a move over many rows (captions with their word runs) says how many, not every row
                        let brief = |rows: &str| -> String {
                            match serde_json::from_str::<Vec<serde_json::Value>>(rows) {
                                Ok(v) if rows.len() > 600 => {
                                    let ids: Vec<&str> = v.iter().filter_map(|r| r["id"].as_str()).take(4).collect();
                                    format!("{} rows ({} ...)", v.len(), ids.join(", "))
                                }
                                _ => rows.to_string(),
                            }
                        };
                        let text = if m.text.len() > 400 { format!("{} ...", &m.text[..m.text.char_indices().nth(400).map(|c| c.0).unwrap_or(m.text.len())]) } else { m.text.clone() };
                        println!("move {n}: {text}\n  before: {}\n  after:  {}", brief(&m.before), brief(&m.after));
                    }
                    if let Some(w) = d.state.program().ok().and_then(|(_, rows)| crate::fonts::warning(&rows)) {
                        println!("{w}");
                    }
                    if let (Some(f), Some(o)) = (flag("--preview"), flag("-o")) {
                        let (prog, rows) = d.state.program().unwrap_or_else(|e| fail(e));
                        let mut e = engine::Engine::new(&prog, &rows, &dir_of(&p)).unwrap_or_else(|e| fail(e));
                        let px = e.frame(f.parse().unwrap_or(0)).unwrap_or_else(|e| fail(e));
                        let (w, h) = e.size();
                        save_png(Path::new(&o), &e.to_rgba(&px), w, h).unwrap_or_else(|e| fail(e));
                        println!("preview: {o}");
                    }
                }
                Err(e) => fail(format!("refused, nothing changed: {e}")),
            }
        }
        "undo" => {
            let p = arg(1);
            let n: usize = args.get(2).and_then(|x| x.parse().ok()).unwrap_or(1);
            let mut d = db::Db::load(Path::new(&p)).unwrap_or_else(|e| fail(e));
            // undo N past the first move takes back every move and says so (it failed there before, after printing
            // the undos it then didn't save)
            if d.moves.is_empty() {
                fail("nothing to undo");
            }
            for _ in 0..n {
                if d.moves.is_empty() {
                    println!("that was every move: the edit is empty again");
                    break;
                }
                let m = d.undo().unwrap_or_else(|e| fail(e));
                println!("undid move {}: {}", m.n, m.text);
            }
            d.save(Path::new(&p)).unwrap_or_else(|e| fail(e));
        }
        "moves" => {
            let d = db::Db::load(Path::new(&arg(1))).unwrap_or_else(|e| fail(e));
            for m in &d.moves {
                println!("{:>3}  {}", m.n, m.text);
            }
        }
        "query" => {
            println!("{}", db::Db::query(Path::new(&arg(1)), &arg(2)).unwrap_or_else(|e| fail(e)));
        }
        "serve" => crate::serve::serve(),
        "captions" => {
            // word captions from a take's words.json through the source map, applied as one move (captions.rs)
            let p = arg(1);
            let words = PathBuf::from(flag("--words").unwrap_or_else(|| fail("captions needs --words takes/<take>/words.json")));
            let mut d = db::Db::load(Path::new(&p)).unwrap_or_else(|e| fail(e));
            let (prog, rows) = d.state.program().unwrap_or_else(|e| fail(e));
            let (sql, n) = crate::captions::caption_sql(&prog, &rows, &dir_of(&p), &words, &flag("--style").unwrap_or_default(), flag("--per").and_then(|v| v.parse().ok()).unwrap_or(1).max(1), flag("--highlight").as_deref()).unwrap_or_else(|e| fail(e));
            match d.apply(&sql) {
                Ok(ns) => {
                    d.save(Path::new(&p)).unwrap_or_else(|e| fail(e));
                    let before = rows.iter().filter(|t| t.kind == "caption").count();
                    let note = if before > 0 { format!("; the edit already had {before} captions, which still show (DELETE FROM caption WHERE id NOT LIKE 'cap%' drops them)") } else { String::new() };
                    println!("{n} captions as move {:?} (undo takes them back){note}", ns);
                }
                Err(e) => fail(format!("refused, nothing changed: {e}")),
            }
        }
        "energy" => {
            // the edit's energy curve, payoffs, breaths, flatlines and picture-music relation (energy.rs)
            let edit = arg(1);
            let (prog, rows) = crate::open(&edit).unwrap_or_else(|e| fail(e));
            let t0 = std::time::Instant::now();
            let mut v = crate::energy::energy(&prog, &rows, &dir_of(&edit)).unwrap_or_else(|e| fail(e));
            v["ms"] = serde_json::json!((t0.elapsed().as_secs_f64() * 1000.0).round());
            println!("{}", serde_json::to_string_pretty(&v).unwrap());
        }
        "compare" => {
            // edits' fingerprints side by side: which pairs are near-copies, and in which facets (fingerprint.rs)
            let names: Vec<String> = args[1..].iter().filter(|a| !a.starts_with("--")).cloned().collect();
            if names.len() < 2 {
                fail("compare needs two or more edits");
            }
            let fps: Vec<crate::fingerprint::Fingerprint> = names.iter().map(|n| {
                let (p, r) = crate::open(n).unwrap_or_else(|e| fail(format!("{n}: {e}")));
                crate::fingerprint::fingerprint(&p, &r).unwrap_or_else(|e| fail(format!("{n}: {e}")))
            }).collect();
            println!("{}", serde_json::to_string_pretty(&crate::fingerprint::compare(&names, &fps)).unwrap());
        }
        "stills" => {
            // ad stills: one moment at the paid/organic formats (stills.rs)
            let edit = arg(1);
            let out = PathBuf::from(flag("-o").unwrap_or_else(|| fail("stills needs -o DIR")));
            let (prog, rows) = crate::open(&edit).unwrap_or_else(|e| fail(e));
            // --best N: the frames the take's ingest says hold up as stills (stills::best_frames)
            let best = flag("--best").map(|n| crate::stills::best_frames(&prog, &rows, &dir_of(&edit), n.parse().unwrap_or(3)).unwrap_or_else(|e| fail(e)));
            if let Some(b) = &best {
                for (f, sc, why) in b {
                    println!("frame {f}: {sc:.2} ({why})");
                }
            }
            let frames: Vec<i64> = match (&best, flag("--at")) {
                (Some(b), _) if !b.is_empty() => b.iter().map(|x| x.0).collect(),
                (_, Some(at)) => at.split(',').map(|f| f.trim().parse().unwrap_or_else(|_| fail(format!("bad frame {f}")))).collect(),
                _ => vec![prog.frames / 3],
            };
            let formats: Vec<crate::stills::Format> = match flag("--formats") {
                Some(list) => list.split(',').map(|n| crate::stills::format(n.trim()).unwrap_or_else(|| fail(format!("unknown format {n} (1x1, 4x5, 9x16, 1.91x1)")))).collect(),
                None => crate::stills::FORMATS.to_vec(),
            };
            let stem = Path::new(&edit).file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or("edit".into());
            let t0 = std::time::Instant::now();
            let made = crate::stills::stills(&prog, &rows, &dir_of(&edit), &stem, &frames, &formats, &out).unwrap_or_else(|e| fail(e));
            for p in &made {
                println!("{}", p.display());
            }
            println!("{} stills in {:.2}s", made.len(), t0.elapsed().as_secs_f64());
        }
        "render" => {
            let out = PathBuf::from(flag("-o").unwrap_or_else(|| fail("render needs -o OUT.mp4|DIR")));
            let frames = flag("--at").map(|at| at.split(',').map(|f| f.trim().parse().unwrap_or_else(|_| fail(format!("bad frame {f}")))).collect());
            let opts = crate::RenderOpts { frames, codec: flag("--codec"), kbps: flag("--kbps").and_then(|k| k.parse().ok()), ssim: flag_on("--ssim"), sound: None, ten_bit: flag_on("--10bit") };
            crate::render_file(&arg(1), &out, &opts).unwrap_or_else(|e| fail(e));
        }
        "lower" | "check" => fail("briefs aren't checked here: a TypeScript project is checked with `raea check`"),
        "g1" => g1(&arg(1)).unwrap_or_else(|e| fail(e)),
        "export" => export_cmd(&arg(1), &PathBuf::from(flag("-o").unwrap_or_else(|| fail("export needs -o OUT.mp4"))), flag("--cache"),
            flag("--gop").and_then(|g| g.parse().ok()).unwrap_or(30), flag("--qp").and_then(|g| g.parse().ok()).unwrap_or(24))
            .unwrap_or_else(|e| fail(e)),
        // the agent's media work on raea-media: `rdb media probe|still|sheet|audio|...`, and ffmpeg/ffprobe command
        // lines read as those verbs (media_verbs.rs); no ffmpeg binary is involved
        "media" if crate::media_verbs::VERBS.contains(&arg(1).as_str()) => std::panic::panic_any(Exit(crate::media_verbs::cli(&args[1..]))),
        "ffmpeg" => std::panic::panic_any(Exit(crate::media_verbs::ffmpeg_cli(&args[1..]))),
        "ffprobe" => std::panic::panic_any(Exit(crate::media_verbs::ffprobe_cli(&args[1..]))),
        "media" => {
            // the media table's rows for FILEs: content hash, path (as given), and the facts the edit reads
            use sha2::{Digest, Sha256};
            let mezz = flag_on("--mezz");
            for f in args[1..].iter().filter(|a| !a.starts_with("--")) {
                let bytes = std::fs::read(f).unwrap_or_else(|e| fail(format!("{f}: {e}")));
                let hash = format!("{:x}", Sha256::digest(&bytes));
                let q = |s: &str| format!("'{}'", s.replace('\'', "''"));
                let backend = raea_media::backend(&engine::media_backend()).unwrap_or_else(|e| fail(e));
                let (kind, codec, w, h, fps, dur) = match backend.open_video(Path::new(f)) {
                    Ok(d) => {
                        let i = d.info().clone();
                        ("video", i.codec, i.width, i.height, i.fps, i.duration)
                    }
                    Err(_) => {
                        let a = raea_media::backend("soft").and_then(|b| b.decode_audio(Path::new(f))).unwrap_or_else(|e| fail(format!("{f}: {e}")));
                        let (codec, _, _) = raea_media::soft::audio_facts(Path::new(f)).unwrap_or_else(|e| fail(format!("{f}: {e}")));
                        ("audio", codec, 0, 0, 0.0, a.duration())
                    }
                };
                // --mezz: video the portable decoders can't take (B-frames, HEVC, ...) gets an edit mezzanine beside it
                let mut extra = String::new();
                if mezz && kind == "video" {
                    let portable = raea_media::backend("soft").and_then(|b| b.open_video(Path::new(f)).map(|_| ()));
                    if portable.is_err() {
                        let m = format!("{}.mezz.mp4", f.trim_end_matches(|c| c != '.').trim_end_matches('.'));
                        if !Path::new(&m).exists() {
                            #[cfg(feature = "h264")]
                            raea_media::soft::mezzanine(Path::new(f), Path::new(&m), &*backend, 30, 16).unwrap_or_else(|e| fail(format!("{f}: mezzanine: {e}")));
                            #[cfg(not(feature = "h264"))]
                            fail(format!("{f}: a mezzanine needs H.264 (the h264 feature)"));
                        }
                        let mb = std::fs::metadata(&m).map(|x| x.len()).unwrap_or(0);
                        extra = format!(", mezz = {}, mezz_bytes = {mb}", q(&m));
                    }
                }
                let (cols, vals) = if extra.is_empty() {
                    ("".to_string(), "".to_string())
                } else {
                    let parts: Vec<&str> = extra.trim_start_matches(", ").split(", ").collect();
                    (
                        parts.iter().map(|p| format!(", {}", p.split(" = ").next().unwrap())).collect::<String>(),
                        parts.iter().map(|p| format!(", {}", p.split(" = ").nth(1).unwrap())).collect::<String>(),
                    )
                };
                println!(
                    "INSERT INTO media (id, path, kind, codec, width, height, fps, duration, bytes{cols}) VALUES ({}, {}, {}, {}, {w}, {h}, {fps:.4}, {dur:.4}, {}{vals});",
                    q(&hash), q(f), q(kind), q(&codec), bytes.len()
                );
            }
        }
        "gpuview" => {
            // scrub: frames in the given order, first visits then revisits; timings per frame, and a PNG of the last
            let (prog, rows) = open(&arg(1)).unwrap_or_else(|e| fail(e));
            let mut v = gpuview::GpuView::new(&prog, &rows, &dir_of(&arg(1))).unwrap_or_else(|e| fail(e));
            if let Some(demo) = flag("--demo") {
                // a scrub: 150 frames in a seeded random order, each composited from the pool and read back, then encoded
                let mut s = 0x5eed_u64;
                let order: Vec<i64> = (0..150).map(|_| {
                    s = s.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
                    ((s >> 33) % prog.frames as u64) as i64
                }).collect();
                let opts = raea_media::EncodeOpts { width: v.w, height: v.h, fps: 10.0, kbps: 6000, nv12: false, realtime: false };
                let mut enc = raea_media::backend(&engine::media_backend()).and_then(|b| b.encoder(Path::new(&demo), &opts)).unwrap_or_else(|e| fail(e));
                let (mut cold, mut warm) = (Vec::new(), Vec::new());
                for pass in 0..2 {
                    for f in &order {
                        let t = v.frame(*f).unwrap_or_else(|e| fail(e));
                        if pass == 0 { cold.push(t.total) } else { warm.push(t.total); enc.push(&v.read()).unwrap_or_else(|e| fail(e)); }
                    }
                }
                enc.finish(None).unwrap_or_else(|e| fail(e));
                let stat = |x: &[f64]| { let mut y = x.to_vec(); y.sort_by(f64::total_cmp); serde_json::json!({"mean": y.iter().sum::<f64>() / y.len() as f64, "p50": y[y.len() / 2], "p95": y[y.len() * 95 / 100], "max": y[y.len() - 1]}) };
                println!("{}", serde_json::json!({"demo": demo, "frames": order.len(), "first_visit_ms": stat(&cold), "revisit_ms": stat(&warm), "pool_mb": v.pool_bytes as f64 / 1e6}));
                return;
            }
            if flag_on("--sketch") {
                // the whole edit at proxy size: cold (ingest included), then warm, then a contact sheet
                let all: Vec<i64> = (0..prog.frames).collect();
                let (_, cold) = v.sketch(&all).unwrap_or_else(|e| fail(e));
                let (buf, warm) = v.sketch(&all).unwrap_or_else(|e| fail(e));
                let t = std::time::Instant::now();
                let px = v.gpu.read(&buf, all.len() * (v.pw * v.ph * 4) as usize);
                let read_ms = t.elapsed().as_secs_f64() * 1e3;
                if let Some(o) = flag("-o") {
                    let (pw, ph) = (v.pw as usize, v.ph as usize);
                    let picks: Vec<usize> = (0..12).map(|k| k * all.len() / 12).collect();
                    let (cols, gap) = (6usize, 4usize);
                    let (sw, sh) = (cols * pw + (cols - 1) * gap, 2 * ph + gap);
                    let mut sheet = vec![255u8; sw * sh * 4];
                    for (n, &f) in picks.iter().enumerate() {
                        let (ox, oy) = ((n % cols) * (pw + gap), (n / cols) * (ph + gap));
                        for y in 0..ph {
                            let src = &px[(f * pw * ph + y * pw) * 4..(f * pw * ph + y * pw + pw) * 4];
                            let d = ((oy + y) * sw + ox) * 4;
                            sheet[d..d + pw * 4].copy_from_slice(src);
                        }
                    }
                    save_png(Path::new(&o), &sheet, sw as u32, sh as u32).unwrap_or_else(|e| fail(e));
                }
                let mut out = serde_json::json!({"proxy": [v.pw, v.ph], "cold": cold.json(), "warm": warm.json(), "readback_all_ms": read_ms});
                // HDC on the GPU: look vectors from the sketch, all-pairs distances, a storyboard
                let (_hv_cold, looks_ms) = v.looks(&buf, all.len()).unwrap_or_else(|e| fail(e));
                let (hv_all, looks_ms) = { let _ = looks_ms; v.looks(&buf, all.len()).unwrap_or_else(|e| fail(e)) }; // warm (kernels compiled)
                let (picks, worst, even_worst, ham_ms, sb_ms) = v.storyboard(&hv_all, all.len(), 12).unwrap_or_else(|e| fail(e));
                out["hdc"] = serde_json::json!({"looks_ms": looks_ms, "all_pairs_gpu_ms": ham_ms, "storyboard_ms": sb_ms,
                    "storyboard": picks, "worst_gap_bits": worst, "even_spacing_worst_gap_bits": even_worst});
                // --snippet SQL: apply it, find the frames it changed, re-sketch only those
                if let Some(sql) = flag("--snippet") {
                    let src = std::fs::read_to_string(arg(1)).unwrap_or_else(|e| fail(e));
                    let t = std::time::Instant::now();
                    let p2 = ir::compile(&format!("{src}\n{sql}")).unwrap_or_else(|e| fail(e));
                    let r2 = inserted_of(&p2);
                    let compile_ms = t.elapsed().as_secs_f64() * 1e3;
                    let t = std::time::Instant::now();
                    let changed = v.replace(&p2, &r2).unwrap_or_else(|e| fail(e));
                    let diff_ms = t.elapsed().as_secs_f64() * 1e3;
                    let t = std::time::Instant::now();
                    let (sk2, st) = v.sketch(&changed).unwrap_or_else(|e| fail(e));
                    let sketch_ms = t.elapsed().as_secs_f64() * 1e3;
                    // how much each changed frame's look moved (old looks vs new, on the GPU)
                    let t = std::time::Instant::now();
                    let (hv_new, _) = v.looks(&sk2, changed.len()).unwrap_or_else(|e| fail(e));
                    let (deltas, _) = v.look_deltas(&hv_all, &hv_new, &changed).unwrap_or_else(|e| fail(e));
                    let delta_ms = t.elapsed().as_secs_f64() * 1e3;
                    let most = deltas.iter().enumerate().max_by_key(|(_, d)| **d).map(|(i, d)| (changed[i], *d));
                    out["snippet"] = serde_json::json!({"sql": sql, "frames_changed": changed.len(), "first": changed.first(), "last": changed.last(),
                        "compile_ms": compile_ms, "find_changed_ms": diff_ms, "resketch_ms": sketch_ms, "total_ms": compile_ms + diff_ms + sketch_ms, "resketch": st.json(),
                        "look_delta_ms": delta_ms, "look_delta_mean_bits": deltas.iter().sum::<u32>() as f64 / deltas.len().max(1) as f64, "most_changed": most});
                }
                println!("{out}");
                return;
            }
            let list: Vec<i64> = flag("--at").map(|a| a.split(',').filter_map(|x| x.trim().parse().ok()).collect()).unwrap_or_else(|| (0..prog.frames).step_by(37).collect());
            let mut first = Vec::new();
            for f in &list {
                first.push(v.frame(*f).unwrap_or_else(|e| fail(e)));
            }
            let mut again = Vec::new();
            let mut with_read = Vec::new();
            for f in list.iter().rev() {
                again.push(v.frame(*f).unwrap_or_else(|e| fail(e)));
                let t = std::time::Instant::now();
                let _ = v.read();
                with_read.push(again.last().unwrap().total + t.elapsed().as_secs_f64() * 1e3);
            }
            let mean = |xs: &[f64]| xs.iter().sum::<f64>() / xs.len().max(1) as f64;
            let max = |xs: &[f64]| xs.iter().cloned().fold(0.0, f64::max);
            let tot = |v: &[gpuview::FrameTimes]| v.iter().map(|t| t.total).collect::<Vec<_>>();
            if let Some(o) = flag("-o") {
                v.frame(list[0]).unwrap_or_else(|e| fail(e));
                save_png(Path::new(&o), &v.read(), v.w, v.h).unwrap_or_else(|e| fail(e));
            }
            println!(
                "{}",
                serde_json::json!({"frames": list.len(), "adapter": v.gpu.adapter,
                    "first_visit_ms": {"mean": mean(&tot(&first)), "max": max(&tot(&first))},
                    "revisit_ms": {"mean": mean(&tot(&again)), "max": max(&tot(&again)),
                        "composite_gpu_mean": mean(&again.iter().map(|t| t.composite_gpu).collect::<Vec<_>>()),
                        "rules_mean": mean(&again.iter().map(|t| t.rules).collect::<Vec<_>>())},
                    "revisit_with_readback_ms": {"mean": mean(&with_read), "max": max(&with_read)},
                    "pool_mb": v.pool_bytes as f64 / 1e6, "sample_first": first.first().map(|t| t.json()), "sample_revisit": again.first().map(|t| t.json())})
            );
        }
        "previz" => previz_cmd(&arg(1), &PathBuf::from(flag("-o").unwrap_or_else(|| fail("previz needs -o DIR"))), flag("--vs"), flag("--snippet"))
            .unwrap_or_else(|e| fail(e)),
        _ => fail("usage: rdb new|apply|undo|moves|query|render|check|lower|g1 ... (see src/main.rs)"),
    }
}


/// Incremental export: the video is a run of GOPs that depend on nothing before them (fixed quantiser, an IDR each),
/// stored by the digest of their frames' digests. An export renders and encodes only the GOPs it can't find, copies
/// the rest, mixes the sound fresh, and muxes. A snippet that touches a few seconds costs a few seconds.
#[cfg(not(feature = "h264"))]
fn export_cmd(_edit: &str, _out: &Path, _cache: Option<String>, _gop: usize, _qp: u8) -> Result<(), String> {
    Err("rdb export makes H.264 GOPs: this build has no h264 feature".into())
}

#[cfg(feature = "h264")]
fn export_cmd(edit: &str, out: &Path, cache: Option<String>, gop: usize, qp: u8) -> Result<(), String> {
    use std::hash::{Hash, Hasher};
    let t0 = std::time::Instant::now();
    let (prog, rows) = open(edit)?;
    let dir = dir_of(edit);
    let cache = cache.map(PathBuf::from).unwrap_or_else(|| dir.join(".gops"));
    std::fs::create_dir_all(&cache).map_err(|e| e.to_string())?;
    let st = previz::states(&prog, &rows)?;
    let n = prog.frames as usize;
    let digests: Vec<u64> = (0..n).map(|f| frame_digest(&st[f], f as i64, prog.fps)).collect();
    let gops: Vec<(Vec<i64>, String)> = (0..n.div_ceil(gop))
        .map(|g| {
            let frames: Vec<i64> = (g * gop..((g + 1) * gop).min(n)).map(|f| f as i64).collect();
            let mut h = std::collections::hash_map::DefaultHasher::new();
            (prog.width, prog.height, prog.fps.to_bits(), qp, "openh264-gop-1").hash(&mut h);
            for f in &frames {
                digests[*f as usize].hash(&mut h);
            }
            (frames, format!("{:016x}", h.finish()))
        })
        .collect();
    let plan_s = t0.elapsed().as_secs_f64();
    let path_of = |k: &str| cache.join(format!("{k}.gop"));
    let missing: Vec<usize> = (0..gops.len()).filter(|&g| !path_of(&gops[g].1).exists()).collect();
    let t1 = std::time::Instant::now();
    if !missing.is_empty() {
        let mut e = engine::Engine::new(&prog, &rows, &dir)?;
        for &g in &missing {
            let mut enc = raea_media::soft::GopEncoder::new(prog.width, prog.height, prog.fps, qp)?;
            let mut samples: Vec<(Vec<u8>, bool)> = Vec::new();
            e.frames(&gops[g].0, |_, px| {
                samples.push(enc.push(&px)?);
                Ok(())
            })?;
            let mut b = Vec::new();
            for (data, key) in &samples {
                b.push(*key as u8);
                b.extend_from_slice(&(data.len() as u32).to_le_bytes());
                b.extend_from_slice(data);
            }
            std::fs::write(path_of(&gops[g].1), b).map_err(|e| e.to_string())?;
        }
    }
    let render_s = t1.elapsed().as_secs_f64();
    let t2 = std::time::Instant::now();
    let mut frames: Vec<(Vec<u8>, bool)> = Vec::new();
    for (_, k) in &gops {
        let b = std::fs::read(path_of(k)).map_err(|e| e.to_string())?;
        let mut i = 0;
        while i < b.len() {
            let key = b[i] == 1;
            let len = u32::from_le_bytes(b[i + 1..i + 5].try_into().unwrap()) as usize;
            frames.push((b[i + 5..i + 5 + len].to_vec(), key));
            i += 5 + len;
        }
    }
    let mix = sound(&prog, &rows, &dir)?;
    let opts = raea_media::EncodeOpts { width: prog.width, height: prog.height, fps: prog.fps, kbps: 0, nv12: false, realtime: false };
    let stats = raea_media::soft::mux_h264(out, &opts, &frames, mix.as_ref())?;
    let mux_s = t2.elapsed().as_secs_f64();
    println!(
        "{}",
        serde_json::json!({"frames": n, "gops": gops.len(), "gops_rendered": missing.len(), "gops_reused": gops.len() - missing.len(),
            "frames_rendered": missing.iter().map(|&g| gops[g].0.len()).sum::<usize>(), "distinct_frame_digests": digests.iter().collect::<std::collections::HashSet<_>>().len(),
            "seconds": {"plan": plan_s, "render_encode": render_s, "sound_mux": mux_s, "total": t0.elapsed().as_secs_f64()}, "bytes": stats.bytes})
    );
    Ok(())
}

/// The previz experiment: sketch every frame from the rows (proxies + boxes), hypervectors from the rows and from the
/// sketches, then: speed, a storyboard, the authored cuts that don't read as cuts, fidelity against a real render
/// (--vs), and which frames a snippet changes (--snippet).
fn previz_cmd(edit: &str, out: &Path, vs: Option<String>, snippet: Option<String>) -> Result<(), String> {
    let (prog, rows) = open(edit)?;
    crate::previz_report::run(edit, prog, rows, &dir_of(edit), out, vs, snippet)
}

/// G1's pass test: the hook built from snippets one at a time equals the file-built hook frame for frame; a DuckDB
/// reload gives the same frames; undoing each move restores the frames from before it.
fn g1(dir: &str) -> Result<(), String> {
    let d = Path::new(dir);
    let read = |f: &str| std::fs::read_to_string(d.join(f)).map_err(|e| format!("{f}: {e}"));
    let frames = [0i64, 14, 45, 90, 140];
    let render = |e: &mut engine::Engine| -> Result<Vec<Vec<u8>>, String> { frames.iter().map(|f| e.frame(*f)).collect() };
    let dir_abs = dir_of(&d.join("hook.sql").display().to_string());
    let same = |a: &[Vec<u8>], b: &[Vec<u8>]| a.len() == b.len() && a.iter().zip(b).all(|(x, y)| x == y);

    // the file-built hook
    let (fp, frows) = open(&d.join("hook.sql").display().to_string())?;
    let mut e = engine::Engine::new(&fp, &frows, &dir_abs)?;
    let file_frames = render(&mut e)?;

    // the same hook, one snippet at a time (snippets are separated by lines holding only `--`)
    let text = read("snippets.sql")?;
    let snippets: Vec<String> = text
        .split("\n--\n")
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty() && !s.lines().all(|l| l.trim_start().starts_with("--")))
        .collect();
    let mut db = db::Db::default();
    let mut after: Vec<Option<Vec<Vec<u8>>>> = Vec::new();
    let t0 = std::time::Instant::now();
    for (i, s) in snippets.iter().enumerate() {
        db.apply(s).map_err(|e| format!("snippet {}: {e}", i + 1))?;
        let (p, rows) = db.state.program()?;
        let renderable = rows.iter().any(|t| t.kind == "camera") && (p.width, p.height) == e.size();
        after.push(if renderable {
            e.load(&p, &rows)?;
            Some(render(&mut e)?)
        } else {
            None
        });
    }
    println!("{} snippets applied as {} moves in {:.1}s, each previewed at frames {frames:?}", snippets.len(), db.moves.len(), t0.elapsed().as_secs_f64());
    let live = after.last().cloned().flatten().ok_or("the snippets never made a renderable edit")?;
    let a = same(&live, &file_frames);
    println!("live == file-built: {a}");

    // save, reload, render
    let path = std::env::temp_dir().join("raea-g1.rdb");
    db.save(&path)?;
    let back = db::Db::load(&path)?;
    println!("reloaded tables == saved: {}", back.state == db.state);
    let (p, rows) = back.state.program()?;
    e.load(&p, &rows)?;
    let reloaded = render(&mut e)?;
    let b = same(&reloaded, &live);
    println!("reloaded frames == live: {b}");

    // undo every move, newest first: after undoing move k, the state is exactly the one after move k-1, and it renders
    // the frames rendered after move k-1 (on a fresh engine: a long-lived engine's retained render bins can break an
    // equal-depth tie the other way after a different history, reported but not the test)
    let states: Vec<db::State> = {
        let mut d = db::Db::default();
        snippets.iter().map(|s| d.apply(s).map(|_| d.state.clone())).collect::<Result<_, _>>()?
    };
    let (mut state_ok, mut state_n, mut undo_ok, mut reused_ok, mut undo_n) = (0, 0, 0, 0, 0);
    let mut d2 = back;
    for k in (1..d2.moves.len()).rev() {
        d2.undo()?;
        state_n += 1;
        if d2.state == states[k - 1] {
            state_ok += 1;
        }
        let Some(live_want) = &after[k - 1] else { continue };
        let (p, rows) = d2.state.program()?;
        undo_n += 1;
        // the earlier state's frames, rendered fresh from the state recorded when it was live
        let (wp, wrows) = states[k - 1].program()?;
        let want = &render(&mut engine::Engine::new(&wp, &wrows, &dir_abs)?)?;
        let fresh = render(&mut engine::Engine::new(&p, &rows, &dir_abs)?)?;
        if same(&fresh, want) {
            undo_ok += 1;
        } else {
            println!("  undoing move {} did not restore the frames from before it", k + 1);
        }
        e.load(&p, &rows)?;
        if same(&render(&mut e)?, live_want) {
            reused_ok += 1;
        }
    }
    println!("undo restores the earlier state exactly: {state_ok}/{state_n}");
    println!("undo restores the earlier frames: {undo_ok}/{undo_n}");
    println!("  (the same on the long-lived engine: {reused_ok}/{undo_n})");
    if !(a && b && undo_ok == undo_n && state_ok == state_n) {
        return Err("G1 pass test failed".into());
    }
    println!("G1 pass test: ok");
    Ok(())
}
