//! A videoform's vocabulary.ts (docs/videoform.feature VF5): what its context says to look for, written by the person's
//! own AI, beside fingerprint.ts. The context says what matters; the references show how it's done. `raea videoform
//! vocab NAME` starts one from the context for the AI to fill; `report` (and every `add`) says what each term did on
//! the references, from their kept rows, for the AI to revise from. The terms' phrasings are embedded once by the text
//! towers (SigLIP 2 for what's seen, CLAP for what's heard) and kept in videoform/vocab.json and vocab.f16 until they
//! change. The references' shot kinds (VF6), named from the terms, are said after it and kept in videoform/kinds.json,
//! and the template made from them and the sections (VF7) is written as NAME.script.ts; the rules mined from all of it
//! (VF8) as NAME.rules.ts, with a contact sheet a shot kind, from the references' stills, to confirm them on.

use crate::{Eval, plain};
use raea_core_brief::videoform::{Correction, Term};
use raea_core_brief::Vocabulary;
use raea_sys_columns::window::Modality;
use raea_sys_columns::cache::{f16s, f32s};
use raea_sys_columns::{CLAP, Encoders, SIG};
use raea_sys_fingerprint::taxonomy::{PICTURE_VOCAB, SOUND_VOCAB, prompt};
use raea_sys_videoform::kinds::{Kinds, kinds, lines};
use raea_sys_videoform::rows::{Rows, stills};
use raea_sys_videoform_rules::{Rule, lines as rule_lines, rules, rules_ts};
use raea_sys_videoform::structure::{lines as st_lines, script_ts, structure, template};
use raea_sys_videoform::vocab::{Background, Head, Pat, Report, Space, fire, lines as did, report};
use std::path::{Path, PathBuf};

/// vocabulary.ts's name, beside fingerprint.ts
pub(crate) const FILE: &str = "vocabulary.ts";

/// The template's name, beside it (VF7)
pub(crate) fn template_file(name: &str) -> String {
    format!("{name}.script.ts")
}

/// The rules' name, beside it (VF8)
pub(crate) fn rules_file(name: &str) -> String {
    format!("{name}.rules.ts")
}

/// The vocabulary, when it's written
pub(crate) fn read(dir: &Path, eval: Eval) -> Result<Option<Vocabulary>, String> {
    let f = dir.join(FILE);
    if !f.is_file() {
        return Ok(None);
    }
    let json = eval(&f).map_err(|e| format!("{}: {e}", f.display()))?;
    raea_core_brief::read("vocabulary.ts", &json).map(Some)
}

/// A new vocabulary.ts for the person's AI to fill from the context
fn starter(name: &str, context: &str) -> String {
    let ctx = serde_json::to_string(context).unwrap();
    format!(
        r#"// {name}'s vocabulary (docs/videoform.feature VF5), written by your AI from the context: what matters, for raea
// to look for in the references, which show how it's done. After each change, `raea videoform report {name}` says
// what each term did on the references (never fires, fires everywhere, fires with another on the same shots, or
// missing where the context says it's always there), where each section was found, and shots that recur with no
// term naming them. Revise from that; two or three rounds is usually enough.
import {{ vocabulary }} from "raea/brief";

export default vocabulary({{
  context: {ctx},
  // Where the videos are, each with its things. `looks` are captions of a still ("a stone kitchen countertop"), several
  // each; `unlike` what it's easily taken for; `how: "always"` or `"usually"` where the context says so.
  places: [],
  // What's seen anywhere: {{ name, looks, unlike?, how? }}
  things: [],
  // What's heard, phrased as sounds ("a crowd laughing and cheering")
  sounds: [],
  // Words on screen and words said, as case-insensitive regular expressions: {{ name, pattern, how? }}
  text: [],
  said: [],
  // The parts the videos move through, in order: {{ name, at?: "open" | "middle" | "close", shows: [names of the terms
  // and patterns above], how? }}
  sections: [],
  // The context's words for how it feels, each with the measures that say it (raea videoform report lists them):
  // those measures are checked, by the context's word, even where the references vary
  feel: [],
  // What's in the references but not the videoform's, as looks
  ignore: [],
}});
"#
    )
}

