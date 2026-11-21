//! `raea notes check` (docs/direction.feature DR6): each ask's check run as a rule on the work as it stands, through
//! `raea check`'s run (apps/direction, which the CLI lends). An ask's check must fail first: one that passes on its
//! first run is refused, as it isn't measuring the ask, and the work may change only once it has failed. What each run
//! found is kept (state.rs); `raea check` keeps it too, when it runs the notes' checks beside its rules. An ask the
//! person signs gets its clip cut here, from its composition's newest render.

use std::path::{Path, PathBuf};

use raea_core_facts::{Finding, Level, record};
/// A rule as `raea check` runs one: an ask's is one, run through the CLI's Runner.
pub use raea_core_facts::Rule;

use crate::state::{self, Clip, Failed, Run, State};
use crate::{Eval, Kept, R, place, quoted, read};

/// One composition's run: its file from the project (`#ID` after it where the file registers several), what the
/// rules found and which waited and for what, its newest render, its shots (name, first frame, end) and its rate.
pub struct Ran {
    pub of: String,
    pub found: Vec<Finding>,
    pub waits: Vec<(String, String)>,
    pub render: Option<Rendered>,
    pub shots: Vec<(String, i64, Option<i64>)>,
    pub fps: f64,
}

/// A composition's newest render: its file, whether the composition changed after it, and how many frames it drew.
pub struct Rendered {
    pub out: PathBuf,
    pub stale: bool,
    pub frames: i64,
}

/// Runs rules on a project's compositions as `raea check` runs its own (taking its --composition, --render and
/// --no-render), every frame read when `whole` (a clip's shot is found in them).
pub type Runner = fn(&Path, &[Rule], &[String], bool) -> R<Vec<Ran>>;

/// An ask's rule's `by`, which its findings carry, so two asks that run one check are told apart.
pub fn by(id: &str) -> String {
    format!("note {id}")
}

/// Whether `of`, the composition an ask names (its file from the project, `#ID` after it where the file registers
/// several), is the one a run names `named` (its file, and `#ID` when it has one): a file alone is any composition
/// in it.
pub fn is_of(of: &str, named: &str) -> bool {
    let (file, id) = named.split_once('#').map_or((named, None), |(f, i)| (f, Some(i)));
    match of.split_once('#') {
        Some((f, i)) => f == file && id == Some(i),
        None => of == file,
    }
}

/// What ask `id`'s check found in a run.
fn found_by<'a>(ran: &'a [Ran], id: &str) -> impl Iterator<Item = &'a Finding> {
    let by = by(id);
    ran.iter().flat_map(|r| &r.found).filter(move |f| f.by == by)
}

/// Each note's file number with the note, for those that read.
fn numbered<'a>(project: &Path, notes: &'a [Kept]) -> Vec<(u32, &'a Kept, &'a raea_core_facts::Note)> {
    notes.iter().filter_map(|k| Some((place(project, &project.join(&k.file))?.1, k, k.note.as_ref().ok()?))).collect()
}

/// The notes' asks that have a check, each as a rule: the ask's id (n3.2) and words, its check (a .ts check's path
/// from the note's folder, as a taste line's is from its file's), its values, `by` the ask; and each note that can't be
/// read, as a finding.
pub fn rules(project: &Path, notes: &[Kept]) -> (Vec<Rule>, Vec<Finding>) {
    let bad = notes.iter().filter_map(|k| k.note.as_ref().err().map(|e| Finding::new("note", Level::Error, format!("raea can't read it: {e}"), "raea notes").about(k.file.display().to_string(), None))).collect();
    let mut out = Vec::new();
    for (number, k, n) in numbered(project, notes) {
        for (i, a) in n.asks.iter().enumerate() {
            let Some(c) = &a.check else { continue };
            let check = if c.ends_with(".ts") { project.join(k.file.parent().unwrap_or(Path::new(""))).join(c).display().to_string() } else { c.clone() };
            let id = state::id(number, i + 1);
            out.push(Rule { by: by(&id), id, text: a.text.clone(), check, values: a.values.clone().unwrap_or_default(), level: None, of: a.of.clone() });
        }
    }
    (out, bad)
}

