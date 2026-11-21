//! The verbs, run with the encoders loaded: new, bootstrap, teach and score; and report, which needs no encoders.

use crate::{Eval, at, flag, has, listed, named, words};
use anyhow::{Context, Result, anyhow, bail};
use raea_sys_columns::{Encoders, FPS, media};
use raea_sys_fingerprint::evidence::{Ledger, Row, features};
use raea_sys_fingerprint::folder::{Folder, root};
use raea_sys_fingerprint::inherit;
use raea_sys_fingerprint_use::score::{Scored, clock, score, summary, timeline};
use raea_sys_fingerprint_use::sheet::{Tick, sheet};
use raea_sys_fingerprint::taxonomy::{Applies, Fingerprint, base, words_of};
use raea_sys_fingerprint::words::Words;
use crate::bootstrap;
use raea_sys_fingerprint::{report_text, retrain};
use serde_json::json;
use std::path::Path;

/// The fingerprint a verb is about: --fp, else the one `new` made first, else "me"

/// A verb's answer and exit code
pub(crate) fn run(enc: &mut Encoders, args: &[String], cwd: &Path, eval: Eval, say: &mut dyn FnMut(String)) -> (String, i32) {
    let rest = &args[1..];
    let ran = match args[0].as_str() {
        "new" => new(enc, rest),
        "bootstrap" => boot(enc, rest, cwd, eval, say),
        "teach" => teach(enc, rest, cwd, eval),
        "approve" => approved(enc, rest, cwd, eval),
        "reembed" => reembedded(enc, rest, cwd, eval),
        "pair" | "versions" => paired(enc, args[0].as_str(), rest, cwd, eval),
        "learn-notes" => learned(enc, rest, cwd, eval),
        "move" => moved(enc, rest, eval),
        "postmortem" => crate::postmortem::run(enc, rest, cwd, eval),
        "things" => crate::things::run(enc, rest, cwd),
        "profile" => {
            let dir = crate::pick::chosen(rest, None).0;
            crate::profiles::make(enc, &dir, rest, cwd).map(|o| (o, 0))
        }
        "score" => scored(enc, rest, cwd, eval),
        v => Err(anyhow!("no verb {v}")),
    };
    ran.unwrap_or_else(|e| (format!("raea fingerprint {}: {e:#}", args[0]), 1))
}

/// A new fingerprint as `new` makes it: raea's base axes, or the axes of the one it --extends, with --who and --about,
/// and the kinds of piece and project tags it judges (--kinds, --tags, each comma-separated)
pub(crate) fn shaped(short: &str, args: &[String], parent: Option<Fingerprint>) -> Fingerprint {
    let mut fp = base(short);
    if let Some(p) = parent {
        fp.axes = p.axes;
        fp.extends = Some(p.name);
    }
    fp.who = flag(args, "--who").map(str::to_string).or_else(|| std::env::var("USER").ok());
    fp.about = flag(args, "--about").map(str::trim).filter(|a| !a.is_empty()).map(str::to_string);
    let (kinds, tags) = (flag(args, "--kinds").map(words_of).unwrap_or_default(), flag(args, "--tags").map(words_of).unwrap_or_default());
    if !kinds.is_empty() || !tags.is_empty() {
        fp.applies = Some(Applies { kinds, tags });
    }
    fp
}

fn new(enc: &mut Encoders, args: &[String]) -> Result<(String, i32)> {
    let n = *words(args).map_err(|e| anyhow!(e))?.first().ok_or_else(|| anyhow!("name it: raea fingerprint new NAME"))?;
    let dir = Folder::named(n);
    let short = dir.dir.file_name().map(|s| s.to_string_lossy().into_owned()).unwrap_or_else(|| n.into());
    let parent = match flag(args, "--extends") {
        Some(p) => Some(Folder::named(p).read(None).with_context(|| format!("--extends {p}"))?),
        None => None,
    };
    let from = parent.as_ref().map(|p| format!("{}'s axes", p.name)).unwrap_or_else(|| "raea's base taxonomy".into());
    let mut fp = shaped(&short, args, parent);
    fp.encoders = enc.pins();
    dir.create(&fp)?;
    let w = Words::build(enc, &fp)?;
    dir.write_words(&w)?;
    retrain(&dir, &fp, &w, &Ledger::default(), None)?;
    if !root().join("default").exists() && dir.dir.parent() == Some(root().as_path()) {
        std::fs::write(root().join("default"), &short)?;
    }
    Ok((
        format!(
            "made {}: {} axes from {from}, {} scoring from their words today (advice until they earn a gate)\n  edit {} to add, remove or reword axes\n  next: raea fingerprint bootstrap FILE... with your own finished work trains the mechanical axes",
            dir.dir.display(),
            fp.axes.len(),
            w.axes.len(),
            dir.ts().display()
        ),
        0,
    ))
}

