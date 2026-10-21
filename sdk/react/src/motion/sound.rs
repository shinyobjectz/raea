//! raea's sound tools from the command line: the two local models (SAO for foley, TokenSynth for an instrument's notes)
//! and the open sample libraries a Tone.js Sampler can play.
//!
//!   raea-motion sao "PROMPT" [--seconds 2] [--seed 1] [--dir EDIT_DIR] [-o OUT.wav]
//!   raea-motion tokensynth "TIMBRE" [--notes C2,C3,C4] [--seconds 2] [--velocity 100] [--seed 1234] [--precision fp16]
//!                          [--dir EDIT_DIR] [-o OUT_DIR]
//!   raea-motion setup-audio [--check] [sao|tokensynth]
//!   raea-motion sounds [WORDS...]
//!   raea-motion sounds get PACK/INSTRUMENT [--notes C2,C4 | --all] [--dir EDIT_DIR] [-o OUT_DIR]
//!
//! sao and tokensynth make through the same asset store a composition's render does (rdb::assets::Store beside
//! EDIT_DIR/edit.tsx, default the working folder), with the same parameters, so a sound made here is the one
//! `sao("…", {seconds})` or `Tone.tokensynth("…", notes)` in that edit finds already made, and the reverse. Their
//! defaults are the TypeScript functions' defaults for the same reason. Either model installs itself the first time
//! it's used (rdb::assets::ensure_installed); setup-audio does it on purpose and says what's there.

use std::path::{Path, PathBuf};

use serde_json::{Value, json};

type R<T> = Result<T, String>;

fn flag(args: &[String], name: &str) -> Option<String> {
    args.iter().position(|a| a == name).and_then(|i| args.get(i + 1)).cloned()
}

/// The words of a command that aren't flags or a flag's value.
fn positional<'a>(args: &'a [String], with_value: &[&str]) -> Vec<&'a str> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < args.len() {
        let a = args[i].as_str();
        if with_value.contains(&a) {
            i += 2;
            continue;
        }
        if !a.starts_with("--") || a == "-" {
            out.push(a);
        }
        i += 1;
    }
    out
}

fn number(args: &[String], name: &str, default: f64) -> R<f64> {
    match flag(args, name) {
        None => Ok(default),
        Some(v) => v.parse::<f64>().ok().filter(|n| n.is_finite()).ok_or(format!("{name} takes a number, got `{v}`")),
    }
}

/// The edit folder whose asset store a make goes through: --dir, else the working folder.
fn store(args: &[String]) -> rdb::assets::Store {
    let dir = PathBuf::from(flag(args, "--dir").unwrap_or_else(|| ".".into()));
    rdb::assets::Store::beside(&dir.join("edit.tsx"))
}

/// A number as JavaScript prints it (2, not 2.0), for the TypeScript line that reuses what was made.
fn js_num(n: f64) -> String {
    if n.fract() == 0.0 && n.abs() < 1e15 { format!("{}", n as i64) } else { format!("{n}") }
}

// ---- notes ----------------------------------------------------------------------------------------------------------

const SHARPS: [&str; 12] = ["C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B"];
const FLATS: [&str; 12] = ["C", "Db", "D", "Eb", "E", "F", "Gb", "G", "Ab", "A", "Bb", "B"];

/// A note's MIDI number: "C4" is 60, as in Tone; sharps as `#` or `s` (tonejs-instruments' file names), flats as `b`,
/// or a MIDI number itself.
pub fn midi_of(note: &str) -> Option<i64> {
    let s = note.trim();
    if let Ok(n) = s.parse::<i64>() {
        return (0..=127).contains(&n).then_some(n);
    }
    let mut c = s.chars();
    let pc = match c.next()?.to_ascii_uppercase() {
        'C' => 0,
        'D' => 2,
        'E' => 4,
        'F' => 5,
        'G' => 7,
        'A' => 9,
        'B' => 11,
        _ => return None,
    };
    let rest: String = c.collect();
    let (acc, oct) = match rest.chars().next() {
        Some('#' | 's') => (1, &rest[1..]),
        Some('b') => (-1, &rest[1..]),
        _ => (0, rest.as_str()),
    };
    let oct: i64 = oct.parse().ok()?;
    let m = (oct + 1) * 12 + pc + acc;
    (0..=127).contains(&m).then_some(m)
}

