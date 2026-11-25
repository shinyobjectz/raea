//! `raea videoform follow NAME FOOTAGE... [--seconds S] [--shape W:H] [--brand B] [--plan FILE] [--script FILE] [--say N,N,...] [--brief FILE,...]`
//! (docs/videoform.feature VF9, VF9b, VF9c): new raw footage
//! read as references are, once, a row a piece (a sentence said, or a few seconds with no talk), kept by content hash
//! in raea's caches so footage followed again isn't read again; each piece matched to the videoform's shot kinds; what
//! the footage lacks said first, as a shot list; and the cut planned slot by slot from the structure, to its lengths,
//! for the agent to cut. Following reads the videoform's kept rows, never its references. With --brand, each card,
//! backdrop and inset shows the brand kit's asset that fits what's said over it; --script writes the plan as
//! NAME.script.ts, the script a project that approves its script first shows the person before anything is edited.

use crate::{Eval, flag, pass, plain, readers};
use raea_sys_columns::{CLAP, Encoders, SIG};
use raea_sys_videoform_follow::{kit::dress, lines, pieces, plan, protos, script::script_ts};
use raea_sys_videoform_kit::Kit;
use raea_sys_videoform::rows::Rows;
use std::path::{Path, PathBuf};

/// Where footage's rows are kept: by content hash, the same for every videoform it's followed to
fn keep() -> PathBuf {
    crate::cache("videoform-footage")
}

/// The footage files: each file given, and every video in each folder given, through its folders
fn files(args: &[String]) -> Result<Vec<PathBuf>, String> {
    let mut out = Vec::new();
    for a in args {
        let p = PathBuf::from(a);
        match (p.is_dir(), p.is_file()) {
            (true, _) => crate::walk(&p, &mut out, crate::is_media),
            (_, true) => out.push(p),
            _ => return Err(format!("raea videoform follow: {a} is neither a file nor a folder")),
        }
    }
    Ok(out)
}

/// A footage file's rows: kept from before, else read now (its pieces) and kept
fn rows_of(p: &Path, enc: &mut Option<Encoders>) -> Result<Rows, String> {
    let hash = readers::hash(p, None)?;
    let dir = keep();
    if let Some(r) = Rows::load(&dir, &hash, SIG, CLAP) {
        return Ok(r);
    }
    if enc.is_none() {
        eprintln!("raea videoform: loading the encoders (SigLIP 2, CLAP, YAMNet)");
        *enc = Some(Encoders::load().map_err(|e| format!("loading the encoders: {e:#}"))?);
    }
    eprintln!("raea videoform: reading the footage {}", p.display());
    let work = dir.join(format!("work-{}", std::process::id()));
    let read = pass::read(p, None, &hash, enc.as_mut().unwrap(), &work, &dir, true);
    let _ = std::fs::remove_dir_all(&work);
    read?;
    Rows::load(&dir, &hash, SIG, CLAP).ok_or_else(|| format!("{}: its rows weren't kept", p.display()))
}

/// The brand's kit, from its fingerprint (brand-SLUG), and that folder
fn kit_of(brand: &str) -> Result<(Kit, PathBuf), String> {
    let dir = raea_sys_fingerprint::folder::Folder::named(&format!("brand-{}", raea_sys_videoform_rules::slug(brand))).dir;
    let kit = Kit::load(&dir)?;
    if kit.assets.is_empty() {
        return Err(format!("raea videoform follow: {brand} has no kit yet (raea videoform kit {brand} FILE|DIR... adds its stills)"));
    }
    Ok((kit, dir))
}

