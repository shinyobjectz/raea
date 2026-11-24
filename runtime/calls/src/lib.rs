//! raea's native calls (`__raea_call(name, json) -> json`), on the JS thread: Manifold solids held as handles
//! (`/3d`'s `manifold`), and ECS worlds on Bevy's own ECS (`/ecs`, ecs.rs). TypeScript gets typed objects; the geometry,
//! the storage and the filtering are Rust.
//!
//!   solid.make    {fn, args}            -> {id, expr}        box, rounded_box, sphere, cylinder, cone, torus, text3d
//!   solid.combine {fn, ids}             -> {id, expr}        union, minus, intersect, hull
//!   solid.move    {id, fn, args}        -> {id, expr}        move, rotate, scale
//!   solid.query   {id, what, z?}        -> ...               volume, bounds, triangles, slice (area, contours, polygons)
//!   solid.op      {id, fn, args, other?} -> {id, expr}        mirror, transform, trim, refine(_length/_tolerance), smooth, simplify, minkowski(_diff)
//!   solid.section {id, z?}             -> section             a slice at z, or the silhouette (project)
//!   solid.decompose / solid.compose / solid.hull_points; solid.extrude, revolve, level_set, from_mesh, import, export
//!   section.op    {fn, polygons, others?, args} -> section    2D cross-sections by value: booleans, offset, hull, moves
//!   solid.free    {id}
//!   ecs.new {} -> {world}; ecs.spawn {world, kind, cols} -> {id}; ecs.set {world, id, cols}; ecs.get {world, id};
//!   ecs.despawn {world, id}; ecs.query {world, kind, where: [[col, op, value]]} -> {ids}; ecs.column {world, kind, col};
//!   ecs.rows {world}; ecs.count {world, kind}; ecs.clone {world} -> {world}; ecs.free {world}
//!   print {text}                                              (a headless run's output, to stdout)
//!   blender.post {sid, ops} -> {r, n, err?}                   (raea/blender's batches, to the Blender `raea blender run` started)

use std::cell::RefCell;
use std::collections::HashMap;
use std::io::Write;

use serde_json::{Value, json};

type R<T> = Result<T, String>;

/// ml.* and speech.* (raea-sys-ml, raea-sys-speech), registered with the rest
#[cfg(feature = "ml")]
pub mod ml;
// ECS worlds on Bevy's own ECS (/ecs)
pub mod ecs;
/// raea/blender's batches, to the Blender `raea blender run SCRIPT.ts` started (sdk/blender)
pub mod blender;
mod media;
mod solid;

/// The `raea` command line, for the `cli.raea` native call: the binary sets it (raea-runtime-react can't depend on the layer
/// above it).
pub static CLI: std::sync::OnceLock<fn(&[String]) -> i32> = std::sync::OnceLock::new();

/// The edit's folder for asset.load (useAsset), set as a render or a play opens it.
pub static ASSET_DIR: std::sync::Mutex<Option<std::path::PathBuf>> = std::sync::Mutex::new(None);

/// An asset's file: an http(s) URL fetched into raea's cache, an absolute path, else one found as a model's src is
/// (staticFile's /media/x: the edit's folder's media/, then its assets/)
fn asset_path(p: &str) -> R<std::path::PathBuf> {
    if p.starts_with("http://") || p.starts_with("https://") {
        return raea_io_net::cache::fetch(p);
    }
    let dir = ASSET_DIR.lock().unwrap().clone();
    Ok(match dir {
        Some(d) if !std::path::Path::new(p).exists() => raea_core_scene::media_path(&d, &format!("/{}", p.trim_start_matches('/'))),
        _ => std::path::PathBuf::from(p),
    })
}

