//! A timeline baked: a composition's React commits recorded frame by frame (React stepped through every frame once, as
//! a render steps it), with keyframes of the whole tree every KEY frames. Playback replays them into the same ECS page,
//! Blitz and compositor (lib.rs ReplayPlugin, live.rs), with no JS running: a timeline is a function of its frame, so
//! what React committed for frame f is all frame f needs, and a render and a play of the same bake are the same pixels.

use std::path::Path;

use bevy::prelude::*;
use serde_json::{json, Map, Value};

use raea_runtime_react::{CommitTap, ReactOutbox, ReactPlugin};

type R<T> = Result<T, String>;

/// Frames between keyframes: a seek replays at most this many frames' commits after its keyframe.
pub const KEY: i64 = 10;

pub struct Baked {
    /// the composition's settings (width, height, fps, frames)
    pub conf: Map<String, Value>,
    pub fps: f64,
    pub frames: i64,
    pub w: u32,
    pub h: u32,
    /// the commits that built the composition before its first frame
    pub init: Vec<String>,
    /// each frame's commits, in order, applied on top of the frame before
    pub frame_ops: Vec<Vec<String>>,
    /// the whole tree after frame k (k = -1: after the first commits), every KEY frames
    pub keys: Vec<(i64, Vec<Value>)>,
    /// each frame's components by code (a bundle made with --lite --comps; empty otherwise): which frames an edit to a
    /// component touches
    pub comps: Vec<Vec<u32>>,
    /// the bundle's functions and the code outside them (`code_of`)
    pub code: Code,
    /// some frame shows 3D (a data-3d element): play opens the scene's rows and draws it under the page, as the
    /// render does
    pub has_3d: bool,
}

/// A batch that clears the page first: what's under the root goes, then the batches after it build the tree again
/// (frames re-stepped by `bake_incremental` come from another JS runtime, whose node ids aren't the bake's).
pub const RESET: &str = "RESET";

/// A bundle's code as an edit is compared: each function by its code (FNV-1a over the UTF-16 of its source, as
/// Function.prototype.toString gives it and sdk/react/js/lite hashes a rendered component) with the codes of the functions
/// around it, and the code outside every function.
#[derive(Clone, Default)]
pub struct Code {
    /// each function's code -> the codes of the functions around it, innermost first
    pub funcs: std::collections::HashMap<u32, Vec<u32>>,
    /// each function's own code with the functions inside it left out, and its whole code: a function whose own code
    /// is unchanged changed only inside the functions it holds (the bundle's module wrapper holds every component)
    pub shells: Vec<(u32, u32)>,
    pub rest: u64,
}

fn fnv_utf16(s: &str) -> u32 {
    let mut h: u32 = 0x811c9dc5;
    for u in s.encode_utf16() {
        h = (h ^ u as u32).wrapping_mul(0x01000193);
    }
    h
}

/// The bundle at `js` as `Code`.
pub fn code_of(js: &Path) -> R<Code> {
    let text = std::fs::read_to_string(js).map_err(|e| format!("{}: {e}", js.display()))?;
    // the bundle's functions (raea-modules: swc); without the `modules` feature (an exported game's player) none, so
    // an incremental bake steps every frame
    #[cfg(feature = "modules")]
    let spans = raea_modules::function_spans(&text).map_err(|e| e.to_string())?;
    #[cfg(not(feature = "modules"))]
    let spans: Vec<(usize, usize, Option<usize>)> = Vec::new();
    let hashes: Vec<u32> = spans.iter().map(|(a, b, _)| fnv_utf16(&text[*a..*b])).collect();
    let mut shells = Vec::with_capacity(spans.len());
    for (i, (a, b, _)) in spans.iter().enumerate() {
        let mut own = String::new();
        let mut at = *a;
        for (ca, cb, cp) in &spans {
            if *cp == Some(i) && *ca >= at {
                own.push_str(&text[at..*ca]);
                own.push('\u{0}');
                at = *cb;
            }
        }
        own.push_str(&text[at..*b]);
        shells.push((fnv_utf16(&own), hashes[i]));
    }
    let mut funcs = std::collections::HashMap::new();
    for (i, (_, _, parent)) in spans.iter().enumerate() {
        let mut chain = Vec::new();
        let mut at = *parent;
        while let Some(p) = at {
            chain.push(hashes[p]);
            at = spans[p].2;
        }
        funcs.insert(hashes[i], chain);
    }
    // outside every function: the outermost functions' text left out
    let mut rest = 0xcbf2_9ce4_8422_2325u64;
    let mut at = 0;
    for (a, b, parent) in &spans {
        if parent.is_some() || *a < at {
            continue;
        }
        for byte in text[at..*a].bytes() {
            rest = (rest ^ byte as u64).wrapping_mul(0x100_0000_01b3);
        }
        at = *b;
    }
    for byte in text[at..].bytes() {
        rest = (rest ^ byte as u64).wrapping_mul(0x100_0000_01b3);
    }
    Ok(Code { funcs, shells, rest })
}

