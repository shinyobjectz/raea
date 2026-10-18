//! `rdb serve`: a resident renderer for an agent's loop. One JSON request per line on stdin, one answer per line on
//! stdout. Engines stay warm per output size and media folder (Bevy, the GPU, the decoders and the pool built once),
//! and an engine reloads only when the rows it's asked to draw change, so a look or a still costs a frame (about
//! 60 ms) rather than a process and an engine (about 400 ms).
//!
//!   {"cmd": "look", "edit": "edits/a.rdb", "at": [0, 90], "out": "previews/a"}   frames as PNGs
//!   {"cmd": "stills", "edit": "edits/a.rdb", "at": [60], "formats": ["1x1"], "out": "exports/a-stills"}
//!   {"cmd": "look", "edit": "edits/a.rdb", "at": [12], "out": "previews/a", "format": "rgba"}   raw RGBA, no PNG
//!   ... "quality": "preview"   no MSAA, no shadow maps (engine::set_preview_quality), for a scrub
//!   {"cmd": "ping"}
//! Each answer: {"ok": true, "files": [...], "ms": N} or {"ok": false, "error": "..."}; a look also says the frame's
//! "size": [w, h] (what an .rgba file's bytes are). An edit is opened once and kept until its file changes (size and
//! modification time), so a look on an unchanged edit costs the frame alone (raea/void's stage asks for one per scrub).

use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::io::{BufRead, Write};
use std::path::{Path, PathBuf};

use serde_json::{json, Value};

use crate::engine::Engine;
use crate::eval::Tup;
use crate::ir::Program;

struct Warm {
    engine: Engine,
    /// what it holds now: a hash of the program's settings and rows
    holds: u64,
}

/// An opened edit, kept while its file is unchanged.
struct Opened {
    stamp: (u64, Option<std::time::SystemTime>),
    prog: std::rc::Rc<Program>,
    rows: std::rc::Rc<Vec<Tup>>,
    key: u64,
}

/// How an edit's path becomes its program and rows: rdb's own `open` (.rdb, .sql), or another language's compiler
/// (a host can pass its own, so its edits get the same warm engines).
pub type Opener = fn(&str) -> Result<(Program, Vec<Tup>), String>;

#[derive(Default)]
pub struct Server {
    opener: Option<Opener>,
    /// Warm engines by output size, media folder and quality (an engine keeps the quality it was made with).
    warm: HashMap<(u32, u32, PathBuf, bool), Warm>,
    /// This request's quality: "preview" turns off MSAA and shadow maps (engine::set_preview_quality).
    preview: bool,
    opened: HashMap<String, Opened>,
    /// The last frame's size, for the answer.
    size: Option<(u32, u32)>,
}

fn rows_key(prog: &Program, rows: &[Tup]) -> u64 {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    format!("{} {} {} {} {:?} {:?} {:?}", prog.fps, prog.width, prog.height, prog.frames, prog.keys, prog.transitions, rows).hash(&mut h);
    for s in &prog.systems {
        s.text.hash(&mut h);
    }
    h.finish()
}

impl Server {
    /// The edit's program and rows, opened again only when its file changed; with their rows_key, computed once.
    fn open(&mut self, edit: &str) -> Result<(std::rc::Rc<Program>, std::rc::Rc<Vec<Tup>>, u64), String> {
        let stamp = std::fs::metadata(edit).map(|m| (m.len(), m.modified().ok())).map_err(|e| format!("{edit}: {e}"))?;
        if let Some(o) = self.opened.get(edit).filter(|o| o.stamp == stamp) {
            return Ok((o.prog.clone(), o.rows.clone(), o.key));
        }
        let (prog, rows) = self.opener.unwrap_or(crate::open)(edit)?;
        let key = rows_key(&prog, &rows);
        let o = Opened { stamp, prog: std::rc::Rc::new(prog), rows: std::rc::Rc::new(rows), key };
        let out = (o.prog.clone(), o.rows.clone(), o.key);
        self.opened.insert(edit.to_string(), o);
        Ok(out)
    }

    /// A frame of (prog, rows) to a PNG, on a warm engine.
    fn frame_png(&mut self, prog: &Program, rows: &[Tup], dir: &Path, f: i64, dest: &Path) -> Result<(), String> {
        let want = rows_key(prog, rows);
        self.frame_to(prog, rows, want, dir, f, dest, false)
    }