/// A composition opened live from `dir` (its bundle `js`): its assets (useAsset) found from its folder, as a render
/// finds them; and a game that says lines has Pocket TTS loaded on its worker now, before the
/// clock starts.
pub fn open_in(dir: &std::path::Path, js: &std::path::Path) {
    *ASSET_DIR.lock().unwrap() = Some(dir.to_path_buf());
    #[cfg(feature = "ml")]
    {
        ml::set_dir(dir);
        if ml::bundle_speaks(js) {
            ml::warm();
        }
    }
    #[cfg(not(feature = "ml"))]
    let _ = js;
}

/// A render's composition folder, where its models' calls read from (ml::set_dir; nothing without the ml feature).
pub fn rendering_in(dir: &std::path::Path) {
    #[cfg(feature = "ml")]
    ml::set_dir(dir);
    #[cfg(not(feature = "ml"))]
    let _ = dir;
}

/// A composition's lines (speech) made in the background, as a render's are when no live play waits for its own
/// (nothing without the ml feature).
pub fn speech_in_background() {
    #[cfg(feature = "ml")]
    raea_sys_speech::speech::set_background(true);
}

/// The open composition's folder (where "/media/..." resolves), set with ml::set_dir.
pub fn composition_dir() -> Option<std::path::PathBuf> {
    #[cfg(feature = "ml")]
    return ml::dir();
    #[cfg(not(feature = "ml"))]
    None
}

/// A decoded picture held natively: NV12 at its size (what mediabunny's samples point to by handle).
struct Picture {
    width: u32,
    height: u32,
    nv12: Vec<u8>,
}

#[derive(Default)]
struct State {
    solids: HashMap<u64, solid::Solid>,
    pictures: HashMap<u64, Picture>,
    #[cfg(target_vendor = "apple")]
    vdecs: HashMap<u64, raea_io_media::apple::packets::PacketDecoder>,
    #[cfg(target_vendor = "apple")]
    vencs: HashMap<u64, raea_io_media::apple::packets::PacketEncoder>,
    adecs: HashMap<u64, raea_io_media::soft::packets::AudioPacketDecoder>,
    #[cfg(target_vendor = "apple")]
    aencs: HashMap<u64, raea_io_media::apple::aenc::AacEncoder>,
    next: u64,
}


/// A headless run's end (`done {code}`): -1 while it runs.
pub static DONE: std::sync::atomic::AtomicI32 = std::sync::atomic::AtomicI32::new(-1);

/// Whether print also writes to stdout (the CLI; not an app).
pub static ECHO: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(true);

/// Whether a script's console goes to the terminal, while ECHO is on: a headless program's (`raea run`, `raea test`,
/// `raea blender run`), whose App has no log to take it, so every console line was dropped (#168). raea-runtime-react's
/// `__raea_log` reads it.
pub static CONSOLE: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// What a headless run printed (an app shows it; the CLI also writes it to stdout as it comes).
pub static PRINTED: std::sync::Mutex<String> = std::sync::Mutex::new(String::new());

fn f(v: &Value, k: &str) -> R<f64> {
    v.get(k).and_then(Value::as_f64).ok_or_else(|| format!("{k}: a number"))
}

fn u(v: &Value, k: &str) -> R<u64> {
    v.get(k).and_then(Value::as_u64).ok_or_else(|| format!("{k}: an id"))
}

fn s<'a>(v: &'a Value, k: &str) -> R<&'a str> {
    v.get(k).and_then(Value::as_str).ok_or_else(|| format!("{k}: a string"))
}

fn args(v: &Value) -> Vec<Value> {
    v.get("args").and_then(Value::as_array).cloned().unwrap_or_default()
}




thread_local! {
    static STATE: RefCell<State> = RefCell::new(State::default());
}

