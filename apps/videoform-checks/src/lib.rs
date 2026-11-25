//! A videoform's rules checked on a composition (docs/videoform.feature VF10, VF11), run by `raea check PROJECT` with
//! raea's floor and the person's taste. The videoform is the one that judges the project (`raea fingerprint which`),
//! and its rules are its NAME.taste.ts, taste lines whose checks are these (videoform-covers, -opens, -closes, -slot,
//! -order, -together, -event). The composition is read from its worlds as a reference is (comp.rs), set beside the
//! references' kept rows, and each rule tested on it by the test that mined it (raea-sys-videoform-rules hold.rs), so
//! a rule holds on a cut exactly as it held on the references. No render is needed: the pictures come from the
//! footage's kept columns. How a cut is layered is read from its newest render, when there is one; the rules on
//! layers wait for it otherwise.

mod comp;

use raea_core_facts::{Facts, Finding, Level, Rule};
use raea_sys_columns::cache;
use raea_sys_fingerprint::folder::Folder;
use raea_sys_videoform::kinds::Kinds;
use raea_sys_videoform::rows::Rows;
use raea_sys_videoform::vocab::{fire, report};
use raea_sys_videoform_rules::{Held, hold};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

pub use comp::{Cut, cuts, rows};

/// Evaluates a .ts file's default export to its JSON (the CLI's spec host)
pub type Eval = fn(&Path) -> Result<String, String>;

/// The checks this crate holds
pub const CHECKS: [&str; 7] = ["videoform-covers", "videoform-opens", "videoform-closes", "videoform-slot", "videoform-order", "videoform-together", "videoform-event"];

/// The videoform that judges `project`: its folder, when it is one
fn judges(project: &Path) -> Option<(Folder, raea_sys_fingerprint::taxonomy::Fingerprint)> {
    let chosen = raea_sys_fingerprint_use::which::which(project)?;
    let dir = Folder { dir: chosen.dir };
    let fp = dir.read(None).ok().filter(|f| f.videoform.is_some())?;
    Some((dir, fp))
}

/// The rules file of the videoform that judges `project`, which raea check reads with the project's taste
pub fn taste(project: &Path) -> Option<PathBuf> {
    let (dir, fp) = judges(project)?;
    // the section names it once `vocab` starts the vocabulary; one written by hand has it by its usual name
    let f = dir.dir.join(fp.videoform?.rules.unwrap_or_else(|| format!("{}.taste.ts", fp.name)));
    f.is_file().then_some(f)
}

