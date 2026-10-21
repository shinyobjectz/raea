//! Machine learning and speech for compositions (raea-ml): the `ml.*` native calls behind @raea/motion/ml (ONNX models
//! loaded once per process and run on tensors) and the `speech.*` calls behind @raea/motion/speech (lines said by
//! Pocket TTS, and their word clock), plus each frame's speech for the mixer (`frame_speech`, from audio.rs tone_at).
//!
//!   ml.load    {src, provider?, threads?, optimization?} -> {id, file, inputs, outputs, deterministic}
//!   ml.run     {id, inputs, outputs?}                     -> {name: tensor}           (waits; pure)
//!   ml.request {id, inputs}                               -> {job}                    (starts it on a worker)
//!   ml.result  {job}                                      -> {name: tensor}           (waits; pure)
//!   ml.poll    {job}                                      -> {done, outputs?}         (display-only)
//!   ml.release {id, now?}; ml.loaded {}
//!   speech.line {text, voice?, seed?, temperature?}       -> {key}                    (starts saying it)
//!   speech.at   {..line, t, cut?}                         -> {word, ended, end?, words, total?} (waits; pure)
//!   speech.duration {..line}                              -> seconds                  (waits for all of it; pure)
//!   speech.made {..line}                                  -> {seconds, done}          (display-only)
//!   speech.stats {}                                       -> the lines and the waits, for a report
//!
//! A tensor is {type: "float32" | "int64" | ..., shape: number[], data: base64 of its little-endian bytes}.
//!
//! Paths: "/media/x.onnx" is the composition's media folder, "cache:name/file.onnx" raea's model cache
//! (~/.cache/raea-void/models), an absolute path itself. A voice is a name (Pocket's built-ins, or a clip in
//! ~/.raea-void/voices) or a WAV path ("/media/narrator.wav").

use std::cell::RefCell;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use base64::Engine as _;
use serde_json::{Map, Value, json};

type R<T> = Result<T, String>;

static DIR: Mutex<Option<PathBuf>> = Mutex::new(None);

/// The composition's folder, where "/media/..." resolves (set when a composition is bundled or opened).
pub fn set_dir(dir: &Path) {
    *DIR.lock().unwrap() = Some(dir.to_path_buf());
}

/// The composition's folder, if one is open (matchmove.solve's store and media).
pub fn dir() -> Option<PathBuf> {
    DIR.lock().unwrap().clone()
}

fn resolve(src: &str) -> R<PathBuf> {
    if let Some(rest) = src.strip_prefix("cache:") {
        return Ok(raea_ml::models::root().join(rest));
    }
    let dir = DIR.lock().unwrap().clone();
    if let Some(rest) = src.strip_prefix("/media/") {
        let dir = dir.ok_or("no composition is open, so /media/ has no folder")?;
        let rest = rest.split('/').map(|s| percent_decode(s)).collect::<Vec<_>>().join("/");
        return Ok(dir.join("media").join(rest));
    }
    let p = PathBuf::from(src);
    if p.is_absolute() {
        return Ok(p);
    }
    Ok(dir.map(|d| d.join(src)).unwrap_or(p))
}