pub fn call(name: &str, v: &Value) -> R<Value> {
    STATE.with(|st| {
        let mut st = st.borrow_mut();
        match name {
            "done" => {
                DONE.store(v.get("code").and_then(Value::as_i64).unwrap_or(0) as i32, std::sync::atomic::Ordering::SeqCst);
                Ok(Value::Null)
            }
            "print" => {
                let text = s(v, "text")?;
                if let Ok(mut p) = PRINTED.lock() {
                    p.push_str(text);
                    p.push('\n');
                }
                // `err`: a line for stderr (a script's throw, #168), where its console goes to the terminal
                if ECHO.load(std::sync::atomic::Ordering::Relaxed) {
                    if v.get("err").and_then(Value::as_bool) == Some(true) && CONSOLE.load(std::sync::atomic::Ordering::Relaxed) {
                        let _ = writeln!(std::io::stderr().lock(), "{text}");
                    } else {
                        let mut o = std::io::stdout().lock();
                        let _ = writeln!(o, "{text}");
                        let _ = o.flush();
                    }
                }
                Ok(Value::Null)
            }
            "blender.post" => blender::post(v),
            n if n.starts_with("ecs.") => ecs::call(n, v),
            // placement by relations (raea-core-place)
            "place.solve" => raea_core_place::solve_json(v),
            // a frame of a character taking a thing (raea-sys-grab, docs/hands.feature HG5)
            "character.take" => raea_sys_grab::take_json(v),
            "character.fit" => raea_sys_grab::fit_json(v),
            "character.hand" => raea_sys_grab::hand_json(v),
            "character.soma" => raea_sys_grab::soma_json(v),
            "character.turns" => raea_sys_grab::turns_json(v),
            "character.twists" => raea_sys_grab::twists_json(v),
            "fonts.families" => Ok(families().into()),
            "fonts.google" => {
                let family = s(v, "family")?;
                raea_sys_fonts::google::want(&family);
                // fetched now, not at the first layout that names it: a component measures its text in an effect
                // (canvas measureText, svg text), which runs before that layout and would get the fallback
                raea_sys_fonts::google::load_now(&family);
                Ok(json!({ "family": family }))
            }
            "fonts.atlases" => font_atlases(v),
            n if n.starts_with("solid.") || n.starts_with("section.") => solid::call(&mut st, n, v),
            n => media::call(&mut st, n, v),
        }
    })
}