/// What the videoform's rules find on a composition of `project`, from its worlds (every frame, in order): the
/// findings, and each rule that can't be told yet with what it waits for
pub fn check(project: &Path, rules: &[&Rule], worlds: &[Facts], eval: Eval) -> Result<(Vec<Finding>, Vec<(String, String)>), String> {
    let Some((dir, fp)) = judges(project) else { return Ok((Vec::new(), rules.iter().map(|r| (r.id.clone(), "the videoform that judges this project".to_string())).collect())) };
    let Some(vf) = raea_app_videoform::held(&dir.dir, eval)? else { return Ok((Vec::new(), rules.iter().map(|r| (r.id.clone(), format!("{}'s vocabulary.ts", fp.name))).collect())) };
    let fps = worlds.first().map_or(30.0, |w| w.fps);
    let cuts = cuts(worlds);
    let mut kept: BTreeMap<String, Option<(Rows, Option<raea_sys_columns::Columns>)>> = BTreeMap::new();
    let mut read = |src: &str| {
        kept.entry(src.to_string()).or_insert_with(|| {
            let f = raea_app_videoform::footage(Path::new(src)).map_err(|e| eprintln!("raea check: {e}")).ok()?;
            let cols = cache::load(&f.media);
            Some((f, cols))
        }).clone()
    };
    let mut cut = rows(&cuts, fps, &mut read);
    layered(&mut cut, worlds, &cuts, fps);
    // what's laid over each cut is seen too: a term fires on a cut when it fires on its footage or on what's over it
    let over: Vec<Cut> = cuts.iter().map(|c| Cut { source: c.over.clone(), ..c.clone() }).collect();
    let over = rows(&over, fps, &mut read);
    // each cut's kind, as following holds footage to the kinds
    let protos = raea_sys_videoform_follow::protos(&vf.rows, &vf.kinds);
    let of: Vec<Option<usize>> = raea_sys_videoform_follow::pieces(std::slice::from_ref(&cut), &protos).iter().map(|p| p.kind).collect();
    let mut all = vf.rows.clone();
    all.push(cut);
    let mut fired: Vec<_> = all.iter().map(|r| fire(r, &vf.heads, &vf.bg, &vf.pats)).collect();
    let laid = fire(&over, &vf.heads, &vf.bg, &vf.pats);
    for ((row, o), c) in fired.last_mut().unwrap().iter_mut().zip(laid).zip(&cuts) {
        if c.over.is_some() {
            row.iter_mut().zip(o).take(vf.heads.len()).for_each(|(a, b)| *a |= b);
        }
    }
    let mut rep = report(&all, &vf.heads, &vf.bg, &vf.pats, &vf.vocab.sections);
    let mut kinds_of = vf.kinds.of.clone();
    kinds_of.push(of);
    rep.kinds = Kinds { kinds: vf.kinds.kinds.clone(), of: kinds_of };
    let (mut found, mut waits) = (Vec::new(), Vec::new());
    for r in rules {
        let level = r.level.unwrap_or(Level::Warn);
        match hold(&r.check, &r.values, &all, &fired, &rep) {
            Held::Holds => {}
            Held::Waits(why) => waits.push((r.id.clone(), why)),
            Held::Moot(_) => {}
            Held::Breaks(at) => {
                let says = format!("{} ({}'s rule): {}", r.text, fp.name, place(&at, &cuts, fps));
                let mut f = Finding::new(&r.id, level, says, &r.by);
                if let (Some(a), Some(b)) = (at.first(), at.last()) {
                    f = f.frames(cuts[*a].from, cuts[*b].to);
                }
                found.push(f);
            }
        }
    }
    Ok((found, waits))
}

/// Where a rule doesn't hold, as the person reads it: nowhere, a cut or two by their seconds, or a run of cuts
fn place(at: &[usize], cuts: &[Cut], fps: f64) -> String {
    let s = |f: i64| f as f64 / fps;
    match at {
        [] => "nowhere in it".into(),
        [a] => format!("not at {:.1}-{:.1} s", s(cuts[*a].from), s(cuts[*a].to)),
        [a, b] => format!("not at {:.1}-{:.1} s or {:.1}-{:.1} s", s(cuts[*a].from), s(cuts[*a].to), s(cuts[*b].from), s(cuts[*b].to)),
        [a, .., b] => format!("in none of its {} shots from {:.1} to {:.1} s", at.len(), s(cuts[*a].from), s(cuts[*b].to)),
    }
}

/// How each cut is layered, read from the composition's newest render when it's a video and not older than the
/// composition (VF8e's reader, on the render's frames at the cuts' seconds)
fn layered(rows: &mut Rows, worlds: &[Facts], cuts: &[Cut], fps: f64) {
    let Some(r) = worlds.first().and_then(|w| w.resources.get("Rendered")).filter(|r| r["stale"] != true) else { return };
    let out = PathBuf::from(r["out"].as_str().unwrap_or_default());
    if !out.extension().is_some_and(|x| ["mp4", "mov"].contains(&x.to_string_lossy().as_ref())) {
        return;
    }
    let shots: Vec<[f64; 2]> = cuts.iter().map(|c| [c.from as f64 / fps, c.to as f64 / fps]).collect();
    let work = raea_io_home::caches().unwrap_or_else(std::env::temp_dir).join(format!("videoform-check-{}", std::process::id()));
    match raea_app_videoform::layers(&out, &work, &shots) {
        Ok(ls) => {
            for (s, l) in rows.shots.iter_mut().zip(ls) {
                s.layers = Some(l);
            }
        }
        Err(e) => eprintln!("raea check: how {} is layered: {e}", out.display()),
    }
    let _ = std::fs::remove_dir_all(&work);
}