fn percent_decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' && i + 3 <= b.len() {
            if let Ok(v) = u8::from_str_radix(&s[i + 1..i + 3], 16) {
                out.push(v);
                i += 3;
                continue;
            }
        }
        out.push(b[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn tensor_in(v: &Value) -> R<raea_ml::Tensor> {
    let dtype = raea_ml::DType::parse(v.get("type").and_then(Value::as_str).unwrap_or("float32")).map_err(|e| e.to_string())?;
    let shape: Vec<i64> = v.get("shape").and_then(Value::as_array).ok_or("a tensor needs its shape")?.iter().map(|d| d.as_i64().ok_or_else(|| "a shape is whole numbers".to_string())).collect::<R<_>>()?;
    let bytes = match v.get("data") {
        Some(Value::String(b64)) => base64::engine::general_purpose::STANDARD.decode(b64).map_err(|e| format!("tensor data: {e}"))?,
        // plain numbers, for small tensors written by hand
        Some(Value::Array(a)) => {
            let mut out = Vec::with_capacity(a.len() * dtype.size());
            for x in a {
                let f = x.as_f64().or_else(|| x.as_bool().map(|b| b as u8 as f64)).ok_or("tensor data: numbers")?;
                match dtype {
                    raea_ml::DType::F32 => out.extend((f as f32).to_le_bytes()),
                    raea_ml::DType::F64 => out.extend(f.to_le_bytes()),
                    raea_ml::DType::F16 => out.extend(raea_ml::half::f16::from_f64(f).to_bits().to_le_bytes()),
                    raea_ml::DType::I64 => out.extend((f as i64).to_le_bytes()),
                    raea_ml::DType::I32 => out.extend((f as i32).to_le_bytes()),
                    raea_ml::DType::I16 => out.extend((f as i16).to_le_bytes()),
                    raea_ml::DType::I8 => out.extend((f as i8).to_le_bytes()),
                    raea_ml::DType::U8 | raea_ml::DType::Bool => out.push(f as u8),
                    raea_ml::DType::U16 => out.extend((f as u16).to_le_bytes()),
                    raea_ml::DType::U32 => out.extend((f as u32).to_le_bytes()),
                    raea_ml::DType::U64 => out.extend((f as u64).to_le_bytes()),
                }
            }
            out
        }
        _ => return Err("a tensor needs its data (base64, or an array of numbers)".into()),
    };
    Ok(raea_ml::Tensor { dtype, shape, bytes })
}

fn tensor_out(t: &raea_ml::Tensor) -> Value {
    json!({"type": t.dtype.name(), "shape": t.shape, "data": base64::engine::general_purpose::STANDARD.encode(&t.bytes)})
}

fn feeds(v: &Value) -> R<Vec<(String, raea_ml::Tensor)>> {
    let m = v.get("inputs").and_then(Value::as_object).ok_or("inputs: {name: tensor}")?;
    m.iter().map(|(k, t)| Ok((k.clone(), tensor_in(t).map_err(|e| format!("{k}: {e}"))?))).collect()
}

fn outputs(o: &[(String, raea_ml::Tensor)]) -> Value {
    Value::Object(o.iter().map(|(k, t)| (k.clone(), tensor_out(t))).collect::<Map<_, _>>())
}

fn io(v: &[raea_ml::Io]) -> Value {
    Value::Array(v.iter().map(|i| json!({"name": i.name, "type": i.dtype, "shape": i.shape})).collect())
}

fn id_of(v: &Value) -> R<u64> {
    v.get("id").and_then(Value::as_u64).ok_or_else(|| "id: a model's id (from ml.load)".to_string())
}

fn spec(v: &Value) -> R<raea_ml::speech::Spec> {
    let text = v.get("text").and_then(Value::as_str).ok_or("text: what to say")?.to_string();
    let voice = match v.get("voice").and_then(Value::as_str) {
        Some(x) if x.ends_with(".wav") => resolve(x)?.display().to_string(),
        Some(x) => x.to_string(),
        None => raea_ml::pocket::Pocket::default_voice(),
    };
    Ok(raea_ml::speech::Spec { text, voice, seed: v.get("seed").and_then(Value::as_u64).unwrap_or(0), temperature: v.get("temperature").and_then(Value::as_f64).unwrap_or(0.7) as f32 })
}

const TENSOR: &str = "{ type: string; shape: number[]; data: string | number[] }";
const IO: &str = "{ name: string; type: string; shape: number[] }[]";
const LINE: &str = "text: string; voice?: string; seed?: number; temperature?: number";

/// Registers the ml.* and speech.* calls (native::register calls it once).
pub fn register() {
    use raea_native::register as reg;
    reg("ml.load", "{ src: string; provider?: \"cpu\" | \"coreml\"; threads?: number; optimization?: number }", &format!("{{ id: number; file: string; inputs: {IO}; outputs: {IO}; deterministic: boolean }}"), "an ONNX model loaded (or the session already loaded for it): its inputs and outputs; held until ml.release", |v| {
        let src = v.get("src").and_then(Value::as_str).ok_or("src: the model's path")?;
        let provider = match v.get("provider").and_then(Value::as_str).unwrap_or("cpu") {
            "cpu" => raea_ml::Provider::Cpu,
            "coreml" => raea_ml::Provider::CoreMl,
            p => return Err(format!("provider {p:?}: cpu or coreml")),
        };
        let o = raea_ml::Options { provider, threads: v.get("threads").and_then(Value::as_u64).map(|n| n as usize), optimization: v.get("optimization").and_then(Value::as_u64).unwrap_or(3) as u8 };
        let m = raea_ml::load(&resolve(src)?, &o).map_err(|e| format!("{e:#}"))?;
        Ok(json!({"id": m.id, "file": m.file, "inputs": io(&m.inputs), "outputs": io(&m.outputs), "deterministic": m.deterministic()}))
    });
    reg("ml.run", &format!("{{ id: number; inputs: Record<string, {TENSOR}>; outputs?: string[] }}"), &format!("Record<string, {TENSOR}>"), "a model run on these inputs (waits for it): its outputs", |v| {
        let m = raea_ml::get(id_of(v)?).map_err(|e| e.to_string())?;
        let wanted: Option<Vec<String>> = v.get("outputs").and_then(Value::as_array).map(|a| a.iter().filter_map(|x| x.as_str().map(str::to_string)).collect());
        let o = m.run(&feeds(v)?, wanted.as_deref()).map_err(|e| format!("{e:#}"))?;
        Ok(outputs(&o))
    });
    reg("ml.request", &format!("{{ id: number; inputs: Record<string, {TENSOR}> }}"), "{ job: string }", "a model run started on a worker, named by its model and inputs (the same run asked again is the same job)", |v| {
        let m = raea_ml::get(id_of(v)?).map_err(|e| e.to_string())?;
        Ok(json!({"job": raea_ml::request(m, feeds(v)?)}))
    });
    reg("ml.result", "{ job: string }", &format!("Record<string, {TENSOR}>"), "a job's outputs, waiting for it to finish (pure: safe to fold into a game's state)", |v| {
        let job = v.get("job").and_then(Value::as_str).ok_or("job")?;
        let o = raea_ml::result(job).map_err(|e| format!("{e:#}"))?;
        Ok(outputs(&o))
    });
    reg("ml.poll", "{ job: string }", &format!("{{ done: boolean; outputs?: Record<string, {TENSOR}>; error?: string }}"), "a job's outputs if it has finished (display-only: when it finishes depends on the machine)", |v| {
        let job = v.get("job").and_then(Value::as_str).ok_or("job")?;
        Ok(match raea_ml::poll(job) {
            None => json!({"done": false}),
            Some(Ok(o)) => json!({"done": true, "outputs": outputs(&o)}),
            Some(Err(e)) => json!({"done": true, "error": format!("{e:#}")}),
        })
    });
    reg("ml.release", "{ id: number; now?: boolean }", "null", "a hold on a model let go (freed 30 s after its last, or at once with now)", |v| {
        raea_ml::release(id_of(v)?, v.get("now").and_then(Value::as_bool).unwrap_or(false));
        Ok(Value::Null)
    });
    reg("ml.loaded", "{}", "{ id: number; file: string; holds: number; runs: number; ms: number }[]", "the models loaded in this process", |_| {
        Ok(Value::Array(raea_ml::loaded().into_iter().map(|(id, f, holds, runs, t)| json!({"id": id, "file": f, "holds": holds, "runs": runs, "ms": t.as_secs_f64() * 1e3})).collect()))
    });
    reg("speech.line", &format!("{{ {LINE} }}"), "{ key: string }", "a line asked for: Pocket TTS starts saying it on its worker (or it's read back from the cache)", |v| {
        let s = raea_ml::speech::service().line(&spec(v)?);
        Ok(json!({"key": s.key}))
    });
    reg("speech.at", &format!("{{ {LINE}; t: number; cut?: number }}"), "{ word: number; ended: boolean; end: number | null; words: string[]; total: number | null }", "which word a line is on t seconds in (-1 before the first), and whether it has ended (cut at `cut` seconds: at the next word); waits until that's decided (pure)", |v| {
        let svc = raea_ml::speech::service();
        let slot = svc.line(&spec(v)?);
        let t = v.get("t").and_then(Value::as_f64).ok_or("t: seconds into the line")?;
        let cut = v.get("cut").and_then(Value::as_f64);
        let end = cut.map(|c| svc.cut_at(&slot, c));
        let (word, ended, words) = svc.word_at(&slot, t);
        let l = slot.lock();
        let ended = match end {
            Some(e) => t >= e || ended,
            None => ended,
        };
        let end = end.or_else(|| l.done.then(|| l.audio.len() as f64 / raea_ml::speech::RATE as f64));
        let total = l.done.then(|| l.clock.words.len());
        Ok(json!({"word": word.map(|w| w as i64).unwrap_or(-1), "ended": ended, "end": end, "words": words, "total": total}))
    });
    reg("speech.duration", &format!("{{ {LINE} }}"), "number", "a line's length in seconds (waits for all of it to be made; pure)", |v| {
        let svc = raea_ml::speech::service();
        Ok(json!(svc.duration(&svc.line(&spec(v)?))))
    });
    reg("speech.made", &format!("{{ {LINE} }}"), "{ seconds: number; done: boolean }", "how much of a line has been made so far (display-only)", |v| {
        let slot = raea_ml::speech::service().line(&spec(v)?);
        let l = slot.lock();
        Ok(json!({"seconds": l.audio.len() as f64 / raea_ml::speech::RATE as f64, "done": l.done}))
    });
    reg("speech.stats", "{}", "unknown", "every line asked for (first audio, real-time factor, cached) and the frames' waits on the worker", |_| Ok(stats()));
}

/// The speech report: each line's first audio and real-time factor, and how long frames waited on the worker.
pub fn stats() -> Value {
    let svc = raea_ml::speech::service();
    let mut lines: Vec<Value> = svc
        .lines()
        .iter()
        .map(|s| {
            let l = s.lock();
            let secs = l.audio.len() as f64 / raea_ml::speech::RATE as f64;
            json!({"text": l.spec.text.chars().take(48).collect::<String>(), "seconds": secs, "done": l.done, "cached": l.from_cache,
                "first_audio_ms": l.first_audio.map(|d| d.as_secs_f64() * 1e3), "rtf": if secs > 0.0 && !l.from_cache { Some(l.made_in.as_secs_f64() / secs) } else { None }, "error": l.error})
        })
        .collect();
    lines.sort_by(|a, b| a["text"].as_str().cmp(&b["text"].as_str()));
    let w = svc.waits();
    json!({"lines": lines, "waits": {"count": w.count, "total_ms": w.total.as_secs_f64() * 1e3, "longest_ms": w.longest.as_secs_f64() * 1e3}, "status": svc.status()})
}

/// A line on the page this frame (`<audio data-speech>`).
struct Playing {
    spec: raea_ml::speech::Spec,
    start: i64,
    cut: Option<i64>,
    volume: f32,
}

thread_local! {
    /// lines heard last frame (key -> (slot, start frame, volume, frame)), so one taken off the page fades out instead
    /// of clicking; and each line's cut point (key, cut frame) -> seconds
    static HEARD: RefCell<HashMap<String, (Arc<raea_ml::speech::Slot>, i64, f32, i64, Option<f64>)>> = RefCell::new(HashMap::new());
    static CUTS: RefCell<HashMap<(String, i64), f64>> = RefCell::new(HashMap::new());
}

fn playing_of(p: &Map<String, Value>) -> Option<Playing> {
    let raw = p.get("data-speech")?.as_str()?;
    let v: Value = serde_json::from_str(raw).ok()?;
    let spec = spec(&v).ok()?;
    Some(Playing { spec, start: v.get("start").and_then(Value::as_i64)?, cut: v.get("cut").and_then(Value::as_i64), volume: v.get("volume").and_then(Value::as_f64).unwrap_or(1.0) as f32 })
}

/// Whether a bundle says lines (so Pocket is loaded before the clock starts).
pub fn bundle_speaks(js: &Path) -> bool {
    std::fs::read_to_string(js).is_ok_and(|t| t.contains("data-speech"))
}

/// Loads Pocket on its worker now (live play calls it as a speaking game opens).
pub fn warm() {
    raea_ml::speech::service().warm();
}

/// Frame `f`'s speech: every `<audio data-speech>` line on the page, the frame's span of each (from its start frame,
/// faded out at its cut) summed, at the mixer's rate. Waits for samples the worker hasn't made yet: a render always
/// has them, live play only waits when the worker is behind (the wait is counted in speech.stats).
pub fn frame_speech(items: &[&Map<String, Value>], f: i64, fps: f64) -> Option<Arc<raea_media::Audio>> {
    let rate = raea_mixer::RATE;
    let t = |g: i64| (g as f64 * rate as f64 / fps).round() as i64;
    let lines: Vec<Playing> = items.iter().filter_map(|p| playing_of(p)).collect();
    let had = HEARD.with(|h| !h.borrow().is_empty());
    if lines.is_empty() && !had {
        return None;
    }
    let svc = raea_ml::speech::service();
    let n = (t(f + 1) - t(f)).max(0) as usize;
    let mut out = vec![0f32; n];
    let mut any = false;
    let mut now: HashMap<String, (Arc<raea_ml::speech::Slot>, i64, f32, i64, Option<f64>)> = HashMap::new();
    for p in &lines {
        let slot = svc.line(&p.spec);
        if f < p.start {
            continue;
        }
        let cut = p.cut.filter(|c| *c <= f).map(|c| {
            let key = (slot.key.clone(), c);
            CUTS.with(|m| *m.borrow_mut().entry(key).or_insert_with(|| svc.cut_at(&slot, (c - p.start) as f64 / fps)))
        });
        let from = t(f) - t(p.start);
        // past the line's end (once it's all made) nothing's left to play
        let l = slot.lock();
        let over = l.done && from as f64 / rate as f64 >= l.audio.len() as f64 / raea_ml::speech::RATE as f64;
        drop(l);
        if over || cut.is_some_and(|c| from as f64 / rate as f64 >= c) {
            continue;
        }
        svc.render(&slot, rate, from as usize, n, cut, p.volume, &mut out);
        any = true;
        now.insert(slot.key.clone(), (slot.clone(), p.start, p.volume, f, cut));
    }
    // lines taken off the page mid-word: 10 ms faded out
    HEARD.with(|h| {
        for (key, (slot, start, vol, last, cut)) in h.borrow().iter() {
            if now.contains_key(key) || *last != f - 1 {
                continue;
            }
            let from = (t(f) - t(*start)) as usize;
            let fade = ((rate as f64 * 0.010) as usize).min(n);
            let mut tail = vec![0f32; fade];
            svc.render(slot, rate, from, fade, *cut, *vol, &mut tail);
            for (i, x) in tail.iter().enumerate() {
                out[i] += x * (1.0 - i as f32 / fade as f32);
            }
            any = true;
        }
        *h.borrow_mut() = now;
    });
    any.then(|| Arc::new(raea_media::Audio { rate, channels: vec![out] }))
}