/// `raea videoform follow NAME FOOTAGE... [--seconds S] [--shape W:H] [--brand B] [--plan FILE] [--script FILE] [--say N,N,...] [--brief FILE,...]`
pub(crate) fn verb(args: &[String], eval: Eval) -> (String, i32) {
    let words = plain(args);
    let (Some(name), true) = (words.first(), words.len() > 1) else { return ("usage: raea videoform follow NAME FOOTAGE... [--seconds S] [--shape W:H] [--brand B] [--plan FILE] [--script FILE] [--say N,N,...] [--brief FILE,...]".into(), 2) };
    let seconds = match flag(args, "--seconds").map(str::parse::<f64>) {
        Some(Ok(s)) if s > 0.0 => Some(s),
        Some(_) => return ("raea videoform follow: --seconds takes a length in seconds".into(), 2),
        None => None,
    };
    let brand = flag(args, "--brand").map(str::trim).filter(|b| !b.is_empty());
    let kit = match brand.map(kit_of).transpose() {
        Ok(k) => k,
        Err(e) => return (e, 1),
    };
    let shape = flag(args, "--shape");
    if shape.is_some_and(|s| s.split(':').filter(|n| n.parse::<u32>().is_ok()).count() != 2 || !s.contains(':')) {
        return ("raea videoform follow: --shape takes W:H, as 9:16".into(), 2);
    }
    let (dir, _) = match crate::verbs::open(name, Some(eval)) {
        Ok(x) => x,
        Err(e) => return (e, 1),
    };
    let v = match crate::vocab::read(&dir.dir, eval) {
        Ok(Some(v)) => v,
        Ok(None) => return (format!("raea videoform follow: {name} has no vocabulary yet (raea videoform vocab {name} starts one)"), 1),
        Err(e) => return (e, 1),
    };
    let found = files(&words[1..]).and_then(|fs| crate::vocab::understood(&dir.dir, &v).map(|u| (fs, u)));
    let (mut fs, (refs, kinds, st, pats)) = match found {
        Ok(x) => x,
        Err(e) => return (e, 1),
    };
    // the brand's own clips (its page, a product spinning) are footage too
    if let Some((k, d)) = &kit {
        fs.extend(k.assets.iter().filter(|a| a.kind == "clip").map(|a| d.join("kit").join(&a.file)));
    }
    if fs.is_empty() {
        return ("raea videoform follow: no video files in what's given".into(), 1);
    }
    let mut enc = None;
    let mut footage = Vec::new();
    let mut names = Vec::new();
    let mut out = Vec::new();
    for f in &fs {
        match rows_of(f, &mut enc) {
            Ok(r) => {
                footage.push(r);
                names.push(f.to_string_lossy().into_owned());
            }
            Err(e) => out.push(format!("  not read: {e}")),
        }
    }
    let protos = protos(&refs, &kinds);
    let all = pieces(&footage, &protos);
    // where each piece says a section's words: the vocabulary's patterns for what's said, by the names its sections show
    let hits: Vec<Vec<bool>> = all.iter().map(|x| st.slots.iter().map(|s| pats.iter().any(|p| p.said && s.shows.contains(&p.name) && p.re.is_match(&x.words))).collect()).collect();
    // the lines the agent chose (--say 12,40,7: pieces as an earlier plan numbers them), in its order
    let mut say: Option<Vec<usize>> = None;
    for n in flag(args, "--say").unwrap_or_default().split(',').map(str::trim).filter(|n| !n.is_empty()) {
        match n.parse() {
            Ok(n) => say.get_or_insert_with(Vec::new).push(n),
            Err(_) => return (format!("raea videoform follow: --say takes pieces' numbers, not {n:?}"), 2),
        }
    }
    let mut p = plan(&st, &kinds, &protos, all, &hits, seconds, say.as_deref());
    dress(&mut p, kit.as_ref().map(|(k, d)| (k, d.as_path(), brand.unwrap_or_default())));
    out.splice(0..0, lines(&p, &names));
    if let Some(f) = flag(args, "--script") {
        let about = format!("{name}'s form{}, {:.0} s{}", brand.map_or(String::new(), |b| format!(" with {b}'s kit")), p.slots.iter().map(|s| s.seconds).sum::<f64>(), shape.map_or(String::new(), |s| format!(", {s}")));
        let title = f.rsplit('/').next().unwrap_or(f).trim_end_matches(".script.ts");
        match std::fs::write(f, script_ts(&p, &names, title, &about)) {
            Ok(()) => out.push(format!("  the script: {f} (`raea spec {f}` checks it; show it to the person before anything is edited)")),
            Err(e) => out.push(format!("  {f}: {e}")),
        }
    }
    // the new job's brief (--brief FILE,FILE and the project's brief.ts) against what was learned of briefs (VF8f)
    let (said, asked) = match raea_app_videoform_docs::plan(&dir.dir, flag(args, "--brief"), eval) {
        Ok(x) => x,
        Err(e) => return (e, 1),
    };
    if !said.is_empty() {
        out.push(said);
    }
    if let Some(j) = flag(args, "--plan") {
        let doc = serde_json::json!({"videoform": name, "files": names, "brand": brand, "shape": shape, "plan": p, "brief": asked});
        if let Err(e) = std::fs::write(j, serde_json::to_vec_pretty(&doc).unwrap()) {
            out.push(format!("  {j}: {e}"));
        }
    }
    (out.join("\n"), 0)
}