/// What's embedded: a space and a sentence, in order, each once
#[derive(Default)]
struct Want(Vec<(Space, String)>);

impl Want {
    /// Each phrase, as the space's encoder was trained to read it, and where it is in the list
    fn add<S: AsRef<str>>(&mut self, sp: Space, phrases: &[S]) -> Vec<usize> {
        let m = if sp == Space::Picture { Modality::Picture } else { Modality::Sound };
        let mut at = Vec::new();
        for p in phrases {
            let text = prompt(m, p.as_ref());
            at.push(self.0.iter().position(|w| w.0 == sp && w.1 == text).unwrap_or_else(|| {
                self.0.push((sp, text));
                self.0.len() - 1
            }));
        }
        at
    }
}

/// The sentences' vectors, from videoform/vocab.json and vocab.f16 when they were made from these, else embedded now
fn embed(dir: &Path, want: &Want) -> Result<Vec<Vec<f32>>, String> {
    let made: Vec<String> = want.0.iter().map(|(sp, t)| format!("{sp:?}|{t}")).collect();
    let (json, halfs) = (dir.join("videoform/vocab.json"), dir.join("videoform/vocab.f16"));
    let width = |sp: Space| if sp == Space::Picture { SIG } else { CLAP };
    let kept: Option<Vec<String>> = std::fs::read(&json).ok().and_then(|b| serde_json::from_slice(&b).ok());
    if kept.as_ref() == Some(&made) {
        if let Ok(b) = std::fs::read(&halfs) {
            let flat = f32s(&b);
            if flat.len() == want.0.iter().map(|w| width(w.0)).sum::<usize>() {
                let mut at = 0;
                return Ok(want.0.iter().map(|w| {
                    at += width(w.0);
                    flat[at - width(w.0)..at].to_vec()
                }).collect());
            }
        }
    }
    eprintln!("raea videoform: embedding the vocabulary's {} phrasings", want.0.len());
    let mut enc = Encoders::load().map_err(|e| format!("loading the text towers: {e:#}"))?;
    let of = |sp: Space| want.0.iter().filter(|w| w.0 == sp).map(|w| w.1.clone()).collect::<Vec<_>>();
    let (pic, snd) = (of(Space::Picture), of(Space::Sound));
    let p = if pic.is_empty() { Vec::new() } else { enc.words_picture(&pic).map_err(|e| format!("SigLIP 2's text tower: {e:#}"))? };
    let s = if snd.is_empty() { Vec::new() } else { enc.words_sound(&snd).map_err(|e| format!("CLAP's text tower: {e:#}"))? };
    let (mut pi, mut si) = (p.chunks(SIG), s.chunks(CLAP));
    let out: Vec<Vec<f32>> = want.0.iter().map(|w| if w.0 == Space::Picture { pi.next() } else { si.next() }.unwrap_or_default().to_vec()).collect();
    std::fs::write(&halfs, f16s(&out.concat())).map_err(|e| format!("{}: {e}", halfs.display()))?;
    std::fs::write(&json, serde_json::to_vec(&made).unwrap()).map_err(|e| format!("{}: {e}", json.display()))?;
    Ok(out)
}

fn unit_mean(vs: &[&Vec<f32>]) -> Vec<f32> {
    let w = vs.first().map_or(0, |v| v.len());
    let mut m = vec![0f32; w];
    vs.iter().for_each(|v| m.iter_mut().zip(v.iter()).for_each(|(a, b)| *a += b));
    let n = m.iter().map(|x| x * x).sum::<f32>().sqrt().max(1e-6);
    m.iter().map(|x| x / n).collect()
}