/// The fingerprint, its encoders checked against this machine's, and its words, made again when they changed
/// The fingerprint a verb uses (pick.rs: --fp, else the one that judges the file), its axes with its parents', and its
/// words
pub(crate) fn open(enc: &mut Encoders, args: &[String], eval: Eval, file: Option<&Path>) -> Result<(Folder, Fingerprint, Words)> {
    let dir = crate::pick::chosen(args, file).0;
    let mut raw = dir.read(Some(eval))?;
    let here = enc.pins();
    if raw.encoders.is_empty() {
        raw.encoders = here;
        dir.rewrite(&raw)?;
    } else if here.iter().filter(|(k, v)| raw.encoders.get(*k) == Some(*v)).count() == raw.encoders.len() && raw.encoders != here {
        // an encoder added since (EmbeddingGemma 2, FP1b) doesn't change the ones it was trained on
        raw.encoders = here;
        dir.rewrite(&raw)?;
    } else if raw.encoders != here {
        bail!(
            "{} was trained on other encoders ({:?}) than this machine has ({:?}); its heads refuse these columns. `raea fingerprint reembed FOLDER...` moves its evidence to these, re-embedding the rows whose media it finds",
            dir.ts().display(),
            raw.encoders,
            here
        );
    }
    // a fingerprint that extends another judges with its parents' axes too (FP14b)
    let fp = inherit::axes(&raw, &inherit::parents(&raw, Some(eval)));
    let w = match dir.words().filter(|w| !w.stale(&fp)) {
        Some(w) => w,
        None => {
            let w = Words::build(enc, &fp)?;
            dir.write_words(&w)?;
            w
        }
    };
    // a shared fingerprint arrives with evidence and no heads: this machine trains its own (FP8a)
    if !dir.dir.join("heads.safetensors").is_file() {
        let l = dir.ledger()?;
        if !l.rows.is_empty() {
            retrain(&dir, &fp, &w, &l, None)?;
        }
    }
    Ok((dir, fp, w))
}

fn boot(enc: &mut Encoders, args: &[String], cwd: &Path, eval: Eval, say: &mut dyn FnMut(String)) -> Result<(String, i32)> {
    let (dir, fp, w) = open(enc, args, eval, None)?;
    let mut files = named(args, cwd)?;
    if let Some(list) = flag(args, "--from") {
        files.extend(listed(cwd, list)?);
    }
    if files.is_empty() {
        bail!("give it real finished work: raea fingerprint bootstrap FILE... or --from LIST (no placeholder footage)");
    }
    let per: usize = flag(args, "--per").map_or(Ok(10), str::parse).context("--per takes a count")?;
    let mut l = dir.ledger()?;
    let who = fp.who.clone().unwrap_or_else(|| "me".into());
    let made = bootstrap::rows(enc, &who, &files, per, &mut l, say)?;
    dir.write_ledger(&l)?;
    say(format!("{} windows from {} sources; measuring held out by source", made.windows, made.sources));
    let curves = bootstrap::curves(&fp, &l);
    let ws = bootstrap::word_scores(&l, &w);
    let patches = raea_sys_fingerprint_things::located::bootstrap(enc, &dir.dir, &files, per, say)?;
    let b = json!({"made": made, "per": per, "sources": files, "curves": curves, "words": ws, "patches": patches});
    retrain(&dir, &fp, &w, &l, Some(b))?;
    Ok((report_text(&dir.report().unwrap_or_default()), 0))
}

/// The project a file belongs to: the folder holding .raea/project above it, else the folder it's in
fn project_of(file: &Path) -> String {
    let mut d = file.parent();
    while let Some(p) = d {
        if p.join(".raea/project").exists() {
            return p.file_name().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
        }
        d = p.parent();
    }
    file.parent().and_then(|p| p.file_name()).map(|s| s.to_string_lossy().into_owned()).unwrap_or_default()
}

/// `approve`: approved work as clean rows on every axis (approve.rs, FP6a), then the heads retrained
fn approved(enc: &mut Encoders, args: &[String], cwd: &Path, eval: Eval) -> Result<(String, i32)> {
    let first = words(args).map_err(|e| anyhow!(e))?.first().map(|f| at(cwd, f));
    let (dir, fp, w) = open(enc, args, eval, first.as_deref())?;
    let (mut out, _) = crate::approve::approve(enc, &dir, &fp, args, cwd, project_of)?;
    let tr = retrain(&dir, &fp, &w, &dir.ledger()?, None)?;
    out += &format!("{} axes gating\n", tr.heads.iter().filter(|h| h.state == "gating").count());
    Ok((out, 0))
}