pub fn note_name(midi: i64, flats: bool) -> String {
    let names = if flats { FLATS } else { SHARPS };
    format!("{}{}", names[midi.rem_euclid(12) as usize], midi.div_euclid(12) - 1)
}

/// A note as a file name: `C#4` is `Cs4` (a `#` in a file name is trouble in a URL).
fn note_file(midi: i64) -> String {
    note_name(midi, false).replace('#', "s")
}

fn parse_notes(list: &str) -> R<Vec<(String, i64)>> {
    list.split(',').map(str::trim).filter(|n| !n.is_empty())
        .map(|n| midi_of(n).map(|m| (n.to_string(), m)).ok_or(format!("`{n}` isn't a note (C4, F#3, Bb2 or a MIDI number)")))
        .collect()
}

// ---- sao and tokensynth ---------------------------------------------------------------------------------------------

/// Copy a made asset to where the CLI was asked to put it.
fn deliver(from: &Path, to: &Path) -> R<()> {
    if let Some(d) = to.parent().filter(|d| !d.as_os_str().is_empty()) {
        std::fs::create_dir_all(d).map_err(|e| format!("{}: {e}", d.display()))?;
    }
    std::fs::copy(from, to).map(|_| ()).map_err(|e| format!("{}: {e}", to.display()))
}

/// `raea-motion sao "PROMPT" [--seconds 2] [--seed 1] [--dir D] [-o OUT.wav]`: one foley sound, made once.
pub fn sao(args: &[String]) -> R<String> {
    const USAGE: &str = "raea-motion sao \"PROMPT\" [--seconds 2] [--seed 1] [--dir EDIT_DIR] [-o OUT.wav]";
    let prompt = positional(&args[1..], &["--seconds", "--seed", "--dir", "-o"]).join(" ");
    if prompt.trim().is_empty() {
        return Err(USAGE.into());
    }
    // sao()'s default length in TypeScript, and the seed every composition's make uses
    let seconds = number(args, "--seconds", 2.0)?;
    if !(seconds > 0.0) {
        return Err(format!("--seconds must be positive, got {seconds}"));
    }
    let seed = number(args, "--seed", 1.0)? as u64;
    let s = store(args);
    let params = json!({"seconds": seconds});
    let cached = s.made("sao", &prompt, seed, &params)?.is_some_and(|a| s.path(&a).exists());
    let t0 = std::time::Instant::now();
    let a = s.make("sao", &prompt, seed, &params)?;
    let made = s.path(&a);
    let at = match flag(args, "-o") {
        Some(o) => {
            deliver(&made, Path::new(&o))?;
            o
        }
        None => made.display().to_string(),
    };
    let how = if cached { "already made".to_string() } else { format!("made in {:.1} s", t0.elapsed().as_secs_f64()) };
    let reuse = if seed == 1 { format!("\n         in TSX: sao({prompt:?}, {{ seconds: {} }}) finds it in {}", js_num(seconds), s.dir.display()) } else { String::new() };
    Ok(format!("APPLIED  sao {prompt:?}, {} s, seed {seed} -> {at} ({how}; asset {}){reuse}", js_num(seconds), &a.hash[..12]))
}