/// The vocabulary's terms as heads, and the background they're scored against
fn heads(dir: &Path, v: &Vocabulary) -> Result<(Vec<Head>, Background), String> {
    let mut want = Want::default();
    let (bg_p, bg_s) = (want.add(Space::Picture, &PICTURE_VOCAB), want.add(Space::Sound, &SOUND_VOCAB));
    let mut terms: Vec<(&Term, Space, bool)> = Vec::new();
    for p in &v.places {
        terms.push((p, Space::Picture, false));
        terms.extend(p.things.iter().map(|t| (t, Space::Picture, false)));
    }
    terms.extend(v.things.iter().map(|t| (t, Space::Picture, false)));
    terms.extend(v.sounds.iter().map(|t| (t, Space::Sound, false)));
    let ignored: Vec<Term> = v.ignore.iter().map(|i| Term { name: format!("(ignore) {i}"), looks: vec![i.clone()], ..Default::default() }).collect();
    terms.extend(ignored.iter().map(|t| (t, Space::Picture, true)));
    let idx: Vec<(Vec<usize>, Vec<usize>)> = terms.iter().map(|(t, sp, _)| (want.add(*sp, &t.looks), want.add(*sp, &t.unlike))).collect();
    let vecs = embed(dir, &want)?;
    let bg = Background { picture: bg_p.iter().map(|&i| vecs[i].clone()).collect(), sound: bg_s.iter().map(|&i| vecs[i].clone()).collect() };
    let heads = terms.iter().zip(idx).map(|((t, sp, ignore), (looks, unlike))| Head {
        name: t.name.clone(),
        space: Some(*sp).filter(|_| !looks.is_empty()),
        v: unit_mean(&looks.iter().map(|&i| &vecs[i]).collect::<Vec<_>>()),
        unlike: unlike.iter().map(|&i| vecs[i].clone()).collect(),
        how: t.how.clone(),
        ignore: *ignore,
    }).collect();
    Ok((heads, bg))
}

/// The patterns, compiled case-insensitive
fn patterns(v: &Vocabulary) -> Result<Vec<Pat>, String> {
    let pats = v.text.iter().map(|p| (p, false)).chain(v.said.iter().map(|p| (p, true)));
    pats.map(|(p, said)| {
        let re = regex::Regex::new(&format!("(?i){}", p.pattern)).map_err(|e| format!("vocabulary.ts: {}'s pattern: {e}", p.name))?;
        Ok(Pat { name: p.name.clone(), re, said, how: p.how.clone() })
    }).collect()
}

/// The vocabulary against the references' kept rows: the person's AI's report, and its JSON
pub(crate) fn found(dir: &Path, v: &Vocabulary, said: &[Correction]) -> Result<(String, Report, Vec<Rule>), String> {
    let kept = crate::found::kept(dir);
    let names: Vec<&str> = kept.iter().map(|k| k.0.as_str()).collect();
    let rows: Vec<_> = kept.iter().map(|k| k.1.clone()).collect();
    let (heads, bg) = heads(dir, v)?;
    let pats = patterns(v)?;
    let mut rep = report(&rows, &heads, &bg, &pats, &v.sections);
    rep.kinds = kinds(&rows, &heads, &bg);
    let path = dir.join("videoform/kinds.json");
    std::fs::write(&path, serde_json::to_vec_pretty(&rep.kinds).unwrap()).map_err(|e| format!("{}: {e}", path.display()))?;
    let n = names.len();
    let mut out = vec![format!("What the vocabulary ({}) did on {n} references. Revise it from this:", dir.join(FILE).display())];
    out.extend(did(&rep, &rows, &names, v));
    out.extend(lines(&rep.kinds, &names));
    let shows: Vec<Vec<String>> = v.sections.iter().map(|s| s.shows.clone()).collect();
    let name = dir.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let st = structure(&rows, &rep.kinds, &rep.sections, &shows);
    let ts = dir.join(template_file(&name));
    std::fs::write(&ts, script_ts(&template(&st, &name, &names))).map_err(|e| format!("{}: {e}", ts.display()))?;
    out.extend(st_lines(&st, n));
    out.push(format!("The template, a scene a section and a beat a kind of shot: {} (raea spec checks it)", ts.display()));
    rep.structure = st;
    let fired: Vec<_> = rows.iter().map(|r| fire(r, &heads, &bg, &pats)).collect();
    let places: Vec<(String, Vec<String>)> = v.places.iter().map(|p| (p.name.clone(), p.things.iter().map(|t| t.name.clone()).collect())).collect();
    let rules_found = rules(&rows, &fired, &rep, &places, said);
    let kept = dir.join("videoform/rules.json");
    std::fs::write(&kept, serde_json::to_vec_pretty(&rules_found).unwrap()).map_err(|e| format!("{}: {e}", kept.display()))?;
    let rf = dir.join(rules_file(&name));
    std::fs::write(&rf, rules_ts(&rules_found, &name, n)).map_err(|e| format!("{}: {e}", rf.display()))?;
    out.extend(rule_lines(&rules_found, &names, &name));
    out.push(format!("The rules, a controller of rule fields: {}", rf.display()));
    out.push(match sheets(dir, &rep.kinds, &rows) {
        Ok(d) => format!("Each shot kind's stills, to confirm the rules on (kind-N.jpg is the kind a rule's kind-N names): {}", d.display()),
        Err(e) => format!("No contact sheets: {e}"),
    });
    Ok((out.join("\n"), rep, rules_found))
}