/// The components the frame marker says rendered (`data-raea-comps`, a --comps bundle's).
fn comps_of(world: &mut World) -> Vec<u32> {
    let mut q = world.query::<&raea_runtime_react::ReactProps>();
    let s = q.iter(world).find(|p| p.0.contains_key("data-raea-comps")).and_then(|p| p.0.get("data-raea-comps").and_then(Value::as_str).map(str::to_string));
    s.map(|s| s.split(',').filter_map(|c| u32::from_str_radix(c, 16).ok()).collect()).unwrap_or_default()
}

/// `tsx`'s composition baked (its bundle at `js`, made by the caller): every frame stepped once, its commits kept.
///
/// Frames are pure, so the bake is split across runtimes (lightspeed T3, L1): the first range on this thread, each
/// later one in its own app and JS runtime on a thread, its first frame a RESET and that runtime's whole tree (as
/// bake_incremental joins a re-stepped run), its keyframes its own. RAEA_BAKE_WORKERS sets how many (default: 4, one
/// for short timelines).
pub fn bake(js: &Path) -> R<Baked> {
    let (t0, timeouts0) = (std::time::Instant::now(), raea_runtime_react::FLUSH_TIMEOUTS.load(std::sync::atomic::Ordering::Relaxed));
    let mut app = runtime(js);
    let conf = raea_runtime_react::scene::settle(&mut app, Some(-1), 20.0)?.ok_or("the composition's settings didn't arrive")?;
    let n = |k: &str, d: f64| conf.get(k).and_then(Value::as_f64).unwrap_or(d);
    let (w, h, fps, frames) = (n("width", 1080.0) as u32, n("height", 1920.0) as u32, n("fps", 30.0), n("frames", 1.0).max(1.0) as i64);
    let t_conf = t0.elapsed().as_secs_f64() * 1e3;
    let init = take_ops(&mut app);
    // the ranges, each a whole number of keyframe spans (at least 60 frames each)
    let workers = std::env::var("RAEA_BAKE_WORKERS").ok().and_then(|v| v.parse::<i64>().ok()).unwrap_or(4).clamp(1, 16).min((frames / 60).max(1));
    let span = (((frames + workers - 1) / workers + KEY - 1) / KEY) * KEY;
    let ranges: Vec<(i64, i64)> = (0..workers).map(|k| (k * span, ((k + 1) * span).min(frames))).filter(|(a, b)| a < b).collect();
    let later: Vec<std::thread::JoinHandle<R<Range>>> = ranges[1..]
        .iter()
        .map(|&(a, b)| {
            let js = js.to_path_buf();
            std::thread::Builder::new().name("raea bake".into()).spawn(move || {
                let mut app = runtime(&js);
                raea_runtime_react::scene::settle(&mut app, Some(-1), 20.0)?;
                take_ops(&mut app);
                step_range(&mut app, a, b, true)
            })
        })
        .collect::<Result<_, _>>()
        .map_err(|e| format!("a bake worker: {e}"))?;
    let mut keys = vec![(-1, raea_runtime_react::snapshot_ops(app.world()))];
    let first = step_range(&mut app, ranges[0].0, ranges[0].1, false)?;
    let mut frame_ops = Vec::with_capacity(frames as usize);
    let mut comps = Vec::with_capacity(frames as usize);
    let mut has_3d = init.iter().any(|j| j.contains("data-3d"));
    let mut add = |r: Range| {
        frame_ops.extend(r.frame_ops);
        comps.extend(r.comps);
        keys.extend(r.keys);
        has_3d |= r.has_3d;
    };
    add(first);
    for t in later {
        add(t.join().map_err(|_| "a bake worker panicked".to_string())??);
    }
    if raea_io_trace::stat("RAEA_BAKE_TRACE") {
        eprintln!("raea bake: settings in {t_conf:.0} ms, {frames} frames in {:.0} ms on {} runtimes, {} flush waits timed out",
            t0.elapsed().as_secs_f64() * 1e3 - t_conf, ranges.len(), raea_runtime_react::FLUSH_TIMEOUTS.load(std::sync::atomic::Ordering::Relaxed) - timeouts0);
    }
    Ok(Baked { conf, fps, frames, w, h, init, frame_ops, keys, comps, code: code_of(js)?, has_3d })
}

