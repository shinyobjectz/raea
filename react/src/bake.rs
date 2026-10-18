//! A timeline baked: a composition's React commits recorded frame by frame (React stepped through every frame once, as
//! a render steps it), with keyframes of the whole tree every KEY frames. Playback replays them into the same ECS page,
//! Blitz and compositor (lib.rs ReplayPlugin, live.rs), with no JS running: a timeline is a function of its frame, so
//! what React committed for frame f is all frame f needs, and a render and a play of the same bake are the same pixels.

use std::path::Path;

use bevy::prelude::*;
use serde_json::{json, Map, Value};

use crate::{CommitTap, ReactOutbox, ReactPlugin};

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
}

/// A batch that clears the page first: what's under the root goes, then the batches after it build the tree again
/// (frames re-stepped by `bake_incremental` come from another JS runtime, whose node ids aren't the bake's).
pub const RESET: &str = "RESET";

/// A bundle's code as an edit is compared: each function by its code (FNV-1a over the UTF-16 of its source, as
/// Function.prototype.toString gives it and react/js/lite hashes a rendered component) with the codes of the functions
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
    let spans = raea_modules::function_spans(&text).map_err(|e| e.to_string())?;
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
    let mut q = world.query::<&crate::ReactProps>();
    let s = q.iter(world).find(|p| p.0.contains_key("data-raea-comps")).and_then(|p| p.0.get("data-raea-comps").and_then(Value::as_str).map(str::to_string));
    s.map(|s| s.split(',').filter_map(|c| u32::from_str_radix(c, 16).ok()).collect()).unwrap_or_default()
}

/// `tsx`'s composition baked (its bundle at `js`, made by the caller): every frame stepped once, its commits kept.
pub fn bake(js: &Path) -> R<Baked> {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins).add_plugins(ReactPlugin::new(js.to_path_buf()));
    app.insert_resource(CommitTap::default());
    let conf = crate::motion::settle_frame(&mut app, Some(-1), 20.0)?.ok_or("the composition's settings didn't arrive")?;
    let n = |k: &str, d: f64| conf.get(k).and_then(Value::as_f64).unwrap_or(d);
    let (w, h, fps, frames) = (n("width", 1080.0) as u32, n("height", 1920.0) as u32, n("fps", 30.0), n("frames", 1.0).max(1.0) as i64);
    let take = |app: &mut App| std::mem::take(&mut app.world_mut().resource_mut::<CommitTap>().0);
    let init = take(&mut app);
    let mut keys = vec![(-1, crate::snapshot_ops(app.world()))];
    let mut frame_ops = Vec::with_capacity(frames as usize);
    let mut comps = Vec::with_capacity(frames as usize);
    for f in 0..frames {
        // the render's own step: the frame number (not live play's {frame, input}), so the commits are a render's
        app.world().resource::<ReactOutbox>().send(json!({"t": "event", "name": "raea-frame", "value": f}));
        crate::motion::settle_frame(&mut app, Some(f), 20.0)?;
        comps.push(comps_of(app.world_mut()));
        frame_ops.push(take(&mut app));
        if f % KEY == KEY - 1 {
            keys.push((f, crate::snapshot_ops(app.world())));
        }
    }
    Ok(Baked { conf, fps, frames, w, h, init, frame_ops, keys, comps, code: code_of(js)? })
}

/// `old` baked again for the bundle at `js` (the edit changed), re-stepping only the frames it must: when every function
/// the edit changed is a component (or inside one) that `old` saw render, the frames where one rendered, each run of them
/// through the keyframe after it; anything else (code outside the functions, a helper no frame recorded, new settings)
/// bakes every frame again. Returns the bake and how many frames were stepped.
pub fn bake_incremental(old: &Baked, js: &Path) -> R<(Baked, i64)> {
    let code = code_of(js)?;
    let full = |why: &str| -> R<(Baked, i64)> {
        if std::env::var_os("RAEA_BAKE_TRACE").is_some() {
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
            if std::env::var_os("RAEA_BAKE_TRACE").is_some() {
                eprintln!("raea bake: function {whole:x} (inside {:x?}) changed; components changed {:x?}", old.code.funcs.get(whole), hit);
            }
            return full("a changed function isn't a component any frame rendered");
        }
    }
    let frames = old.frames;
    let changed: Vec<bool> = old.comps.iter().map(|c| c.iter().any(|h| hit.contains(h))).collect();
    let mut app = App::new();
    app.add_plugins(MinimalPlugins).add_plugins(ReactPlugin::new(js.to_path_buf()));
    app.insert_resource(CommitTap::default());
    let conf = crate::motion::settle_frame(&mut app, Some(-1), 20.0)?.ok_or("the composition's settings didn't arrive")?;
    if conf != old.conf {
        return full("the composition's settings changed");
    }
    let take = |app: &mut App| std::mem::take(&mut app.world_mut().resource_mut::<CommitTap>().0);
    take(&mut app);
    let mut b = Baked { conf: old.conf.clone(), fps: old.fps, frames, w: old.w, h: old.h, init: old.init.clone(), frame_ops: old.frame_ops.clone(), keys: old.keys.clone(), comps: old.comps.clone(), code };
    let mut stepped = 0;
    let mut f = 0;
    while f < frames {
        if !changed[f as usize] {
            f += 1;
            continue;
        }
        // the run: from here to the keyframe after its last changed frame, that keyframe unchanged (the old bake's tree
        // there is the page after it), or the timeline's end
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
        for g in a..=k {
            app.world().resource::<ReactOutbox>().send(json!({"t": "event", "name": "raea-frame", "value": g}));
            crate::motion::settle_frame(&mut app, Some(g), 20.0)?;
            b.comps[g as usize] = comps_of(app.world_mut());
            let ops = take(&mut app);
            stepped += 1;
            b.frame_ops[g as usize] = if g == a { vec![RESET.to_string(), Value::Array(crate::snapshot_ops(app.world())).to_string()] } else { ops };
            if g % KEY == KEY - 1 && (g < k || k == frames - 1) {
                if let Some(e) = b.keys.iter_mut().find(|(kf, _)| *kf == g) {
                    e.1 = crate::snapshot_ops(app.world());
                }
            }
        }
        // back on the old bake's tree (its node ids) for the frames after the run
        if k < frames - 1 {
            let key = old.keys.iter().find(|(kf, _)| *kf == k).ok_or("a run ended off a keyframe")?;
            b.frame_ops[k as usize].push(RESET.to_string());
            b.frame_ops[k as usize].push(Value::Array(key.1.clone()).to_string());
        }
        f = k + 1;
    }
    Ok((b, stepped))
}