/// The calls above in raea-native's registry, with their TypeScript types (what `raea types` writes into
/// sdk/raea/src/native-calls.ts). Idempotent; every host running raea calls it before its first bundle.
pub fn register() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        raea_runtime_wear::register(raea_runtime_wear::Place { dir: composition_dir, media: raea_core_scene::media_path });
        const BOUNDS: &str = "{ min: [number, number, number]; max: [number, number, number] }";
        const SOLID: &str = "{ id: number; expr: string }";
        const SECTION: &str = "{ polygons: [number, number][][]; area: number; contours: number; bounds: { min: [number, number]; max: [number, number] } }";
        let calls: &[(&str, &str, &str, &str)] = &[
            ("done", "{ code: number }", "null", "a headless run ends with this exit code"),
            ("print", "{ text: string; err?: boolean }", "null", "one line of a headless run's output (err: for stderr, on a terminal)"),
            ("blender.post", "{ sid: string; ops: unknown[] }", "{ r: Record<string, unknown>; n: number; err?: { i: number; type: string; msg: string; trace: string } }", "a batch of raea/blender's operations run by the Blender `raea blender run SCRIPT.ts` started (sdk/blender)"),
            ("solid.make", "{ fn: \"box\" | \"rounded_box\" | \"sphere\" | \"cylinder\" | \"cone\" | \"torus\" | \"text3d\"; args: (number | string)[] }", SOLID, "a Manifold solid from a primitive"),
            ("solid.expr", "{ expr: string }", SOLID, "a Manifold solid from an expression (raea-render's mesh shape language)"),
            ("solid.combine", "{ fn: \"union\" | \"minus\" | \"intersect\" | \"hull\"; ids: number[] }", SOLID, "a boolean or hull of solids"),
            ("solid.move", "{ id: number; fn: \"move\" | \"rotate\" | \"scale\"; args: number[] }", SOLID, "a solid translated, turned (degrees) or scaled"),
            ("solid.query", "{ id: number; what: \"volume\" | \"bounds\" | \"triangles\" | \"empty\" | \"slice\" | \"area\" | \"genus\" | \"min_gap\" | \"ray\"; z?: number; other?: number; args?: number[] }", &format!("number | boolean | {BOUNDS} | {{ area: number; contours: number; polygons: [number, number][][] }} | {{ distance: number; position: [number, number, number]; normal: [number, number, number] }}[]"), "a solid's volume, bounds, triangle count, emptiness, cross-section at z, area, genus, gap to another (searching z far) or ray hits (args from, to)"),
            ("solid.op", "{ id: number; fn: \"mirror\" | \"transform\" | \"trim\" | \"refine\" | \"refine_length\" | \"refine_tolerance\" | \"smooth\" | \"simplify\" | \"minkowski\" | \"minkowski_diff\"; args?: number[]; other?: number }", SOLID, "a solid mirrored, transformed (3x4, by columns), trimmed by a plane, refined (by count, edge length or tolerance), smoothed, simplified, or Minkowski-summed/differenced with another"),
            ("solid.extrude", "{ polygons: [number, number][][]; args: number[] }", SOLID, "a cross-section extruded: args height, divisions, twist degrees, top scale x, y"),
            ("solid.revolve", "{ polygons: [number, number][][]; args?: number[] }", SOLID, "a cross-section revolved about y: args degrees, segments"),
            ("solid.mesh", "{ id: number }", "{ verts: number[]; tris: number[] }", "a solid's triangles (positions x y z, indices by threes)"),
            ("solid.from_mesh", "{ verts: number[]; tris: number[] }", SOLID, "a solid from closed triangles"),
            ("solid.level_set", "{ grid: number[]; dims: [number, number, number]; min: [number, number, number]; max: [number, number, number]; level?: number; edge?: number }", SOLID, "the surface of an SDF sampled on a grid (negative inside)"),
            ("solid.export", "{ id: number; path: string; format?: \"obj\" | \"glb\" }", "{ bytes: number }", "a solid written as OBJ or glTF binary"),
            ("solid.import", "{ path: string }", SOLID, "a solid read from OBJ or glTF binary"),
            ("solid.section", "{ id: number; z?: number }", SECTION, "a solid's cross-section at z, or with no z its silhouette on the x-y plane (project)"),
            ("solid.decompose", "{ id: number }", &format!("{SOLID}[]"), "a solid's disconnected pieces"),
            ("solid.compose", "{ ids: number[] }", SOLID, "solids that don't overlap held as one"),
            ("solid.hull_points", "{ points: [number, number, number][] }", SOLID, "the convex hull of points"),
            ("section.op", "{ fn: \"make\" | \"circle\" | \"square\" | \"hull_points\" | \"union\" | \"minus\" | \"intersect\" | \"hull\" | \"minkowski\" | \"offset\" | \"simplify\" | \"translate\" | \"rotate\" | \"scale\" | \"mirror\" | \"decompose\"; polygons?: [number, number][][]; others?: [number, number][][][]; args?: number[] }", &format!("{SECTION} | {SECTION}[]"), "a 2D cross-section made, combined, offset or moved (Manifold's CrossSection), held by value"),
            ("solid.free", "{ id: number }", "null", "drop a solid"),
            ("place.solve", "{ objects: { id: string; size: [number, number, number]; at?: [number, number, number]; yaw?: number; fixed?: boolean }[]; rules: Record<string, string | number>[]; room?: { min: [number, number]; max: [number, number] } }", "{ objects: { id: string; at: [number, number, number]; yaw: number; size: [number, number, number] }[]; found: { part: string; check: string; level: string; says: string }[]; errors: number }", "boxes placed by relations (on, against, beside, near, apart, facing, align, clear), what can't hold found"),
            ("character.take", raea_sys_grab::TAKE_ARGS, raea_sys_grab::TAKE_ANSWER, "a frame of a character taking a thing with its hands (reach, grab, lift, carry, put down or throw): the pose to draw and where the thing is, with how far a wrist missed (metres) and how far the body was turned from its own motion (degrees)"),
            ("character.fit", raea_sys_grab::FIT_ARGS, raea_sys_grab::FIT_ANSWER, "a pose fitted to points of the body seen somewhere (joints or marks on them, from a capture, a tracker or another rig), and their root-mean-square miss, metres"),
            ("character.hand", raea_sys_grab::HAND_ARGS, raea_sys_grab::HAND_ANSWER, "a hand shaped by a few numbers of a model of real hand poses (none: the average relaxed hand; the first opens it, positive, and closes it), on a pose; and that hand read back: its numbers and how odd it is (0: the average hand)"),
            ("character.soma", raea_sys_grab::SOMA_ARGS, raea_sys_grab::SOMA_ANSWER, "the pose that has a body's bones pointing as a SOMA body's do (SOMA: the 78-joint skeleton of NVIDIA's motion models and datasets), from SOMA's joint names and where each joint is"),
            ("character.turns", raea_sys_grab::TURNS_ARGS, raea_sys_grab::TURNS_ANSWER, "a SOMA body's pose from a motion given as turns (each named joint's world turn from SOMA's T-pose, as Kimodo writes them, and where the hips are), with the body's twist joints set"),
            ("character.twists", raea_sys_grab::TWISTS_ARGS, raea_sys_grab::TURNS_ANSWER, "a SOMA body's pose with its twist joints set from its limbs' turns: for a pose from a clip or a solver, which turn the named joints only"),
            ("fonts.families", "{}", "string[]", "the system's font families by name, sorted, as fontique (bevy_text's FontCx) lists them"),
            ("fonts.google", "{ family: string }", "{ family: string }", "a Google Fonts family the page loads (its Latin faces, every weight and style, fetched once through raea's URL cache), as a stylesheet link loads it in Chrome (raea/text loadFont)"),
            ("fonts.atlases", "{ font: string; size: number; text: string }", "{ width: number; height: number; src: string }[]", "bevy_text's glyph atlases for a text in a font file at a size px, each page a PNG data: URL, as Bevy's FontAtlasSet holds them"),
            ("media.live", "{}", "{ vdecs: number; vencs: number; adecs: number; frames: number; solids: number; worlds: number }", "how many native coders, frames, solids and worlds are alive (for leak checks)"),
            ("media.size", "{ path: string }", "number", "a file's size in bytes"),
            ("media.read", "{ path: string; start: number; end: number }", "string", "bytes start..end of a file, base64"),
            ("media.write", "{ path: string; data: string; truncate?: boolean; at?: number }", "null", "base64 bytes written to a file"),
            ("media.transcode", "{ path: string; out: string; from?: number; to?: number; height?: number; kbps?: number }", "{ frames: number; width: number; height: number; fps: number; kbps: number; bytes: number }", "trim, scale and encode with sound to MP4, natively end to end"),
            ("vdec.new", "{ codec: string; description: string }", "{ id: number }", "a VideoToolbox decoder for avc or hevc"),
            ("vdec.decode", "{ id: number; data: string; pts: number; dur: number }", "{ frame: number; pts: number; dur: number; width: number; height: number }[]", "a packet decoded; the frames it completes"),
            ("vdec.flush", "{ id: number }", "{ frame: number; pts: number; dur: number; width: number; height: number }[]", "the frames still held"),
            ("vdec.free", "{ id: number }", "null", "drop a decoder"),
            ("venc.new", "{ width: number; height: number; fps: number; kbps: number }", "{ id: number }", "a VideoToolbox H.264 encoder"),
            ("venc.encode", "{ id: number; frame: number; pts: number; dur: number }", "{ packets: { data: string; key: boolean; pts: number; dur: number }[]; description: string | null }", "an engine frame encoded"),
            ("venc.finish", "{ id: number }", "{ packets: { data: string; key: boolean; pts: number; dur: number }[]; description: string | null }", "the encoder flushed"),
            ("frame.info", "{ frame: number }", "{ width: number; height: number }", "an engine frame's size"),
            ("frame.png", "{ frame: number; path: string }", "null", "an engine frame written as PNG"),
            ("frame.scale", "{ frame: number; width: number; height: number }", "{ frame: number; width: number; height: number }", "an engine frame scaled (a new frame)"),
            ("frame.transform", "{ frame: number; rotation?: number; flip?: boolean; crop?: { left: number; top: number; width: number; height: number }; width: number; height: number; fit?: \"fill\" | \"contain\" | \"cover\" }", "{ frame: number; width: number; height: number }", "an engine frame rotated, flipped, cropped and resized (a new frame): mediabunny's transformations, natively"),
            ("frame.free", "{ frame: number }", "null", "drop an engine frame"),
            ("adec.new", "{ codec: string; description: string; rate: number; channels: number }", "{ id: number }", "a Symphonia decoder for aac, mp3 or flac"),
            ("adec.decode", "{ id: number; data: string }", "{ rate: number; channels: number; frames: number; data: string }", "a packet decoded to planar f32, base64"),
            ("venc.codecs", "{}", "{ hardware: string[]; avc: boolean; hevc: boolean; av1: boolean; vp9: boolean }", "the video codecs this machine encodes in hardware (VideoToolbox's list)"),
            ("aenc.new", "{ rate: number; channels: number; kbps?: number }", "{ id: number; description: string; priming: number }", "an AAC-LC encoder (AudioToolbox): its AudioSpecificConfig (base64) and its priming in frames"),
            ("aenc.encode", "{ id: number; data: string; channels: number }", "{ packets: { data: string; pts: number; dur: number }[] }", "planar f32 samples (base64, channel after channel) encoded; the packets they complete"),
            ("aenc.finish", "{ id: number }", "{ packets: { data: string; pts: number; dur: number }[] }", "the encoder flushed"),
            ("aenc.free", "{ id: number }", "null", "drop an encoder"),
            ("adec.free", "{ id: number }", "null", "drop a decoder"),
        ];
        // raea's command line, in this process (the engine's half of the shell's `raea`): run in `cwd` (a real folder)
        // with its output captured
        for (tool, doc) in [("raea", "raea's command line (render, play, guide, bundle, types; the media verbs: raea media, raea ffmpeg)")] {
            raea_native::register(
                &format!("cli.{tool}"),
                "{ args: string[]; cwd: string }",
                "{ exit: number; stdout: string; stderr: string }",
                &format!("{doc}, run in process in cwd"),
                move |v| {
                    let args: Vec<String> = v.get("args").and_then(Value::as_array).map(|a| a.iter().filter_map(|x| x.as_str().map(str::to_string)).collect()).unwrap_or_default();
                    let cwd = std::path::PathBuf::from(s(v, "cwd")?);
                    let cli = *CLI.get().ok_or("raea's command line isn't in this process")?;
                    #[cfg(unix)]
                    {
                        let (exit, stdout, stderr) = raea_native::capture(&cwd, move || cli(&args))?;
                        Ok(json!({"exit": exit, "stdout": stdout, "stderr": stderr}))
                    }
                    #[cfg(not(unix))]
                    {
                        let _ = cwd;
                        Ok(json!({"exit": cli(&args), "stdout": "", "stderr": ""}))
                    }
                },
            );
        }
        // the run's stat channels, and its timings and counters so far, from TypeScript (docs/nobuild.feature NB2a)
        raea_native::register("stats.on", "{ channels: string[] }", "{ chosen: string[] }", "stat channels on for the rest of the run: a RAEA_ variable's name without RAEA_, in lower case (bevy_passes, bake_trace), or all; at a composition's top level, before the Engine opens (docs/nobuild.feature NB2a)", |v| {
            raea_io_trace::choose_stats(&v["channels"].as_array().into_iter().flatten().filter_map(|n| n.as_str().map(str::to_string)).collect::<Vec<_>>());
            Ok(json!({"chosen": raea_io_trace::chosen_stats()}))
        });
        raea_native::register("stats.read", "{}", "{ chosen: string[]; passes: Record<string, { n: number; mean_ms: number; max_ms: number }>; counters: Record<string, number> }", "the run so far: its stat channels, each timed pass's runs and mean and worst ms, each counter (docs/nobuild.feature NB2a)", |_| Ok(raea_io_trace::stats()));
        // an asset loaded now, before the frame that names it uses it, as the Engine loads a handle's (S2b useAsset)
        raea_native::register("asset.load", "{ path: string }", "{ path: string; bytes: number; width?: number; height?: number }", "an asset by its path (absolute, or from the edit's folder), loaded now: the file found and read, a picture's size; an error when it isn't there (S2b useAsset)", |v| {
            let p = s(v, "path")?;
            let path = asset_path(&p)?;
            let bytes = std::fs::metadata(&path).map_err(|e| format!("asset {p}: {e}"))?.len();
            let mut out = json!({"path": path.to_string_lossy(), "bytes": bytes});
            if let Ok((w, h)) = image::image_dimensions(&path) {
                out["width"] = json!(w);
                out["height"] = json!(h);
            }
            Ok(out)
        });
        // a file's bytes read now, for a loader that parses them in JS (three's FileLoader under R3F: useGLTF)
        raea_native::register("asset.read", "{ path: string }", "{ path: string; base64: string }", "a file's bytes (absolute, from the edit's folder, or an http(s) URL through raea's cache), read now, as base64 (R3F's loaders, docs/r3f.feature R3F13)", |v| {
            use base64::Engine as _;
            let p = s(v, "path")?;
            let path = asset_path(&p)?;
            let bytes = std::fs::read(&path).map_err(|e| format!("asset {p}: {e}"))?;
            Ok(json!({"path": path.to_string_lossy(), "base64": base64::engine::general_purpose::STANDARD.encode(bytes)}))
        });
        for (name, args, answer, doc) in calls.iter().copied().chain(ecs::CALLS.iter().copied()) {
            let n = name.to_string();
            raea_native::register(name, args, answer, doc, move |v| call(&n, v));
        }
        #[cfg(feature = "ml")]
        ml::register();
        // bevy.world.send, bevy.world.answer: a live game's queued world calls (engine/bevy-world, drained by raea-render)
        raea_runtime_plugins::register_world_natives();
    });
}