    /// A frame to `dest`: a PNG, or (raw) the RGBA bytes as they are, row by row, with no encoding.
    #[allow(clippy::too_many_arguments)]
    fn frame_to(&mut self, prog: &Program, rows: &[Tup], want: u64, dir: &Path, f: i64, dest: &Path, raw: bool) -> Result<(), String> {
        let key = (prog.width, prog.height, dir.to_path_buf(), self.preview);
        let w = match self.warm.get_mut(&key) {
            Some(w) => {
                if w.holds != want {
                    let t = std::time::Instant::now();
                    w.engine.load(prog, rows)?;
                    w.holds = want;
                    if std::env::var_os("RAEA_SERVE_TIMES").is_some() {
                        eprintln!("serve: load {:.1} ms", t.elapsed().as_secs_f64() * 1e3);
                    }
                }
                w
            }
            None => {
                crate::engine::set_preview_quality(self.preview);
                let engine = Engine::new(prog, rows, dir)?;
                self.warm.entry(key).or_insert(Warm { engine, holds: want })
            }
        };
        let t = std::time::Instant::now();
        let px = w.engine.frame(f)?;
        if std::env::var_os("RAEA_SERVE_TIMES").is_some() {
            eprintln!("serve: frame {f} {:.1} ms", t.elapsed().as_secs_f64() * 1e3);
        }
        let (fw, fh) = w.engine.size();
        self.size = Some((fw, fh));
        if let Some(d) = dest.parent() {
            std::fs::create_dir_all(d).map_err(|e| e.to_string())?;
        }
        if raw {
            std::fs::write(dest, w.engine.to_rgba(&px)).map_err(|e| format!("{}: {e}", dest.display()))
        } else {
            crate::save_png(dest, &w.engine.to_rgba(&px), fw, fh)
        }
    }

    pub fn handle(&mut self, req: &Value) -> Result<Vec<PathBuf>, String> {
        self.preview = req["quality"].as_str() == Some("preview");
        let s = |k: &str| req[k].as_str().map(String::from).ok_or(format!("{k}?"));
        let frames = |p: &Program| -> Vec<i64> {
            req["at"].as_array().map(|a| a.iter().filter_map(|v| v.as_i64()).collect()).filter(|v: &Vec<i64>| !v.is_empty()).unwrap_or_else(|| vec![p.frames / 3])
        };
        match req["cmd"].as_str().unwrap_or("") {
            "ping" => Ok(vec![]),
            "look" => {
                let edit = s("edit")?;
                let (prog, rows, want) = self.open(&edit)?;
                let dir = crate::dir_of(&edit);
                let out = PathBuf::from(s("out")?);
                let raw = req["format"].as_str() == Some("rgba");
                let mut made = Vec::new();
                for f in frames(&prog) {
                    let dest = out.join(format!("frame-{f:05}.{}", if raw { "rgba" } else { "png" }));
                    self.frame_to(&prog, &rows, want, &dir, f, &dest, raw)?;
                    made.push(dest);
                }
                Ok(made)
            }
            "stills" => {
                let edit = s("edit")?;
                let (prog, rows, _) = self.open(&edit)?;
                let dir = crate::dir_of(&edit);
                let formats: Vec<crate::stills::Format> = match req["formats"].as_array() {
                    Some(a) => a.iter().filter_map(|v| v.as_str()).map(|n| crate::stills::format(n).ok_or(format!("unknown format {n}"))).collect::<Result<_, _>>()?,
                    None => crate::stills::FORMATS.to_vec(),
                };
                let stem = Path::new(&edit).file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or("edit".into());
                // "best": n picks the frames from the take's ingest (stills::best_frames) when "at" is empty
                let mut at = frames(&prog);
                if let Some(n) = req["best"].as_u64().filter(|_| req["at"].as_array().is_none_or(|a| a.is_empty())) {
                    let picks = crate::stills::best_frames(&prog, &rows, &dir, n as usize)?;
                    if !picks.is_empty() {
                        at = picks.iter().map(|p| p.0).collect();
                    }
                }
                let out = PathBuf::from(s("out")?);
                crate::stills::stills_with(&prog, &rows, &dir, &stem, &at, &formats, &out, &mut |p, r, d, dest| self.frame_png(p, r, d, 0, dest))
            }
            other => Err(format!("unknown cmd {other:?} (look, stills, ping)")),
        }
    }
}

/// The loop: requests on stdin until it closes.
pub fn serve() {
    serve_with(crate::open)
}

/// The loop, opening edits with `opener` (rdb's own `open` unless a host passes another).
pub fn serve_with(opener: Opener) {
    let mut server = Server { opener: Some(opener), ..Server::default() };
    let stdin = std::io::stdin();
    let mut out = std::io::stdout();
    for line in stdin.lock().lines() {
        let Ok(line) = line else { break };
        if line.trim().is_empty() {
            continue;
        }
        let t0 = std::time::Instant::now();
        let answer = match serde_json::from_str::<Value>(&line) {
            Ok(req) => match server.handle(&req) {
                Ok(files) => {
                    let mut a = json!({"ok": true, "files": files, "ms": (t0.elapsed().as_secs_f64() * 1e4).round() / 10.0});
                    if let Some((w, h)) = server.size.take() {
                        a["size"] = json!([w, h]);
                    }
                    a
                }
                Err(e) => json!({"ok": false, "error": e}),
            },
            Err(e) => json!({"ok": false, "error": format!("bad request: {e}")}),
        };
        let _ = writeln!(out, "{answer}");
        let _ = out.flush();
    }
}
