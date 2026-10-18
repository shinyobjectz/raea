//! ugc-embed: open embeddings for ad cards from the command line (docs/ad-card.feature).
//!
//!   ugc-embed image <file>...            one JSON line per image: {"file", "model", "v": [768 numbers]}
//!   ugc-embed text <phrase>...           the same for short phrases
//!   ugc-embed labels <labels.json> -o T  embed every label's phrases (averaged) into a shippable table
//!   ugc-embed classify <file>... --table T    each group's top labels per image (zero-shot)
//!   ugc-embed check <file>...            the int8 model against the fp32 reference: cosine per image, PASS at 0.99+
//!   ugc-embed audio <wav>...             YAMNet on 16 kHz mono WAVs: {"file", "model", "v": [1024], "top": [[label, score]]}
//!   raea-embed audio-frames <wav> -o COL  YAMNet per 0.48 s frame: COL.emb.f32 [n,1024], COL.cls.f32 [n,521], COL.json
//!
//! --int8 uses the shipping vision model (default: the fp32 reference). --dir sets the model folder.

use std::path::PathBuf;
use raea_embed::{default_dir, normalised, read_wav_16k, Audio, LabelTable, Text, Vision, AUDIO_DIM, AUDIO_MODEL_ID, MODEL_ID};

fn main() {
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    let take = |args: &mut Vec<String>, flag: &str| -> Option<String> {
        let i = args.iter().position(|a| a == flag)?;
        args.remove(i);
        (i < args.len()).then(|| args.remove(i))
    };
    let dir = take(&mut args, "--dir").map(PathBuf::from).unwrap_or_else(default_dir);
    let out = take(&mut args, "-o");
    let table = take(&mut args, "--table");
    let int8 = if let Some(i) = args.iter().position(|a| a == "--int8") { args.remove(i); true } else { false };
    let Some(cmd) = (!args.is_empty()).then(|| args.remove(0)) else {
        eprintln!("usage: ugc-embed image|audio|text|labels|classify|check …  (see the source header)");
        std::process::exit(2);
    };
    if let Err(e) = run(&cmd, &args, &dir, int8, out, table) {
        eprintln!("error: {e}");
        std::process::exit(1);
    }
}

