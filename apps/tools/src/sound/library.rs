//! The sound library: words and their synonyms searched, sounds fetched and kept.

use super::*;

/// The ways a library names what a search word means: a drum kit's words as producers say them ("kick", "hihat",
/// "rimshot", "crash") onto the libraries' names (VCSL's "Bass Drum", "Hi-Hat Cymbal", "rimshot" in a snare's files),
/// and a plural's singular. The word itself first.
pub(crate) fn synonyms(word: &str) -> Vec<&str> {
    let w = match word {
        "kicks" | "hats" | "hihats" | "snares" | "claps" | "toms" | "cymbals" | "shakers" | "drums" | "rims" | "rimshots" => &word[..word.len() - 1],
        _ => word,
    };
    match w {
        "kick" | "bd" | "kickdrum" | "kick-drum" => vec!["kick", "bass drum", "bass-drum", "bassdrum"],
        "hihat" | "hi-hat" | "hh" | "hat" => vec!["hihat", "hi-hat", "hi hat"],
        "rimshot" | "rim" | "sidestick" | "side-stick" => vec!["rimshot", "_rim"],
        "crash" => vec!["crash", "clash-cymbal", "clash cymbal", "suspended-cymbal"],
        "ride" => vec!["ride", "suspended-cymbal"],
        "clap" | "handclap" => vec!["clap"],
        "drum" | "kit" => vec!["drum", "membranophone"],
        "perc" | "percussion" => vec!["percussion", "idiophone", "membranophone"],
        "tom" | "toms" => vec!["tom-", "melodic_tom", "tom "],
        _ => vec![w],
    }
}

/// `raea sounds [WORDS...]`: the instruments whose names hold every word (all of them with no words, by pack).
pub fn sounds(args: &[String]) -> R<String> {
    if args.get(1).map(String::as_str) == Some("get") {
        return sounds_get(args);
    }
    let c = catalog()?;
    let words: Vec<String> = positional(&args[1..], &[]).iter().map(|w| w.to_lowercase()).collect();
    let alts: Vec<Vec<&str>> = words.iter().map(|w| synonyms(w)).collect();
    let mut out = Vec::new();
    for pack in c["packs"].as_array().into_iter().flatten() {
        let pid = pack["id"].as_str().unwrap_or_default();
        let hits: Vec<String> = pack["instruments"].as_array().into_iter().flatten().filter_map(|i| {
            let hay = format!("{pid}/{} {} {}", i["id"].as_str().unwrap_or_default(), i["name"].as_str().unwrap_or_default(), i["family"].as_str().unwrap_or_default()).to_lowercase();
            // a word in its name, or else in some of its files' names (a snare's rimshots, a tom's rim hits)
            let mut takes = Vec::new();
            for a in &alts {
                if a.iter().any(|w| hay.contains(w)) {
                    continue;
                }
                let named: Vec<String> = files_of(pack, i).into_iter().map(|f| f.1).filter(|f| a.iter().any(|w| f.to_lowercase().contains(w))).collect();
                if named.is_empty() {
                    return None;
                }
                takes.push(format!("{} of its files are {} takes", named.len(), a[0]));
            }
            Some((i, takes))
        }).map(|(i, takes)| {
            let files = files_of(pack, i);
            let pitched = files.iter().filter(|f| f.0.is_some()).count();
            let what = if pitched >= 3 {
                let lo = files.iter().filter_map(|f| f.0).min().unwrap_or(0);
                let hi = files.iter().filter_map(|f| f.0).max().unwrap_or(0);
                format!("pitched, {}-{}", note_name(lo, false), note_name(hi, false))
            } else {
                format!("{} hits", files.len())
            };
            let takes = if takes.is_empty() { String::new() } else { format!(": {}", takes.join(", ")) };
            format!("  {pid}/{}  {} ({what}{takes})", i["id"].as_str().unwrap_or_default(), i["name"].as_str().unwrap_or_default())
        }).collect();
        if !hits.is_empty() {
            out.push(format!("{} [{}] {}\n{}", pack["name"].as_str().unwrap_or_default(), pack["licence"].as_str().unwrap_or_default(), pack["source"].as_str().unwrap_or_default(), hits.join("\n")));
        }
    }
    if out.is_empty() {
        return Ok(format!("no instrument in the sample libraries matches {:?}: try one word (piano, snare, strings, marimba)", words.join(" ")));
    }
    Ok(format!("{}\n\nraea sounds get PACK/INSTRUMENT [--notes C2,C4] copies one into media/sounds/ and prints the Sampler for it", out.join("\n\n")))
}