/// `raea-motion tokensynth "TIMBRE" [--notes C2,C3,C4] ... [-o OUT_DIR]`: an instrument's root notes, one WAV each.
pub fn tokensynth(args: &[String]) -> R<String> {
    const USAGE: &str = "raea-motion tokensynth \"TIMBRE\" [--notes C2,C3,C4] [--seconds 2] [--velocity 100] [--seed 1234] [--precision fp16] [--dir EDIT_DIR] [-o OUT_DIR]";
    let timbre = positional(&args[1..], &["--notes", "--seconds", "--velocity", "--seed", "--precision", "--dir", "-o"]).join(" ");
    if timbre.trim().is_empty() {
        return Err(USAGE.into());
    }
    // Tone.tokensynth()'s defaults (sdk/motion/src/assets.ts tokensynthNote), so the params and the cache key are the same
    let notes = parse_notes(&flag(args, "--notes").unwrap_or_else(|| "C2,C3,C4".into()))?;
    if notes.is_empty() {
        return Err(USAGE.into());
    }
    let seconds = number(args, "--seconds", 2.0)?;
    if !(seconds > 0.0 && seconds < 5.0) {
        return Err(format!("--seconds is the note's held length, under 5 (TokenSynth reads notes that end before 5 s); got {seconds}"));
    }
    let velocity = number(args, "--velocity", 100.0)?.round() as i64;
    if !(1..=127).contains(&velocity) {
        return Err(format!("--velocity is 1 to 127, got {velocity}"));
    }
    let seed = number(args, "--seed", 1234.0)? as u64;
    let precision = flag(args, "--precision").unwrap_or_else(|| rdb::assets::TOKENSYNTH_PRECISION.into());
    if !rdb::assets::TOKENSYNTH_PRECISIONS.contains(&precision.as_str()) {
        return Err(format!("--precision is one of {}", rdb::assets::TOKENSYNTH_PRECISIONS.join(", ")));
    }
    let s = store(args);
    let out = flag(args, "-o").map(PathBuf::from);
    let mut lines = Vec::new();
    for (name, midi) in &notes {
        let params = json!({"seconds": seconds, "midi": midi, "velocity": velocity, "seed": seed, "device": "cpu", "precision": precision});
        let cached = s.made("tokensynth", &timbre, seed, &params)?.is_some_and(|a| s.path(&a).exists());
        let t0 = std::time::Instant::now();
        let a = s.make("tokensynth", &timbre, seed, &params)?;
        let made = s.path(&a);
        let at = match &out {
            Some(d) => {
                let to = d.join(format!("{}.wav", note_file(*midi)));
                deliver(&made, &to)?;
                to.display().to_string()
            }
            None => made.display().to_string(),
        };
        let how = if cached { "already made".to_string() } else { format!("made in {:.1} s", t0.elapsed().as_secs_f64()) };
        lines.push(format!("  {name:<4} (MIDI {midi}) -> {at} ({how})"));
    }
    let names: Vec<String> = notes.iter().map(|(n, _)| format!("{n:?}")).collect();
    let mut opts = vec![format!("seconds: {}", js_num(seconds))];
    if velocity != 100 {
        opts.push(format!("velocity: {velocity}"));
    }
    if seed != 1234 {
        opts.push(format!("seed: {seed}"));
    }
    if precision != rdb::assets::TOKENSYNTH_PRECISION {
        opts.push(format!("precision: {precision:?}"));
    }
    Ok(format!(
        "APPLIED  tokensynth {timbre:?}: {} note(s), {} s held, velocity {velocity}, seed {seed}, {precision}\n{}\n         in TSX: new Tone.Sampler({{ urls: Tone.tokensynth({timbre:?}, [{}], {{ {} }}) }}) finds them in {}",
        notes.len(),
        js_num(seconds),
        lines.join("\n"),
        names.join(", "),
        opts.join(", "),
        s.dir.display()
    ))
}