/// One range of a bake: its frames' commits, components and keyframes, and whether any frame shows 3D.
struct Range {
    frame_ops: Vec<Vec<String>>,
    comps: Vec<Vec<u32>>,
    keys: Vec<(i64, Vec<Value>)>,
    has_3d: bool,
}

/// An app running the bundle's composition with its commits tapped.
fn runtime(js: &Path) -> App {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins).add_plugins(ReactPlugin::new(js.to_path_buf()));
    app.insert_resource(CommitTap::default()).insert_resource(raea_runtime_react::OneBatch::default());
    app
}

/// Sorted frames as runs of neighbours, each to step as one range (`step_frames`): 3, 4, 5, 9 are 3..6 and 9..10.
pub fn runs<'a>(frames: impl IntoIterator<Item = &'a i64>) -> Vec<(i64, i64)> {
    frames.into_iter().fold(Vec::new(), |mut rs: Vec<(i64, i64)>, &f| {
        match rs.last_mut() {
            Some(r) if r.1 == f => r.1 = f + 1,
            _ => rs.push((f, f + 1)),
        }
        rs
    })
}

/// Frames `a..b` stepped as one range (lightspeed T3): React renders them back to back (raea-frames), with no round
/// trip to Rust between frames, and each frame's commit is applied on its own (OneBatch), `each` reading the world
/// after it. Stepped one event a frame, each frame waited on the JS thread and back: most of an incremental bake.
pub fn step_frames(app: &mut App, a: i64, b: i64, each: impl FnMut(&mut App, i64) -> R<()>) -> R<()> {
    step_frames_with(app, a, b, &mut raea_runtime_react::layout::nothing_drawn(), each)
}

/// [`step_frames`], a layout read the composition makes answered by `answer` (`page.layout`, docs/layout-reads.feature
/// LR2): a host that lays a page out answers from it, a pass that draws none with no boxes.
pub fn step_frames_with(app: &mut App, a: i64, b: i64, answer: &mut raea_runtime_react::layout::Answer, mut each: impl FnMut(&mut App, i64) -> R<()>) -> R<()> {
    if b <= a {
        return Ok(());
    }
    let ld = |a: &std::sync::atomic::AtomicU64| a.load(std::sync::atomic::Ordering::Relaxed);
    let (t0, w0, a0, j0) = (std::time::Instant::now(), ld(&raea_runtime_react::FLUSH_WAIT_NS), ld(&raea_runtime_react::APPLY_NS), ld(&raea_runtime_react::JS_NS));
    let mut t_each = std::time::Duration::ZERO;
    app.world().resource::<ReactOutbox>().send(json!({"t": "event", "name": "raea-frames", "value": {"from": a, "to": b}}));
    // the range's end, for a settle that finds a frame lost to ask for the rest again (#158)
    app.world_mut().insert_resource(raea_runtime_react::Stepping { to: b });
    let stepped = (a..b).try_for_each(|g| {
        raea_runtime_react::scene::settle_with(app, Some(g), 20.0, answer)?;
        let te = std::time::Instant::now();
        each(app, g)?;
        t_each += te.elapsed();
        Ok::<(), String>(())
    });
    app.world_mut().remove_resource::<raea_runtime_react::Stepping>();
    stepped?;
    if raea_io_trace::stat("RAEA_BAKE_TRACE") {
        let ms = |a: &std::sync::atomic::AtomicU64, z: u64| (ld(a) - z) as f64 / 1e6;
        eprintln!(
            "raea bake: frames {a}..{b} in {:.1} ms: waited on React {:.1} ms, applying {:.1} ms, JS {:.1} ms (those three across every runtime), recording {:.1} ms",
            t0.elapsed().as_secs_f64() * 1e3,
            ms(&raea_runtime_react::FLUSH_WAIT_NS, w0),
            ms(&raea_runtime_react::APPLY_NS, a0),
            ms(&raea_runtime_react::JS_NS, j0),
            t_each.as_secs_f64() * 1e3
        );
    }
    Ok(())
}

fn take_ops(app: &mut App) -> Vec<String> {
    std::mem::take(&mut app.world_mut().resource_mut::<CommitTap>().0)
}