/// What a run of the notes' rules says of each ask whose rule was among `rules` (`--only` runs one), kept in `s`: a
/// line for each, and whether one was refused (its first run passed).
pub fn settle(project: &Path, notes: &[Kept], rules: &[Rule], ran: &[Ran], stamp: &str, s: &mut State) -> (Vec<String>, bool) {
    let commit = record::commit(project, ".");
    let (mut out, mut refused) = (Vec::new(), false);
    for (number, k, n) in numbered(project, notes) {
        for (i, a) in n.asks.iter().enumerate() {
            let id = state::id(number, i + 1);
            let Some(check) = a.check.as_ref().filter(|_| rules.iter().any(|r| r.id == id)) else { continue };
            let found = found_by(ran, &id).count();
            let waits = ran.iter().flat_map(|r| &r.waits).find(|(r, _)| *r == id).map(|(_, w)| w.clone());
            let run = Run { at: stamp.into(), commit: commit.clone(), found, waits: waits.clone().filter(|_| found == 0) };
            let kept = s.entry(id.clone()).or_default();
            let first = kept.failed.as_ref().is_none_or(|f| !state::same_check(a, f));
            let head = format!("{id} {}: {}", k.file.display(), quoted(&a.text, 70));
            out.push(match (&run.waits, found, first) {
                (Some(w), _, _) => format!("{head}: waits for {w}"),
                (None, 0, true) => {
                    refused = true;
                    kept.refused = Some(run.clone());
                    format!("{head}: refused: its check passes on the work as it stands, so it isn't measuring the ask; write one that fails now, before the work changes")
                }
                (None, 0, false) => format!("{head}: passing"),
                (None, n, true) => {
                    kept.failed = Some(Failed { check: check.clone(), values: a.values.clone().unwrap_or_default(), run: run.clone() });
                    kept.refused = None;
                    format!("{head}: fails on the work as it stands ({n} finding{}): its check measures the ask, and the work may change now", if n == 1 { "" } else { "s" })
                }
                (None, n, false) => format!("{head}: failing ({n} finding{})", if n == 1 { "" } else { "s" }),
            });
            kept.last = Some(run);
        }
    }
    (out, refused)
}

/// A file's SHA-256.
fn sha256(p: &Path) -> R<String> {
    use sha2::{Digest, Sha256};
    let mut f = std::fs::File::open(p).map_err(|e| format!("{}: {e}", p.display()))?;
    let mut d = Sha256::new();
    std::io::copy(&mut f, &mut d).map_err(|e| format!("{}: {e}", p.display()))?;
    Ok(format!("{:x}", d.finalize()))
}

/// Frames `a` to `b` (the frame after its last) of `out` at `fps`, cut to `to` with ffmpeg.
fn cut(out: &Path, [a, b]: [i64; 2], fps: f64, to: &Path) -> R<()> {
    if let Some(d) = to.parent() {
        std::fs::create_dir_all(d).map_err(|e| format!("{}: {e}", d.display()))?;
    }
    let (from, len) = (format!("{:.4}", a as f64 / fps), format!("{:.4}", (b - a) as f64 / fps));
    let args = ["-hide_banner", "-loglevel", "error", "-y", "-ss", &from, "-i"];
    let h264 = raea_io_ffmpeg::h264().ok_or("ffmpeg here has no H.264 encoder (libx264, Windows' h264_mf or VideoToolbox's) to cut the clip with")?;
    let o = std::process::Command::new("ffmpeg").args(args).arg(out).args(["-t", &len]).args(h264).args(["-c:a", "aac"]).arg(to).output().map_err(|e| format!("ffmpeg: {e}"))?;
    if !o.status.success() {
        return Err(format!("ffmpeg: {}", String::from_utf8_lossy(&o.stderr).trim()));
    }
    Ok(())
}