/// `raea-motion setup-audio [--check] [sao|tokensynth]`: installs whichever local model is missing (--check only
/// says), then where each lives and how much disk it takes.
pub fn setup_audio(args: &[String]) -> R<String> {
    let check = args.iter().any(|a| a == "--check");
    let wanted: Vec<&str> = match positional(&args[1..], &[]).first() {
        Some(&m) if m == "sao" || m == "tokensynth" => vec![m],
        Some(m) => return Err(format!("raea-motion setup-audio [--check] [sao|tokensynth]: no model `{m}`")),
        None => vec!["tokensynth", "sao"],
    };
    let mut lines = Vec::new();
    let mut missing = 0;
    for m in wanted {
        let dir = rdb::assets::install_dir(m);
        let was = rdb::assets::installed(m, &dir);
        if !was && !check {
            rdb::assets::ensure_installed(m)?;
        }
        let now = rdb::assets::installed(m, &dir);
        missing += !now as usize;
        let what = if m == "sao" { "foley (Stable Audio Open Small, Core ML)" } else { "music: an instrument's notes (TokenSynth)" };
        let state = match (was, now) {
            (true, _) => "installed".to_string(),
            (false, true) => "installed now".to_string(),
            (false, false) => format!("not installed: {} installs it", rdb::assets::setup_script(m).display()),
        };
        lines.push(format!("  {m:<10} {state}; {what}\n             {} ({})", dir.display(), if now { disk(&dir) } else { "-".into() }));
    }
    let head = if check { if missing > 0 { "CHECKED  local audio models (some missing)" } else { "CHECKED  local audio models" } } else { "APPLIED  local audio models ready" };
    Ok(format!("{head}\n{}", lines.join("\n")))
}

/// What a folder takes on disk, as du says it (clones of another install count in full here, as du counts them).
fn disk(dir: &Path) -> String {
    std::process::Command::new("du").arg("-sh").arg(dir).output().ok()
        .and_then(|o| String::from_utf8_lossy(&o.stdout).split_whitespace().next().map(str::to_string))
        .unwrap_or_else(|| "?".into())
}

// ---- the open sample libraries --------------------------------------------------------------------------------------

/// sdk/motion/library/sounds/catalog.json (tools/sounds/catalog.py writes it): the libraries, their licences and every
/// instrument's files, built into the binary so a search needs no network.
const CATALOG: &str = include_str!("../../../motion/library/sounds/catalog.json");

fn catalog() -> R<Value> {
    serde_json::from_str(CATALOG).map_err(|e| format!("the sounds catalog: {e}"))
}

/// An instrument's files: (MIDI note or None, file within the instrument, bytes when known). FluidR3 has every note
/// A0-C8, named with flats; the others list their files, a pitched one's note in its name.
fn files_of(pack: &Value, inst: &Value) -> Vec<(Option<i64>, String, u64)> {
    if pack["id"] == "fluid" {
        return (21..=108).map(|m| (Some(m), format!("{}.mp3", note_name(m, true)), 0)).collect();
    }
    inst["files"].as_array().into_iter().flatten().map(|f| {
        let (path, size) = match f {
            Value::Array(a) => (a.first().and_then(|v| v.as_str()).unwrap_or_default().to_string(), a.get(1).and_then(|v| v.as_u64()).unwrap_or(0)),
            other => (other.as_str().unwrap_or_default().to_string(), 0),
        };
        (note_in(&path), path, size)
    }).collect()
}

/// The note a sample's file name carries: a token like `B2`, `A#0` or `Cs4` between underscores, spaces or dashes.
fn note_in(path: &str) -> Option<i64> {
    let base = path.rsplit('/').next().unwrap_or(path);
    let stem = base.rsplit_once('.').map(|(s, _)| s).unwrap_or(base);
    stem.split(['_', ' ', '-']).rev().find_map(|t| {
        let ok = t.len() >= 2 && t.len() <= 4 && t.starts_with(|c: char| ('A'..='G').contains(&c)) && t.ends_with(|c: char| c.is_ascii_digit());
        if ok { midi_of(t) } else { None }
    })
}