/// A contact sheet a shot kind, from the references' stills, into videoform/kinds/kind-N.jpg: what the person
/// confirms the rules on. A sheet takes each reference's first shot of the kind, then their second, and so on.
fn sheets(dir: &Path, kinds: &Kinds, rows: &[Rows]) -> Result<PathBuf, String> {
    /// stills a sheet
    const TILES: usize = 8;
    let out = dir.join("videoform/kinds");
    let _ = std::fs::remove_dir_all(&out);
    std::fs::create_dir_all(&out).map_err(|e| format!("{}: {e}", out.display()))?;
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let mut made = 0;
    for (k, kind) in kinds.kinds.iter().enumerate() {
        // each shot's place among its reference's, so the sheet goes across the references first
        let mut nth = std::collections::BTreeMap::new();
        let mut order: Vec<(usize, usize, usize)> = kind.shots.iter().map(|&(r, s)| {
            let k = nth.entry(r).or_insert(0);
            *k += 1;
            (*k, r, s)
        }).collect();
        order.sort_unstable();
        let files: Vec<PathBuf> = order.iter().map(|&(_, r, s)| stills(dir, &rows[r].media).join(format!("{s}.jpg"))).filter(|p| p.is_file()).take(TILES).collect();
        let sheet = out.join(format!("kind-{k}.jpg"));
        // a sheet of one file is that file's frames, so one still is copied as it is
        let done = match files.as_slice() {
            [] => continue,
            [one] => std::fs::copy(one, &sheet).is_ok(),
            _ => std::process::Command::new(&exe).args(["media", "sheet"]).args(&files).args(["--width", "160", "-o"]).arg(&sheet).output().is_ok_and(|o| o.status.success()),
        };
        made += usize::from(done);
    }
    match made {
        0 => Err("no reference has its stills yet: read them again with raea videoform add, which reads one again when it has none".into()),
        _ => Ok(out),
    }
}

/// `raea videoform vocab NAME`: a vocabulary.ts started from the context when there's none, else what it did
pub(crate) fn verb(args: &[String], eval: Eval) -> (String, i32) {
    let Some(name) = plain(args).into_iter().next() else { return (format!("usage: {}", crate::USAGE), 2) };
    let (dir, mut fp) = match crate::verbs::open(&name, Some(eval)) {
        Ok(x) => x,
        Err(e) => return (e, 1),
    };
    let f = dir.dir.join(FILE);
    if !f.is_file() {
        let section = fp.videoform.as_mut().unwrap();
        if let Err(e) = std::fs::write(&f, starter(&name, &section.context)) {
            return (format!("{}: {e}", f.display()), 1);
        }
        section.vocabulary = Some(FILE.into());
        section.template = Some(template_file(&name));
        section.rules = Some(rules_file(&name));
        if let Err(e) = dir.rewrite(&fp) {
            return (format!("{e:#}"), 1);
        }
        return (format!("raea videoform: started {}: your AI fills it from the context, then `raea videoform report {name}` says what each term did", f.display()), 0);
    }
    let said = fp.videoform.as_ref().map(|s| s.corrections.clone()).unwrap_or_default();
    match read(&dir.dir, eval).and_then(|v| found(&dir.dir, &v.unwrap_or_default(), &said)) {
        Ok((t, ..)) => (t, 0),
        Err(e) => (e, 1),
    }
}