/// Each ask the person signs, its clip cut from its composition's newest render (again only when the render or the
/// frames changed), kept in `s`: a line for each.
pub fn clips(project: &Path, notes: &[Kept], ran: &[Ran], s: &mut State) -> Vec<String> {
    let mut out = Vec::new();
    for (number, k, n) in numbered(project, notes) {
        for (i, a) in n.asks.iter().enumerate() {
            let Some(p) = a.proof.as_ref().filter(|p| p.sign) else { continue };
            let id = state::id(number, i + 1);
            let head = format!("{id} {}: its clip", k.file.display());
            let comp = match &a.of {
                Some(of) => ran.iter().find(|r| is_of(of, &r.of)),
                None if ran.len() == 1 => ran.first(),
                None => {
                    out.push(format!("{head}: which composition? the project has {}: name one in the ask's `of`", ran.iter().map(|r| r.of.as_str()).collect::<Vec<_>>().join(", ")));
                    continue;
                }
            };
            let Some(r) = comp else {
                out.push(format!("{head}: no composition {} was checked", a.of.as_deref().unwrap_or("")));
                continue;
            };
            let Some(render) = &r.render else {
                out.push(format!("{head} waits for a render of {}", r.of));
                continue;
            };
            // a render the composition changed after isn't the work: nothing is cut from it, and the clip kept from it
            // (what the person signed, if they did) is marked so, until it's rendered again
            if render.stale {
                if let Some(c) = s.get_mut(&id).and_then(|k| k.clip.as_mut()) {
                    c.stale = true;
                }
                out.push(format!("{head} waits for a render of {} as it stands: it changed after its newest render", r.of));
                continue;
            }
            let frames = match (p.frames, &p.shot) {
                (Some(f), _) => f,
                (None, Some(shot)) => match r.shots.iter().find(|(name, ..)| name == shot) {
                    Some((_, from, to)) => [*from, to.unwrap_or(render.frames)],
                    None => {
                        out.push(format!("{head}: {} has no shot named {shot}", r.of));
                        continue;
                    }
                },
                (None, None) => [0, render.frames],
            };
            let hash = match sha256(&render.out) {
                Ok(h) => h,
                Err(e) => {
                    out.push(format!("{head}: {e}"));
                    continue;
                }
            };
            let kept = s.entry(id.clone()).or_default();
            let file = project.join(".raea/notes-clips").join(format!("{id}.mp4"));
            let same = kept.clip.as_ref().is_some_and(|c| c.render == hash && c.frames == frames && c.file.is_file());
            if !same {
                if let Err(e) = cut(&render.out, frames, r.fps, &file) {
                    out.push(format!("{head}: {e}"));
                    continue;
                }
            }
            kept.clip = Some(Clip { file: file.clone(), of: r.of.clone(), render: hash, frames, stale: false });
            out.push(format!("{head}, frames {}-{} of {}, {} from its newest render", frames[0], frames[1] - 1, r.of, if same { "kept" } else { "cut" }));
        }
    }
    out
}

/// `raea notes check`: each ask's check on the work as it stands, and each clip to sign cut; 1 when an ask was
/// refused or a note can't be read.
pub(crate) fn check(project: &Path, args: &[String], eval: Eval, run: Runner) -> R<(String, i32)> {
    let notes = read(project, eval);
    let (rules, bad) = rules(project, &notes);
    let signs = notes.iter().filter_map(|k| k.note.as_ref().ok()).flat_map(|n| &n.asks).any(|a| a.proof.as_ref().is_some_and(|p| p.sign));
    let mut out: Vec<String> = bad.iter().map(Finding::line).collect();
    if rules.is_empty() && !signs {
        out.push("no ask has a check or a clip to sign yet: restate the notes' asks with one each (raea notes ls)".into());
        return Ok((out.join("\n"), i32::from(!bad.is_empty())));
    }
    let ran = run(project, &rules, args, signs)?;
    let stamp = raea_io_trace::utc(std::time::SystemTime::now()).1;
    let mut s = state::read(project);
    let (lines, refused) = settle(project, &notes, &rules, &ran, &stamp, &mut s);
    // each ask's line, then what its check found
    for l in lines {
        let id = l.split(' ').next().unwrap_or("").to_string();
        out.push(l);
        out.extend(found_by(&ran, &id).map(|f| format!("  {}", f.line())));
    }
    out.extend(clips(project, &notes, &ran, &mut s));
    state::write(project, &s)?;
    let tally = crate::tally(project, &notes, &s);
    out.push(tally);
    Ok((out.join("\n"), i32::from(refused || !bad.is_empty())))
}