fn run(cmd: &str, args: &[String], dir: &PathBuf, int8: bool, out: Option<String>, table: Option<String>) -> Result<(), String> {
    match cmd {
        "image" => {
            // --table adds each group's top three labels beside the vector (one model load for both)
            let lt: Option<LabelTable> = match &table {
                Some(tp) => Some(serde_json::from_str(&std::fs::read_to_string(tp).map_err(|e| format!("{tp}: {e}"))?).map_err(|e| format!("{tp}: {e}"))?),
                None => None,
            };
            let mut v = Vision::load(dir, int8)?;
            for f in args {
                match v.embed_file(f.as_ref()) {
                    Ok(e) => {
                        let mut row = serde_json::json!({"file": f, "model": model_id(int8), "v": round(&e)});
                        if let Some(lt) = &lt {
                            row["labels"] = serde_json::json!(lt.score(&e).into_iter().map(|(g, s)| (g, serde_json::json!(s.into_iter().take(3).map(|(l, c)| serde_json::json!([l, (c * 1000.0).round() / 1000.0])).collect::<Vec<_>>()))).collect::<serde_json::Map<String, serde_json::Value>>());
                        }
                        println!("{row}");
                    }
                    Err(e) => eprintln!("{f}: {e}"),
                }
            }
        }
        "audio" => {
            let mut a = Audio::load(&dir.parent().map(|p| p.join("yamnet")).unwrap_or_default())?;
            for f in args {
                match read_wav_16k(f.as_ref()).and_then(|pcm| a.embed_pcm(&pcm)) {
                    Ok(s) => {
                        let top: Vec<_> = a.top(&s, 5).into_iter().map(|(l, c)| serde_json::json!([l, (c * 1000.0).round() / 1000.0])).collect();
                        println!("{}", serde_json::json!({"file": f, "model": AUDIO_MODEL_ID, "v": round(&s.embedding), "top": top}));
                    }
                    Err(e) => eprintln!("{f}: {e}"),
                }
            }
        }
        "audio-frames" => {
            // raea-embed audio-frames WAV -o COL: COL.emb.f32 [n,1024] + COL.cls.f32 [n,521] + COL.json, one row per 0.48 s
            let (wav, col) = match (args.first(), &out) {
                (Some(w), Some(c)) => (w.clone(), c.clone()),
                _ => return Err("audio-frames needs WAV -o COL".into()),
            };
            let mut a = Audio::load(&dir.parent().map(|p| p.join("yamnet")).unwrap_or_default())?;
            let f = a.frames_pcm(&read_wav_16k(wav.as_ref())?)?;
            let write = |ext: &str, rows: &[Vec<f32>]| -> Result<(), String> {
                let bytes: Vec<u8> = rows.iter().flatten().flat_map(|x| x.to_le_bytes()).collect();
                std::fs::write(format!("{col}.{ext}.f32"), bytes).map_err(|e| format!("{col}.{ext}.f32: {e}"))
            };
            write("emb", &f.embeddings)?;
            write("cls", &f.scores)?;
            let meta = serde_json::json!({"source": wav, "model": AUDIO_MODEL_ID, "hop_s": raea_embed::AUDIO_HOP_S, "window_s": 0.96,
                                          "n": f.embeddings.len(), "emb_dim": AUDIO_DIM, "cls_dim": raea_embed::AUDIO_CLASSES});
            std::fs::write(format!("{col}.json"), serde_json::to_string_pretty(&meta).unwrap()).map_err(|e| e.to_string())?;
            eprintln!("{} frames -> {col}", f.embeddings.len());
        }
        "text" => {
            let mut t = Text::load(dir)?;
            for s in args {
                println!("{}", serde_json::json!({"text": s, "model": MODEL_ID, "v": round(&t.embed(s)?)}));
            }
        }
        "labels" => {
            let src = args.first().ok_or("labels needs a labels.json")?;
            let mut lt: LabelTable = serde_json::from_str(&std::fs::read_to_string(src).map_err(|e| format!("{src}: {e}"))?).map_err(|e| format!("{src}: {e}"))?;
            let mut t = Text::load(dir)?;
            for g in &mut lt.groups {
                for l in &mut g.labels {
                    let mut sum = vec![0f32; raea_embed::DIM];
                    for p in &l.phrases {
                        for (s, x) in sum.iter_mut().zip(t.embed(p)?) {
                            *s += x;
                        }
                    }
                    l.v = round(&normalised(&sum));
                }
            }
            lt.model = MODEL_ID.into();
            let json = serde_json::to_string(&lt).map_err(|e| e.to_string())?;
            match out {
                Some(o) => std::fs::write(&o, json).map_err(|e| format!("{o}: {e}"))?,
                None => println!("{json}"),
            }
        }
        "classify" => {
            let tp = table.ok_or("classify needs --table (from `ugc-embed labels`)")?;
            let lt: LabelTable = serde_json::from_str(&std::fs::read_to_string(&tp).map_err(|e| format!("{tp}: {e}"))?).map_err(|e| format!("{tp}: {e}"))?;
            let mut v = Vision::load(dir, int8)?;
            for f in args {
                let e = v.embed_file(f.as_ref())?;
                let groups: serde_json::Map<String, serde_json::Value> = lt
                    .score(&e)
                    .into_iter()
                    .map(|(g, s)| (g, serde_json::json!(s.iter().take(3).map(|(l, c)| serde_json::json!([l, (c * 1000.0).round() / 1000.0])).collect::<Vec<_>>())))
                    .collect();
                println!("{}", serde_json::json!({"file": f, "labels": groups}));
            }
        }
        "check" => {
            let (mut r, mut q) = (Vision::load(dir, false)?, Vision::load(dir, true)?);
            let mut worst = 1f32;
            for f in args {
                let c = raea_embed::cosine(&r.embed_file(f.as_ref())?, &q.embed_file(f.as_ref())?);
                worst = worst.min(c);
                println!("{f}: cosine {c:.4}");
            }
            println!("{} (worst {worst:.4} over {} images; the int8 model ships only at 0.99 or better)", if worst >= 0.99 { "PASS" } else { "FAIL" }, args.len());
        }
        other => return Err(format!("unknown command {other}")),
    }
    Ok(())
}

fn model_id(int8: bool) -> String {
    format!("{MODEL_ID}{}", if int8 { "-int8" } else { "" })
}

fn round(v: &[f32]) -> Vec<f32> {
    v.iter().map(|x| (x * 1e5).round() / 1e5).collect()
}