/// How loud a take is, from its name, so a fetch can take the middle dynamic: pp..ff, soft/med/loud, vlN or vN.
fn dynamic(path: &str) -> Option<i64> {
    let base = path.rsplit('/').next().unwrap_or(path).to_lowercase();
    let stem = base.rsplit_once('.').map(|(s, _)| s.to_string()).unwrap_or(base);
    stem.split(['_', ' ', '-']).find_map(|t| match t {
        "pp" => Some(10),
        "p" | "soft" => Some(20),
        "mp" => Some(30),
        "mf" | "med" | "medium" => Some(40),
        "f" | "loud" => Some(50),
        "ff" => Some(60),
        t if t.starts_with("vl") => t[2..].parse::<i64>().ok(),
        t if t.starts_with('v') && t.len() <= 3 => t[1..].parse::<i64>().ok(),
        _ => None,
    })
}

fn is_first_take(path: &str) -> bool {
    let l = path.to_lowercase();
    !l.contains("rr") || l.contains("rr1")
}

/// `raea-motion sounds [WORDS...]`: the instruments whose names hold every word (all of them with no words, by pack).
pub fn sounds(args: &[String]) -> R<String> {
    if args.get(1).map(String::as_str) == Some("get") {
        return sounds_get(args);
    }
    let c = catalog()?;
    let words: Vec<String> = positional(&args[1..], &[]).iter().map(|w| w.to_lowercase()).collect();
    let mut out = Vec::new();
    for pack in c["packs"].as_array().into_iter().flatten() {
        let pid = pack["id"].as_str().unwrap_or_default();
        let hits: Vec<String> = pack["instruments"].as_array().into_iter().flatten().filter(|i| {
            let hay = format!("{pid}/{} {} {}", i["id"].as_str().unwrap_or_default(), i["name"].as_str().unwrap_or_default(), i["family"].as_str().unwrap_or_default()).to_lowercase();
            words.iter().all(|w| hay.contains(w.as_str()))
        }).map(|i| {
            let files = files_of(pack, i);
            let pitched = files.iter().filter(|f| f.0.is_some()).count();
            let what = if pitched >= 3 {
                let lo = files.iter().filter_map(|f| f.0).min().unwrap_or(0);
                let hi = files.iter().filter_map(|f| f.0).max().unwrap_or(0);
                format!("pitched, {}-{}", note_name(lo, false), note_name(hi, false))
            } else {
                format!("{} hits", files.len())
            };
            format!("  {pid}/{}  {} ({what})", i["id"].as_str().unwrap_or_default(), i["name"].as_str().unwrap_or_default())
        }).collect();
        if !hits.is_empty() {
            out.push(format!("{} [{}] {}\n{}", pack["name"].as_str().unwrap_or_default(), pack["licence"].as_str().unwrap_or_default(), pack["source"].as_str().unwrap_or_default(), hits.join("\n")));
        }
    }
    if out.is_empty() {
        return Ok(format!("no instrument in the sample libraries matches {:?}: try one word (piano, snare, strings, marimba)", words.join(" ")));
    }
    Ok(format!("{}\n\nraea-motion sounds get PACK/INSTRUMENT [--notes C2,C4] copies one into media/sounds/ and prints the Sampler for it", out.join("\n\n")))
}

/// Percent-encode a URL path (spaces, commas and `#` in VCSL's names), keeping its slashes.
fn url_path(p: &str) -> String {
    p.bytes().map(|b| match b {
        b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' | b'/' => (b as char).to_string(),
        _ => format!("%{b:02X}"),
    }).collect()
}

/// A library file, downloaded once into ~/.cache/raea-sounds and then copied from there.
fn fetch(url: &str, cached: &Path) -> R<()> {
    if cached.exists() {
        return Ok(());
    }
    std::fs::create_dir_all(cached.parent().unwrap()).map_err(|e| e.to_string())?;
    let part = cached.with_extension("part");
    let o = std::process::Command::new("curl").args(["-fsSL", "--retry", "2", "-o"]).arg(&part).arg(url).output().map_err(|e| format!("curl: {e}"))?;
    if !o.status.success() {
        let _ = std::fs::remove_file(&part);
        return Err(format!("{url}: {}", String::from_utf8_lossy(&o.stderr).trim()));
    }
    std::fs::rename(&part, cached).map_err(|e| e.to_string())
}