/// Percent-encode a URL path (spaces, commas and `#` in VCSL's names), keeping its slashes.
pub(crate) fn url_path(p: &str) -> String {
    p.bytes().map(|b| match b {
        b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' | b'/' => (b as char).to_string(),
        _ => format!("%{b:02X}"),
    }).collect()
}

/// A library file, downloaded once into ~/.cache/raea-sounds and then copied from there.
pub(crate) fn fetch(url: &str, cached: &Path) -> R<()> {
    if cached.exists() {
        return Ok(());
    }
    raea_io_home::offline(&format!("the sample library file {url}"))?;
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
pub(crate) fn static_path(p: &Path) -> String {
    let parts: Vec<String> = p.components().map(|c| c.as_os_str().to_string_lossy().to_string()).collect();
    match parts.iter().rposition(|c| c == "media") {
        Some(i) => parts[i + 1..].join("/"),
        None => p.display().to_string(),
    }
}

/// `raea sounds get PACK/INSTRUMENT [--notes ..|--all] [-o DIR]`: an instrument's samples into the edit's
/// media (default media/sounds/PACK-INSTRUMENT/), its licence beside them, and the TSX that plays them.
pub(crate) fn sounds_get(args: &[String]) -> R<String> {
    const USAGE: &str = "raea sounds get PACK/INSTRUMENT [--notes C2,C4 | --all] [--dir EDIT_DIR] [-o OUT_DIR]   (raea sounds WORDS finds one)";
    let id = *positional(&args[2..], &["--notes", "--dir", "-o"]).first().ok_or(USAGE)?;
    let (pid, iid) = id.split_once('/').ok_or(USAGE)?;
    let c = catalog()?;
    let pack = c["packs"].as_array().into_iter().flatten().find(|p| p["id"] == pid).ok_or(format!("no sample library `{pid}`: raea sounds lists them"))?;
    let inst = pack["instruments"].as_array().into_iter().flatten().find(|i| i["id"] == iid)
        .ok_or(format!("{pid} has no instrument `{iid}`: raea sounds {iid} searches for it"))?;
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
    let mut out = flag(args, "-o").map(PathBuf::from).unwrap_or_else(|| base.join("media/sounds").join(format!("{pid}-{iid}")));
    let licence = format!(
        "{}\n{}\nlicence: {}\nsource: {}\n{}\n",
        pack["name"].as_str().unwrap_or_default(),
        inst["name"].as_str().unwrap_or_default(),
        pack["licence"].as_str().unwrap_or_default(),
        pack["source"].as_str().unwrap_or_default(),
        pack["attribution"].as_str().unwrap_or_default()
    );
    // a folder that already holds another instrument keeps it: two instruments' notes share names (C4.mp3), and the
    // second one's replaced the first's and its licence (#217). This one goes in a folder of its own inside it
    if std::fs::read_to_string(out.join("LICENCE.txt")).is_ok_and(|held| held != licence) {
        out = out.join(format!("{pid}-{iid}"));
        said.push(format!("the folder given already holds another instrument, so this one is in {}", out.display()));
    }
    std::fs::create_dir_all(&out).map_err(|e| format!("{}: {e}", out.display()))?;
    let cache = raea_io_home::user_home().unwrap_or_default().join(".cache/raea-sounds").join(pid).join(iid);
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
    std::fs::write(out.join("LICENCE.txt"), &licence).map_err(|e| e.to_string())?;
    let var = iid.replace(|c: char| !c.is_ascii_alphanumeric(), "_");
    let tsx = if pitched {
        let urls: Vec<String> = chosen.iter().map(|(_, n, m)| format!("    \"{}\": staticFile(\"{}\"),", note_name(m.unwrap(), false), static_path(&out.join(n)))).collect();
        format!("const {var} = new Tone.Sampler({{\n  urls: {{\n{}\n  }},\n  release: 1,\n}}).connect(Tone.buses.music);", urls.join("\n"))
    } else {
        let files: Vec<String> = chosen.iter().map(|(_, n, _)| format!("  staticFile(\"{}\"),", static_path(&out.join(n)))).collect();
        // a drum or a struck percussion is a kit's part, played in the music bed; anything else (a siren, a whistle)
        // is a sound effect on the picture
        let family = inst["family"].as_str().unwrap_or_default();
        let kit = family.contains("Struck Membranophones") || family.contains("Struck Idiophones");
        let (bus, other) = if kit { ("music", "Tone.buses.sfx for a one-off hit on the picture") } else { ("sfx", "Tone.buses.music when it plays in the score") };
        format!("// one Player per take; Tone.vary or alternating takes keeps repeats from sounding copied ({other})\nconst {var} = [\n{}\n].map((f) => new Tone.Player(f).connect(Tone.buses.{bus}));", files.join("\n"))
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