/// Frames `a..b` stepped in `app`; with `reset`, the first frame's commits are a RESET and the whole tree (another
/// runtime's node ids don't follow on from the range before).
fn step_range(app: &mut App, a: i64, b: i64, reset: bool) -> R<Range> {
    let mut r = Range { frame_ops: Vec::with_capacity((b - a) as usize), comps: Vec::with_capacity((b - a) as usize), keys: Vec::new(), has_3d: false };
    // the render's own steps: frame numbers (not live play's {frame, input}), so the commits are a render's. A bake lays
    // no page out, so a layout read is refused, plainly, rather than baked as 0 (docs/layout-reads.feature LR2)
    let mut answer = raea_runtime_react::layout::no_page("a baked timeline (raea play --baked; play it without --baked)");
    step_frames_with(app, a, b, &mut answer, |app, f| {
        r.comps.push(comps_of(app.world_mut()));
        let ops = take_ops(app);
        r.has_3d |= ops.iter().any(|j| j.contains("data-3d"));
        r.frame_ops.push(if reset && f == a { vec![RESET.to_string(), Value::Array(raea_runtime_react::snapshot_ops(app.world())).to_string()] } else { ops });
        if f % KEY == KEY - 1 {
            r.keys.push((f, raea_runtime_react::snapshot_ops(app.world())));
        }
        Ok(())
    })?;
    Ok(r)
}

/// `bake`, kept on disk by what it's made from: the bundle's bytes (the edit, its parts and the runtime bundled into it)
/// and `build` (the binary that steps it). A hit reads the bake back as it was written, so it's the bake a fresh run
/// makes, bit for bit; an open then costs the bundle and a read instead of every frame stepped (lightspeed F5).
/// raea's caches' bakes/ (raea_io_home::caches; RAEA_BAKE_CACHE moves it; "0" turns it off), newest 64 kept.
pub fn bake_cached(js: &Path, build: u64) -> R<Baked> {
    let dir = match std::env::var("RAEA_BAKE_CACHE") {
        Ok(v) if v == "0" => return bake(js),
        Ok(v) => std::path::PathBuf::from(v),
        Err(_) => match raea_io_home::caches() {
            Some(c) => c.join("bakes"),
            None => return bake(js),
        },
    };
    let bytes = std::fs::read(js).map_err(|e| format!("{}: {e}", js.display()))?;
    let mut key: u64 = 0xcbf2_9ce4_8422_2325 ^ build;
    for b in &bytes {
        key = (key ^ *b as u64).wrapping_mul(0x100_0000_01b3);
    }
    let path = dir.join(format!("{key:016x}.json"));
    if let Ok(text) = std::fs::read(&path) {
        match serde_json::from_slice::<Value>(&text).map_err(|e| e.to_string()).and_then(|v| from_json(v)) {
            Ok(b) => {
                // touched, so pruning keeps what's used
                let _ = std::fs::File::options().append(true).open(&path).and_then(|f| f.set_modified(std::time::SystemTime::now()));
                // RAEA_BAKE_CHECK=1: baked fresh too, and the two compared whole
                if std::env::var_os("RAEA_BAKE_CHECK").is_some() {
                    let same = to_json(&bake(js)?) == to_json(&b);
                    eprintln!("raea bake: the cached bake {} the fresh one", if same { "equals" } else { "DIFFERS FROM" });
                }
                return Ok(b);
            }
            Err(e) => eprintln!("raea bake: the cached bake {} is unreadable ({e}); baking again", path.display()),
        }
    }
    let b = bake(js)?;
    if std::fs::create_dir_all(&dir).is_ok() {
        // written whole, then moved in, so a reader never sees half of one
        let tmp = dir.join(format!(".{key:016x}.{}.tmp", std::process::id()));
        if std::fs::write(&tmp, serde_json::to_vec(&to_json(&b)).unwrap_or_default()).is_ok() && std::fs::rename(&tmp, &path).is_ok() {
            raea_runtime_bundle::prune_cache(&dir, "json", 64);
        } else {
            let _ = std::fs::remove_file(&tmp);
        }
    }
    Ok(b)
}

fn to_json(b: &Baked) -> Value {
    json!({"conf": b.conf, "fps": b.fps, "frames": b.frames, "w": b.w, "h": b.h, "init": b.init, "frame_ops": b.frame_ops,
        "keys": b.keys, "comps": b.comps, "funcs": b.code.funcs, "shells": b.code.shells, "rest": b.code.rest, "has_3d": b.has_3d})
}