/// The path a file has under the nearest `media` folder above it, for staticFile(); else as given.
fn static_path(p: &Path) -> String {
    let parts: Vec<String> = p.components().map(|c| c.as_os_str().to_string_lossy().to_string()).collect();
    match parts.iter().rposition(|c| c == "media") {
        Some(i) => parts[i + 1..].join("/"),
        None => p.display().to_string(),
    }
}

/// `raea-motion sounds get PACK/INSTRUMENT [--notes ..|--all] [-o DIR]`: an instrument's samples into the edit's
/// media (default media/sounds/PACK-INSTRUMENT/), its licence beside them, and the TSX that plays them.
fn sounds_get(args: &[String]) -> R<String> {
    const USAGE: &str = "raea-motion sounds get PACK/INSTRUMENT [--notes C2,C4 | --all] [--dir EDIT_DIR] [-o OUT_DIR]   (raea-motion sounds WORDS finds one)";
    let id = *positional(&args[2..], &["--notes", "--dir", "-o"]).first().ok_or(USAGE)?;
    let (pid, iid) = id.split_once('/').ok_or(USAGE)?;
    let c = catalog()?;
    let pack = c["packs"].as_array().into_iter().flatten().find(|p| p["id"] == pid).ok_or(format!("no sample library `{pid}`: raea-motion sounds lists them"))?;
    let inst = pack["instruments"].as_array().into_iter().flatten().find(|i| i["id"] == iid)
        .ok_or(format!("{pid} has no instrument `{iid}`: raea-motion sounds {iid} searches for it"))?;
    let files = files_of(pack, inst);
    let pitched_files: Vec<_> = files.iter().filter(|f| f.0.is_some()).cloned().collect();
    let pitched = pitched_files.iter().map(|f| f.0).collect::<std::collections::BTreeSet<_>>().len() >= 3;
    let all = args.iter().any(|a| a == "--all");
    // (the file within the instrument, the name it gets here, its note)
    let mut chosen: Vec<(String, String, Option<i64>)> = Vec::new();
    let mut said = Vec::new();
    if pitched {
        // one take per note: the folder most of the notes are in, the middle dynamic, the first round robin
        let folder = |p: &str| p.rsplit_once('/').map(|(d, _)| d.to_string()).unwrap_or_default();
        let mut by_folder: std::collections::BTreeMap<String, usize> = Default::default();
        for f in &pitched_files {
            *by_folder.entry(folder(&f.1)).or_default() += 1;
        }
        let main = by_folder.iter().max_by_key(|(_, n)| **n).map(|(d, _)| d.clone()).unwrap_or_default();
        let mut dyns: Vec<i64> = pitched_files.iter().filter_map(|f| dynamic(&f.1)).collect();
        dyns.sort();
        dyns.dedup();
        let mid = dyns.get(dyns.len() / 2).copied();
        let mut per: std::collections::BTreeMap<i64, String> = Default::default();
        let mut ranked = pitched_files.clone();
        ranked.sort_by_key(|f| (folder(&f.1) != main, dynamic(&f.1) != mid, !is_first_take(&f.1), f.1.clone()));
        for f in ranked {
            per.entry(f.0.unwrap()).or_insert(f.1);
        }
        let avail: Vec<i64> = per.keys().copied().collect();
        let picks: Vec<i64> = if all {
            avail.clone()
        } else if let Some(list) = flag(args, "--notes") {
            let mut v = Vec::new();
            for (name, m) in parse_notes(&list)? {
                let near = *avail.iter().min_by_key(|a| (**a - m).abs()).ok_or("no notes")?;
                if near != m {
                    said.push(format!("{name} isn't sampled; the nearest is {} (a Sampler pitches it)", note_name(near, false)));
                }
                v.push(near);
            }
            v.sort();
            v.dedup();
            v
        } else {
            // enough roots for a Sampler: C1-C7, at least a minor third apart (it pitches between them)
            let mut v: Vec<i64> = Vec::new();
            for &m in avail.iter().filter(|m| (24..=96).contains(*m)) {
                if v.last().is_none_or(|l| m - l >= 3) {
                    v.push(m);
                }
            }
            if v.is_empty() { avail.clone() } else { v }
        };
        for m in picks {
            let f = &per[&m];
            let ext = f.rsplit_once('.').map(|(_, e)| e).unwrap_or("wav");
            chosen.push((f.clone(), format!("{}.{ext}", note_file(m)), Some(m)));
        }
    } else {
        // a drum or an effect: its takes, first round robins first, up to 12 unless --all
        let mut v: Vec<_> = files.iter().map(|f| f.1.clone()).collect();
        v.sort_by_key(|p| (!is_first_take(p), p.clone()));
        let n = if all { v.len() } else { v.len().min(12) };
        if n < v.len() {
            said.push(format!("{} of its {} takes (--all for every one)", n, v.len()));
        }
        for f in v.into_iter().take(n) {
            let name = f.rsplit('/').next().unwrap_or(&f).replace(' ', "_");
            chosen.push((f, name, None));
        }
    }
    let base = PathBuf::from(flag(args, "--dir").unwrap_or_else(|| ".".into()));
    let out = flag(args, "-o").map(PathBuf::from).unwrap_or_else(|| base.join("media/sounds").join(format!("{pid}-{iid}")));
    std::fs::create_dir_all(&out).map_err(|e| format!("{}: {e}", out.display()))?;
    let cache = PathBuf::from(std::env::var("HOME").unwrap_or_default()).join(".cache/raea-sounds").join(pid).join(iid);
    let template = pack["url"].as_str().unwrap_or_default();
    let mut bytes = 0u64;
    for (file, name, _) in &chosen {
        let url = template
            .replace("{instrument_path}", &url_path(inst["path"].as_str().unwrap_or_default()))
            .replace("{instrument}", iid)
            .replace("{file}", &url_path(file));
        let cached = cache.join(file);
        fetch(&url, &cached)?;
        let to = out.join(name);
        std::fs::copy(&cached, &to).map_err(|e| format!("{}: {e}", to.display()))?;
        bytes += std::fs::metadata(&to).map(|m| m.len()).unwrap_or(0);
    }
    let licence = format!(
        "{}\n{}\nlicence: {}\nsource: {}\n{}\n",
        pack["name"].as_str().unwrap_or_default(),
        inst["name"].as_str().unwrap_or_default(),
        pack["licence"].as_str().unwrap_or_default(),
        pack["source"].as_str().unwrap_or_default(),
        pack["attribution"].as_str().unwrap_or_default()
    );
    std::fs::write(out.join("LICENCE.txt"), &licence).map_err(|e| e.to_string())?;
    let var = iid.replace(|c: char| !c.is_ascii_alphanumeric(), "_");
    let tsx = if pitched {
        let urls: Vec<String> = chosen.iter().map(|(_, n, m)| format!("    \"{}\": staticFile(\"{}\"),", note_name(m.unwrap(), false), static_path(&out.join(n)))).collect();
        format!("const {var} = new Tone.Sampler({{\n  urls: {{\n{}\n  }},\n  release: 1,\n}}).connect(Tone.buses.music);", urls.join("\n"))
    } else {
        let files: Vec<String> = chosen.iter().map(|(_, n, _)| format!("  staticFile(\"{}\"),", static_path(&out.join(n)))).collect();
        format!("// one Player per take; Tone.vary or alternating takes keeps repeats from sounding copied\nconst {var} = [\n{}\n].map((f) => new Tone.Player(f).connect(Tone.buses.sfx));", files.join("\n"))
    };
    let credit = if pack["licence"] == "CC0" { "CC0: no credit needed".to_string() } else { format!("credit it where the piece is shown: {}", pack["attribution"].as_str().unwrap_or_default()) };
    Ok(format!(
        "APPLIED  {id}: {} file(s), {:.1} MB -> {} (LICENCE.txt beside them; {credit}){}\n\n{tsx}",
        chosen.len(),
        bytes as f64 / 1e6,
        out.display(),
        if said.is_empty() { String::new() } else { format!("\n         {}", said.join("; ")) }
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn notes_parse_as_tone_reads_them() {
        assert_eq!(midi_of("C4"), Some(60));
        assert_eq!(midi_of("A0"), Some(21));
        assert_eq!(midi_of("C#4"), Some(61));
        assert_eq!(midi_of("Cs4"), Some(61));
        assert_eq!(midi_of("Db4"), Some(61));
        assert_eq!(midi_of("Bb2"), Some(46));
        assert_eq!(midi_of("C-1"), Some(0));
        assert_eq!(midi_of("64"), Some(64));
        assert_eq!(midi_of("H2"), None);
        assert_eq!(note_name(61, false), "C#4");
        assert_eq!(note_name(61, true), "Db4");
        assert_eq!(note_file(66), "Fs4");
        assert!(parse_notes("C3, C4,C5").unwrap().iter().map(|n| n.1).eq([48, 60, 72]));
        assert!(parse_notes("C3,X9").is_err());
    }

    #[test]
    fn the_cli_reads_its_flags_and_words() {
        let a: Vec<String> = ["sao", "a", "door", "slam", "--seconds", "1.5", "-o", "x.wav", "--seed", "3"].iter().map(|s| s.to_string()).collect();
        assert_eq!(positional(&a[1..], &["--seconds", "--seed", "--dir", "-o"]).join(" "), "a door slam");
        assert_eq!(number(&a, "--seconds", 2.0).unwrap(), 1.5);
        assert_eq!(number(&a, "--seed", 1.0).unwrap(), 3.0);
        assert_eq!(number(&a, "--velocity", 100.0).unwrap(), 100.0);
        assert!(number(&["--seconds".to_string(), "x".to_string()], "--seconds", 2.0).is_err());
        assert_eq!(js_num(2.0), "2");
        assert_eq!(js_num(0.5), "0.5");
    }

    #[test]
    fn sample_names_give_their_note_and_dynamic() {
        assert_eq!(note_in("Marimba_hit_Outrigger_B2_med_01.wav"), Some(47));
        assert_eq!(note_in("NoSus/JHPiano_NoSus_Close_A#0_vl2_rr1.wav"), Some(22));
        assert_eq!(note_in("Ds1.mp3"), Some(27));
        assert_eq!(note_in("Clap_rr1.wav"), None);
        assert_eq!(dynamic("Marimba_hit_Outrigger_B2_med_01.wav"), Some(40));
        assert_eq!(dynamic("JHPiano_NoSus_Close_A#0_vl3_rr1.wav"), Some(3));
        assert!(is_first_take("Snare2_HitNS_v2_rr1_Mid.wav") && !is_first_take("Snare2_HitNS_v2_rr2_Mid.wav"));
        assert_eq!(url_path("Idiophones/Struck Idiophones/Claps/Clap_rr1.wav"), "Idiophones/Struck%20Idiophones/Claps/Clap_rr1.wav");
        assert_eq!(static_path(Path::new("/x/obj/media/sounds/fluid-marimba/C4.mp3")), "sounds/fluid-marimba/C4.mp3");
    }

    #[test]
    fn the_catalog_has_its_libraries() {
        let c = catalog().unwrap();
        let ids: Vec<&str> = c["packs"].as_array().unwrap().iter().map(|p| p["id"].as_str().unwrap()).collect();
        assert_eq!(ids, ["fluid", "salamander", "tonejs-instruments", "vcsl"]);
        let fluid = &c["packs"][0];
        assert_eq!(files_of(fluid, &fluid["instruments"][0]).len(), 88, "A0-C8");
    }
}