/// `reembed`: the evidence moved to this machine's encoders (reembed.rs, FP8c), the words remade and the heads retrained
fn reembedded(enc: &mut Encoders, args: &[String], cwd: &Path, eval: Eval) -> Result<(String, i32)> {
    let dir = crate::pick::chosen(args, None).0;
    let raw = dir.read(Some(eval))?;
    let (fp, out) = crate::reembed::reembed(enc, &dir, raw, enc.pins(), args, cwd)?;
    let fp = inherit::axes(&fp, &inherit::parents(&fp, Some(eval)));
    let w = Words::build(enc, &fp)?;
    dir.write_words(&w)?;
    retrain(&dir, &fp, &w, &dir.ledger()?, None)?;
    Ok((out, 0))
}

/// `pair` and `versions` (pairing.rs, FP6b, FP6c), then the heads retrained
fn paired(enc: &mut Encoders, verb: &str, args: &[String], cwd: &Path, eval: Eval) -> Result<(String, i32)> {
    let first = words(args).map_err(|e| anyhow!(e))?.first().map(|f| at(cwd, f));
    let (dir, fp, w) = open(enc, args, eval, first.as_deref())?;
    let who = fp.who.clone().unwrap_or_else(|| "me".into());
    let mut l = dir.ledger()?;
    let out = if verb == "pair" { crate::pairing::pair(enc, &mut l, &who, args, cwd, project_of)? } else { crate::pairing::versions(enc, &mut l, &who, args, cwd, project_of)? };
    dir.write_ledger(&l)?;
    retrain(&dir, &fp, &w, &l, None)?;
    Ok((out, 0))
}

/// `learn-notes [PROJECT]` (notes.rs, FP5a): checked readings as evidence in the fingerprint that judges the project
/// (FP14d), then its heads retrained
fn learned(enc: &mut Encoders, args: &[String], cwd: &Path, eval: Eval) -> Result<(String, i32)> {
    let (dir, fp, w) = open(enc, args, eval, Some(&crate::pick::piece_in(args, cwd)))?;
    let mut l = dir.ledger()?;
    let out = crate::notes::learn(enc, &dir, &mut l)?;
    dir.write_ledger(&l)?;
    retrain(&dir, &fp, &w, &l, None)?;
    Ok((out, 0))
}

/// `move` (moving.rs, FP14d): a child's own labels up into a fingerprint it extends, then both retrained
fn moved(enc: &mut Encoders, args: &[String], eval: Eval) -> Result<(String, i32)> {
    let (dir, fp, w) = open(enc, args, eval, None)?;
    let (to, out) = crate::moving::up(&dir, &dir.read(Some(eval))?, args, eval)?;
    retrain(&dir, &fp, &w, &dir.ledger()?, None)?;
    let (pd, pfp, pw) = open(enc, &["--fp".into(), to], eval, None)?;
    retrain(&pd, &pfp, &pw, &pd.ledger()?, None)?;
    Ok((out, 0))
}