fn from_json(mut v: Value) -> R<Baked> {
    macro_rules! take {
        ($k:literal) => {
            serde_json::from_value(v.get_mut($k).map(Value::take).ok_or(concat!("no ", $k))?).map_err(|e| format!("{}: {e}", $k))?
        };
    }
    Ok(Baked {
        conf: take!("conf"),
        fps: take!("fps"),
        frames: take!("frames"),
        w: take!("w"),
        h: take!("h"),
        init: take!("init"),
        frame_ops: take!("frame_ops"),
        keys: take!("keys"),
        comps: take!("comps"),
        code: Code { funcs: take!("funcs"), shells: take!("shells"), rest: take!("rest") },
        has_3d: take!("has_3d"),
    })
}

/// `old` baked again for the bundle at `js` (the edit changed), re-stepping only the frames it must: when every function
/// the edit changed is a component (or inside one) that `old` saw render, the frames where one rendered, each run of them
/// through the keyframe after it; anything else (code outside the functions, a helper no frame recorded, new settings)
/// bakes every frame again. Returns the bake and how many frames were stepped.
pub fn bake_incremental(old: &Baked, js: &Path) -> R<(Baked, i64)> {
    let code = code_of(js)?;
    let full = |why: &str| -> R<(Baked, i64)> {
        if raea_io_trace::stat("RAEA_BAKE_TRACE") {
            eprintln!("raea bake: every frame again ({why})");
        }
        let b = bake(js)?;
        let n = b.frames;
        Ok((b, n))
    };
    if old.comps.len() != old.frames as usize || old.comps.iter().all(|c| c.is_empty()) {
        return full("the old bake recorded no components");
    }
    if code.rest != old.code.rest {
        return full("code outside the functions changed");
    }
    let recorded: std::collections::HashSet<u32> = old.comps.iter().flatten().copied().collect();
    // the components the edit changed: rendered by some frame, their code gone from the new bundle
    let hit: std::collections::HashSet<u32> = recorded.iter().filter(|h| old.code.funcs.contains_key(h) && !code.funcs.contains_key(h)).copied().collect();
    // every function whose own code changed is one of them or inside one
    let now: std::collections::HashSet<u32> = code.shells.iter().map(|s| s.0).collect();
    for (shell, whole) in &old.code.shells {
        if now.contains(shell) {
            continue;
        }
        if !hit.contains(whole) && !old.code.funcs.get(whole).is_some_and(|c| c.iter().any(|p| hit.contains(p))) {
            if raea_io_trace::stat("RAEA_BAKE_TRACE") {
                eprintln!("raea bake: function {whole:x} (inside {:x?}) changed; components changed {:x?}", old.code.funcs.get(whole), hit);
            }
            return full("a changed function isn't a component any frame rendered");
        }
    }
    let frames = old.frames;
    let changed: Vec<bool> = old.comps.iter().map(|c| c.iter().any(|h| hit.contains(h))).collect();
    // the runs: from a changed frame to the keyframe after its last changed frame, that keyframe unchanged (the old
    // bake's tree there is the page after it), or the timeline's end
    let mut runs: Vec<(i64, i64)> = Vec::new();
    let mut f = 0;
    while f < frames {
        if !changed[f as usize] {
            f += 1;
            continue;
        }
        let a = f;
        let mut k = a;
        loop {
            while k + 1 < frames && changed[(k + 1) as usize] {
                k += 1;
            }
            k = ((k / KEY) * KEY + KEY - 1).min(frames - 1);
            if k + 1 < frames && changed[(k + 1) as usize] || changed[k as usize] && k < frames - 1 {
                k += 1;
                continue;
            }
            break;
        }
        runs.push((a, k));
        f = k + 1;
    }
    // their frames split across runtimes as a whole bake's are (lightspeed T3), each piece from a RESET of its own
    // runtime's tree, as a run's first frame is; the later pieces' runtimes start before this one's, so their opening
    // overlaps (RAEA_BAKE_WORKERS, default 4, a piece at least 60 frames)
    let stepped: i64 = runs.iter().map(|(a, k)| k - a + 1).sum();
    let workers = std::env::var("RAEA_BAKE_WORKERS").ok().and_then(|v| v.parse::<i64>().ok()).unwrap_or(4).clamp(1, 16).min((stepped / 60).max(1));
    let per = (stepped + workers - 1) / workers;
    let mut pieces: Vec<(i64, i64)> = Vec::new();
    for &(a, k) in &runs {
        let mut s = a;
        while s <= k {
            let e = (s + per).min(k + 1);
            pieces.push((s, e));
            s = e;
        }
    }
    let later: Vec<std::thread::JoinHandle<R<Range>>> = pieces
        .iter()
        .skip(1)
        .map(|&(a, e)| {
            let js = js.to_path_buf();
            std::thread::Builder::new().name("raea bake".into()).spawn(move || {
                let mut app = runtime(&js);
                raea_runtime_react::scene::settle(&mut app, Some(-1), 20.0)?;
                take_ops(&mut app);
                step_range(&mut app, a, e, true)
            })
        })
        .collect::<Result<_, _>>()
        .map_err(|e| format!("a bake worker: {e}"))?;
    let mut app = runtime(js);
    let conf = raea_runtime_react::scene::settle(&mut app, Some(-1), 20.0)?.ok_or("the composition's settings didn't arrive")?;
    if conf != old.conf {
        return full("the composition's settings changed");
    }
    take_ops(&mut app);
    let mut b = Baked { conf: old.conf.clone(), fps: old.fps, frames, w: old.w, h: old.h, init: old.init.clone(), frame_ops: old.frame_ops.clone(), keys: old.keys.clone(), comps: old.comps.clone(), code, has_3d: old.has_3d };
    let mut results = Vec::with_capacity(pieces.len());
    if let Some(&(a, e)) = pieces.first() {
        results.push(step_range(&mut app, a, e, true)?);
    }
    for t in later {
        results.push(t.join().map_err(|_| "a bake worker panicked".to_string())??);
    }
    // a run's last keyframe stays the old bake's (the frames after the run go on from its tree)
    let run_end = |g: i64| runs.iter().find(|(a, k)| *a <= g && g <= *k).map(|r| r.1).unwrap_or(frames - 1);
    for (&(a, _), r) in pieces.iter().zip(results) {
        for (i, (ops, comps)) in r.frame_ops.into_iter().zip(r.comps).enumerate() {
            b.frame_ops[a as usize + i] = ops;
            b.comps[a as usize + i] = comps;
        }
        b.has_3d |= r.has_3d;
        for (kf, snap) in r.keys {
            let k = run_end(kf);
            if kf < k || k == frames - 1 {
                if let Some(e) = b.keys.iter_mut().find(|(f, _)| *f == kf) {
                    e.1 = snap;
                }
            }
        }
    }
    // back on the old bake's tree (its node ids) for the frames after each run
    for &(_, k) in &runs {
        if k < frames - 1 {
            let key = old.keys.iter().find(|(kf, _)| *kf == k).ok_or("a run ended off a keyframe")?;
            b.frame_ops[k as usize].push(RESET.to_string());
            b.frame_ops[k as usize].push(Value::Array(key.1.clone()).to_string());
        }
    }
    if raea_io_trace::stat("RAEA_BAKE_TRACE") {
        eprintln!("raea bake: {} runs, {stepped} frames on {} runtimes", runs.len(), pieces.len());
    }
    Ok((b, stepped))
}