/// The system's font families, sorted and each once: Bevy's `FontCx` collection's `family_names()` as its
/// ui/text/system_fonts example lists them, from a collection of the system's faces alone (none of raea's).
fn families() -> Vec<String> {
    let mut cx = parley::FontContext::new();
    let mut out: Vec<String> = cx.collection.family_names().map(ToOwned::to_owned).collect();
    out.sort_unstable();
    out.dedup();
    out
}

/// bevy_text's atlas pages for `text` in the font file `font` (an asset path: staticFile's /media/x) at `size` px
/// (raea-sys-fonts atlas.rs), each a PNG data: URL.
fn font_atlases(v: &Value) -> R<Value> {
    use base64::Engine as _;
    let bytes = std::fs::read(asset_path(s(v, "font")?)?).map_err(|e| format!("fonts.atlases: {e}"))?;
    let size = v.get("size").and_then(Value::as_f64).ok_or("fonts.atlases: size in px")? as f32;
    let pages = raea_sys_fonts::atlas::atlases(bytes, size, s(v, "text")?)?;
    let mut out = Vec::new();
    for (w, h, rgba) in pages {
        let img = image::RgbaImage::from_raw(w, h, rgba).ok_or("an atlas page's pixels don't fill it")?;
        let mut png = std::io::Cursor::new(Vec::new());
        img.write_to(&mut png, image::ImageFormat::Png).map_err(|e| e.to_string())?;
        let src = format!("data:image/png;base64,{}", base64::engine::general_purpose::STANDARD.encode(png.into_inner()));
        out.push(serde_json::json!({ "width": w, "height": h, "src": src }));
    }
    Ok(out.into())
}