fn teach(enc: &mut Encoders, args: &[String], cwd: &Path, eval: Eval) -> Result<(String, i32)> {
    let file = at(cwd, words(args).map_err(|e| anyhow!(e))?.first().ok_or_else(|| anyhow!("which file?"))?);
    let (dir, fp, w) = open(enc, args, eval, Some(&file))?;
    let axis = flag(args, "--axis").ok_or_else(|| anyhow!("--axis A (or \"*\" for every axis)"))?;
    if axis != "*" && fp.axis(axis).is_none() {
        bail!("no axis {axis:?} in {}: its axes are {}", dir.ts().display(), fp.axes.iter().map(|a| a.name.as_str()).collect::<Vec<_>>().join(", "));
    }
    let t: f64 = flag(args, "--at").ok_or_else(|| anyhow!("--at SECONDS"))?.parse().context("--at takes seconds")?;
    let span: f64 = flag(args, "--span").map_or(Ok(2.0), str::parse).context("--span takes seconds")?;
    let region = flag(args, "--box").map(|b| -> Result<[f32; 4]> {
        let v: Vec<f32> = b.split(',').map(str::parse).collect::<Result<_, _>>().context("--box takes x0,y0,x1,y1 in 0-1")?;
        v.try_into().map_err(|_| anyhow!("--box takes four numbers"))
    });
    let c = enc.columns(&file)?;
    let a = (t - span / 2.0).clamp(0.0, (c.dur - span).max(0.0));
    let b = (a + span).min(c.dur);
    let bad = !has(args, "--good");
    let row = Row {
        media: c.media.clone(),
        a,
        b,
        axis: axis.into(),
        bad,
        weight: 1.0,
        who: fp.who.clone().unwrap_or_else(|| "me".into()),
        source: "teach".into(),
        project: flag(args, "--project").map_or_else(|| project_of(&file), str::to_string),
        kind: None,
        region: region.transpose()?,
        words: flag(args, "--words").map(str::to_string),
        at: 0,
        pin: None,
        band: None,
    };
    let mut l = dir.ledger()?;
    l.push(row, &features(&c, a, b));
    dir.write_ledger(&l)?;
    let tr = retrain(&dir, &fp, &w, &l, None)?;
    let h = tr.heads.iter().find(|h| h.axis == axis);
    let (p, n) = l.for_axis(axis).iter().fold((0, 0), |(p, n), &i| if l.rows[i].bad { (p + 1, n) } else { (p, n + 1) });
    Ok((
        format!(
            "taught {axis}: {} at {} in {}\n  {axis} has {p} defect and {n} clean labels; its head: {}",
            if bad { "the defect" } else { "clean" },
            clock(t),
            file.display(),
            h.map_or("every axis's clean labels grew".to_string(), |h| format!("{} ({})", h.state, h.why))
        ),
        0,
    ))
}

/// raea check's findings on the file, in seconds: (rule, from, to, what)
fn checks(file: &Path) -> Vec<(String, f64, f64, String)> {
    let Ok(found) = raea_app_check::floor(&[file.display().to_string()], &[], &[]) else { return Vec::new() };
    found.into_iter().map(|f| {
        let (a, b) = f.frames.map_or((0.0, 0.0), |[a, b]| (a as f64 / 30.0, b as f64 / 30.0));
        (f.check, a, b, f.says)
    }).collect()
}

/// A check wins over a head (FP4c): a moment on an axis a check measures is no gate unless the check found it too
fn arbitrate(s: &mut Scored, fp: &Fingerprint, found: &[(String, f64, f64, String)]) {
    for m in &mut s.moments {
        let Some(rule) = fp.axis(&m.axis).and_then(|a| a.check.as_deref()) else { continue };
        let agrees = found.iter().any(|(r, a, b, _)| r == rule && *a <= m.b + 1.0 && *b >= m.a - 1.0);
        if !agrees {
            m.gating = false;
            m.state = format!("{}; the {rule} check found none", m.state);
        }
    }
}