// raea-runtime-bundle's cache key, tested here: that crate is at its size limit
#[cfg(test)]
mod bundle_key {
    #[test]
    fn a_dot_named_edit_rewritten_gets_another_bundle() {
        // the app's preview wrapper (.raea-view-NAME.tsx) is rewritten at one path, and the folder walk passes dot files over
        let t = std::env::temp_dir().join(format!("raea-bundle-dot-{}", std::process::id()));
        std::fs::create_dir_all(&t).unwrap();
        let o = raea_runtime_bundle::comp_opts(&[]).unwrap();
        let (hidden, plain) = (t.join(".raea-view-body.tsx"), t.join("body.tsx"));
        std::fs::write(&hidden, "export const a = 1\n").unwrap();
        std::fs::write(&plain, "export const a = 1\n").unwrap();
        let (first, other) = (raea_runtime_bundle::bundle_cache(&hidden, &o).unwrap(), raea_runtime_bundle::bundle_cache(&plain, &o).unwrap());
        std::fs::write(&hidden, "export const a = 22\n").unwrap();
        assert_ne!(raea_runtime_bundle::bundle_cache(&hidden, &o).unwrap(), first);
        // a file the walk sees is counted once, as before
        assert_eq!(raea_runtime_bundle::bundle_cache(&plain, &o).unwrap(), other);
        std::fs::remove_dir_all(&t).unwrap();
    }
}