fn scored(enc: &mut Encoders, args: &[String], cwd: &Path, eval: Eval) -> Result<(String, i32)> {
    let file = at(cwd, words(args).map_err(|e| anyhow!(e))?.first().ok_or_else(|| anyhow!("which file?"))?);
    let (dir, fp, w) = open(enc, args, eval, Some(&file))?;
    let why = crate::pick::chosen(args, Some(&file)).1.map_or_else(|| "named with --fp, or the account's default".to_string(), |c| c.why);
    let c = enc.columns(&file)?;
    let trained = dir.trained()?.map(|(t, _)| t);
    let mut s = score(&fp, Some(&w), trained.as_ref(), &c, &file.display().to_string());
    let found = checks(&file);
    arbitrate(&mut s, &fp, &found);
    raea_sys_fingerprint_things::located::locate(enc, &file, &raea_sys_fingerprint_use::patch::load(&dir.dir), &mut s)?;
    // AK8: each flag asked again of the decision model on its own window, the answers kept as clef's evidence
    let doubles = match flag(args, "--ask").map(str::to_string).or_else(|| fp.ask.clone()) {
        Some(which) => {
            anyhow::ensure!(which == "flags" || which == "unsure", "--ask takes flags or unsure, not {which}");
            let d = raea_app_double::ask(&file, &fp, raea_app_double::moments(&s, &fp, &which), &raea_app_double::Opts::new(4, false));
            raea_app_double::keep(&dir, &c, &project_of(&file), &d).map_err(|e| anyhow!(e))?;
            d
        }
        None => Vec::new(),
    };
    // FP9a: each gating flag on the cut before, judged on this one, and its axis's disguises rerun on the fix
    let fixes = match flag(args, "--was") {
        Some(old) => {
            let was = at(cwd, old);
            let mut ws = score(&fp, Some(&w), trained.as_ref(), &enc.columns(&was)?, &was.display().to_string());
            arbitrate(&mut ws, &fp, &checks(&was));
            Some(raea_sys_fingerprint_use::fixed::fixes(&ws, &s, &fp, &dir.ledger()?, &c))
        }
        None => None,
    };
    let mut out = if has(args, "--json") {
        serde_json::to_string_pretty(&json!({"fingerprint": {"name": fp.name, "about": fp.about, "why": why}, "scored": s, "clef": doubles, "checks": found.iter().map(|(r, a, b, says)| json!({"check": r, "a": a, "b": b, "says": says})).collect::<Vec<_>>(), "fixes": fixes}))?
    } else if let Some(f) = &fixes {
        let mut t = format!("judged by {}: {why}\n", fp.name);
        t += &raea_sys_fingerprint_use::fixed::said(f);
        t += &summary(&s);
        t
    } else {
        let mut t = format!("judged by {}: {why}\n", fp.name);
        t += &summary(&s);
        if !doubles.is_empty() {
            t += &raea_app_double::lines(&doubles);
        }
        t += &format!("checks: {}\n", if found.is_empty() { "no findings".to_string() } else { format!("{} findings", found.len()) });
        for (r, a, _, says) in &found {
            t += &format!("  {} {r}: {says}\n", clock(*a));
        }
        t
    };
    if has(args, "--timeline") {
        out += &timeline(&s);
    }
    if let Some(p) = flag(args, "--profile") {
        out += &crate::profiles::against(enc, &dir, p, &file)?.0;
    }
    if let Some(png) = flag(args, "--sheet") {
        let png = at(cwd, png);
        let frames: Vec<media::Rgb> = (0..12).filter_map(|i| media::frames(&file, FPS, c.dur * (i as f64 + 0.5) / 12.0, 0.25).ok()?.into_iter().next()).collect();
        let pcm = media::sound(&file, 16_000)?;
        let ticks: Vec<Tick> = found.iter().map(|(r, a, b, _)| Tick { a: *a, b: *b, what: r.clone() }).collect();
        sheet(&s, &frames, &pcm, &ticks, &|axis| fp.axis(axis).map(|a| a.modality), &png)?;
        out += &format!("sheet: {}\n", png.display());
    }
    let unfixed = fixes.iter().flatten().any(|f| f.verdict != "fixed");
    Ok((out, i32::from(unfixed || s.moments.iter().any(|m| m.gating))))
}

/// `raea fingerprint report`: report.json, read; no encoders needed
pub(crate) fn report_here(args: &[String]) -> i32 {
    let dir = crate::pick::chosen(args, None).0;
    match dir.report() {
        Some(r) if has(args, "--json") => {
            println!("{}", serde_json::to_string_pretty(&r).unwrap_or_default());
            0
        }
        Some(r) => {
            println!("{}", report_text(&r).trim_end());
            0
        }
        None => {
            eprintln!("raea fingerprint report: no report at {} (`raea fingerprint new NAME` makes one)", dir.dir.display());
            1
        }
    }
}

#[cfg(test)]
mod tests {
    use super::shaped;
    use raea_sys_fingerprint::taxonomy::{Applies, base};

    fn args(xs: &[&str]) -> Vec<String> {
        xs.iter().map(|x| x.to_string()).collect()
    }

    /// FP14a: a new fingerprint says what it's for, and one that extends another starts from its axes
    #[test]
    fn a_new_fingerprint_says_what_its_for_and_what_it_builds_on() {
        let own = shaped("me", &args(&["me", "--who", "w"]), None);
        assert_eq!((own.about.as_deref(), own.extends.as_deref(), own.who.as_deref()), (None, None, Some("w")));
        assert_eq!(own.axes, base("me").axes);

        let mut parent = base("me");
        parent.axes.truncate(2);
        parent.axes[0].words = vec!["a soft picture".into()];
        let ads = shaped("ads", &args(&["ads", "--about", " Short ads for one brand ", "--extends", "me"]), Some(parent.clone()));
        assert_eq!(ads.name, "ads");
        assert_eq!(ads.about.as_deref(), Some("Short ads for one brand"));
        assert_eq!(ads.extends.as_deref(), Some("me"));
        assert_eq!(ads.axes, parent.axes);
        assert_eq!(shaped("x", &args(&["x", "--about", "  "]), None).about, None);
        assert_eq!(own.applies, None);
        let judged = shaped("ads", &args(&["ads", "--kinds", "ad, Playable", "--tags", "brand,"]), None);
        assert_eq!(judged.applies, Some(Applies { kinds: vec!["ad".into(), "playable".into()], tags: vec!["brand".into()] }));
    }
}
