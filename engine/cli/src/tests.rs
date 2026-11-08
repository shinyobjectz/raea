#![cfg(test)]
use super::*;
use crate::cli::*;

#[test]
fn shell_runs() {
    // engine/cli's handler loads and answers on raea's JavaScriptCore (part list is the shell's own, no host step);
    // `raea help` lists SHELL's verbs (help_lists_every_verb)
    let parts = shell(&["part".to_string(), "list".to_string()]).unwrap();
    assert!(parts.contains("captions"), "{parts}");
}

#[test]
fn help_lists_every_verb() {
    // the verbs run() dispatches (`Some("v") => return`, `== Some("v")`, `matches!(.., Some("a" | "b"))`, SHELL),
    // each with a line in `raea help`; a verb's flags come from its own USAGE, so a line can't drift from it
    let help = run(&["help".to_string()]).unwrap();
    let src = include_str!("cli.rs");
    let body = &src[src.find("\npub fn run(").unwrap()..];
    let pat = regex::Regex::new(r#"(?:Some\("([a-z][a-z-]*)"\) => return|== Some\("([a-z][a-z-]*)"\)|Some\(("[a-z][a-z-]*"(?: \| "[a-z][a-z-]*")*)\)\))"#).unwrap();
    let mut verbs: Vec<String> = SHELL.iter().map(|v| v.to_string()).collect();
    for c in pat.captures_iter(body) {
        let m = c.get(1).or(c.get(2)).or(c.get(3)).unwrap().as_str();
        verbs.extend(m.split(" | ").map(|v| v.trim_matches('"').to_string()));
    }
    assert!(verbs.len() > 20, "{verbs:?}");
    // not the agent's: what raea runs for itself or for raea's own development
    let unlisted = [
        "master-gain", // live play asks a child process for the mix's gain (play.rs)
        "types", "test", // the native calls' TypeScript and conformance tests on V8, for raea's own work
    ];
    let listed = |v: &str| [format!("raea {v} "), format!("raea {v}\n"), format!("| {v} ")].iter().any(|l| help.contains(l.as_str()));
    let missing: Vec<_> = verbs.iter().filter(|v| !listed(v) && !unlisted.contains(&v.as_str())).collect();
    assert!(missing.is_empty(), "not in raea help: {missing:?}");
    for usage in [raea_app_check::USAGE, raea_app_game::USAGE, raea_runtime_export::SCENE_USAGE, motion::physics::USAGE, tools::splat::USAGE, tools::sound::SAO_USAGE, tools::sound::TOKENSYNTH_USAGE, tools::song::USAGE, motion::track::USAGE, raea_app_solid::USAGE, raea_app_judge::USAGE, tools::blender::USAGE] {
        assert!(help.contains(usage), "{usage}");
    }
}

#[test]
fn help_json_is_every_listed_verb() {
    // `raea help --json` (tools/skills.sh gen's source): the verbs the help lists, not its prose, each with its flags
    let help = run(&["help".to_string()]).unwrap();
    let j: serde_json::Value = serde_json::from_str(&run(&["help".to_string(), "--json".to_string()]).unwrap()).unwrap();
    let verbs: Vec<&str> = j["verbs"].as_array().unwrap().iter().map(|v| v["verb"].as_str().unwrap()).collect();
    assert_eq!(verbs, verb_list(&help).iter().map(String::as_str).collect::<Vec<_>>());
    assert!(!verbs.contains(&"compositions"), "the help's opening line isn't a verb: {verbs:?}");
    let controller = j["verbs"].as_array().unwrap().iter().find(|v| v["verb"] == "controller").unwrap();
    assert_eq!(controller["args"], 3);
    assert!(controller["flags"].as_array().unwrap().iter().any(|f| f["flag"] == "--by" && f["values"] == 1));
    assert!(j["verbs"].as_array().unwrap().iter().any(|v| v["verb"] == "gen" && v["own_parser"] == true));
}

#[test]
fn a_verb_refuses_what_it_doesnt_take() {
    let help = run(&["help".to_string()]).unwrap();
    let a = |s: &[&str]| s.iter().map(|a| a.to_string()).collect::<Vec<_>>();
    // what raea's own callers run (tools/, apps/, sdk/, the guides and skills): all taken
    for ok in [
        &["render", "e.tsx", "-o", "o.mp4", "--frame", "20", "--input", "i.json", "--manifest", "m.json", "--stage", "1080x1920", "--words", "w.json"][..],
        &["render", "e.tsx", "-o", "d", "--at", "0,30", "--composition", "Main", "--w", "1080", "--h", "1920", "--fps", "30", "--frames", "90", "--lite"],
        &["render", "e.tsx", "-o", "o.mp4", "--kbps", "-6"],
        &["render", "--bundle", "a.comp.js", "-o", "o.png", "--dir", "d", "--frame", "3"],
        &["play", "g.tsx", "--input-script", "k.json", "--input-log", "l.json", "--offscreen", "--seconds", "end", "--size", "540x960", "--snapshot", "1.5", "a.png", "--seek", "30", "b.png", "--json", "p.json"],
        &["play", "g.tsx", "--input", "--baked", "--scrub", "8", "--autoplay", "--no-vsync", "--master", "--3d"],
        &["export-game", "g.tsx", "--keys", "arrows", "-o", "G.app"],
        &["sounds", "get", "vsco/piano", "--all", "-o", "d"],
        &["song", "s.wav", "--plot", "p.png", "--backend", "x"],
        &["cut", "take.mp4", "one line", "another", "--append"],
        &["part", "add", "captions", "face-frame", "--force"],
        &["mezz", "--check", "a.mov", "b.mov"],
        &["traces", "show", "last"],
        &["media", "ffmpeg", "-i", "x", "--anything"],
        &["physics", "bake", "r.json", "--whatever"],
        &["test", "--filter", "x"], &["solid", "export", "b.ts", "-o", "b.glb", "--set", "w=1", "--part", "top", "--colour", "#8a6a4a"],
    ] {
        assert_eq!(stray(&help, &a(ok)), None, "{ok:?}");
    }
    // zsh's "--frame 20", one word: refused, named, with the verb's usage
    let why = stray(&help, &a(&["render", "e.tsx", "-o", "o.mp4", "--frame 20"])).unwrap();
    assert!(why.starts_with("raea render doesn't take \"--frame 20\"") && why.contains("\nusage: raea render EDIT.tsx"), "{why}");
    let why = stray(&help, &a(&["render", "e.tsx", "f.tsx", "-o", "o.mp4"])).unwrap();
    assert!(why.contains("\"f.tsx\": it takes 1 argument besides its flags"), "{why}");
    assert!(stray(&help, &a(&["play", "g.tsx", "--full"])).unwrap().starts_with("raea play doesn't take \"--full\""));
    assert!(stray(&help, &a(&["track", "c.mp4", "--setup"])).is_some());
    assert!(stray(&help, &a(&["splat", "train", "v.mp4", "-o", "o.ply", "--steps", "100"])).is_none());
}

#[test]
fn every_flag_a_verb_reads_is_in_the_help() {
    // the ratchet under the refusal: a flag the code reads (flag(args, ..), == or position over args, a match arm,
    // starts_with) that the help doesn't name would be refused before the code saw it. Every "--flag" literal in
    // the verbs' sources counts; the ones that aren't raea's flags are listed with why
    let help = run(&["help".to_string()]).unwrap();
    let mut named: std::collections::HashSet<String> = std::collections::HashSet::new();
    for line in help.lines() {
        let v = line.trim().trim_start_matches("usage:").trim().split_whitespace().nth(1).unwrap_or("");
        if let Some(u) = usage_of(&help, v) {
            named.extend(u.flags.into_keys());
        }
    }
    let not_ours = [
        "--sign", "--timestamp", "--retry", "--uuid", "--out", // what raea passes to codesign, curl, xcrun and raea-perceive
        "--game", "--info", "--selftest", "--no-pointer", // an exported game's own player reads these (game.rs player_main)
        "--o", // track and splat take "--o" for -o, a slip they forgive
        "--nope", "--loud", // test fixtures
        "--help", "--version", // raea's own, before any verb
        "--filter", "--bundle-only", "--define", "--alias", "--node-stub", // `raea test`, raea's own conformance runner, unlisted
    ];
    let lit = regex::Regex::new(r#""(--[a-z0-9][a-z0-9-]*)[="]"#).unwrap();
    let src = include_str!("cli.rs");
    let run_body = &src[src.find("\npub fn run(").unwrap()..];
    let motion = include_str!("../../../runtime/export/src/render.rs");
    let render_body = &motion[motion.find("\npub fn render(").unwrap()..motion.find("\n/// A bundled composition rendered").unwrap()];
    let files = [
        ("cli.rs", run_body),
        ("runtime/export render.rs", render_body),
        ("apps/play lib.rs", include_str!("../../../apps/play/src/lib.rs")),
        ("apps/play run.rs", include_str!("../../../apps/play/src/run.rs")),
        ("apps/play offscreen.rs", include_str!("../../../apps/play/src/offscreen.rs")),
        ("runtime/live lib.rs", include_str!("../../../runtime/live/src/lib.rs")),
        ("runtime/live open.rs", include_str!("../../../runtime/live/src/open.rs")),
        ("runtime/live sound.rs", include_str!("../../../runtime/live/src/sound.rs")),
        ("runtime/live draw.rs", include_str!("../../../runtime/live/src/draw.rs")),
        ("apps/game lib.rs", include_str!("../../../apps/game/src/lib.rs")),
        ("apps/game export.rs", include_str!("../../../apps/game/src/export.rs")),
        ("apps/game player.rs", include_str!("../../../apps/game/src/player.rs")),
        ("apps/tools song.rs", include_str!("../../../apps/tools/src/song.rs")),
        ("motion/track.rs", include_str!("../../../runtime/react/src/scene/track.rs")),
        ("apps/tools splat.rs", include_str!("../../../apps/tools/src/splat.rs")),
        ("apps/tools sound.rs", include_str!("../../../apps/tools/src/sound.rs")),
        ("runtime/export scene.rs", include_str!("../../../runtime/export/src/scene.rs")),
    ];
    let mut missing = Vec::new();
    for (file, text) in files {
        for c in lit.captures_iter(text) {
            let f = &c[1];
            if !named.contains(f) && !not_ours.contains(&f) {
                missing.push(format!("{file}: {f}"));
            }
        }
    }
    missing.dedup();
    assert!(missing.is_empty(), "flags the code reads that raea help doesn't name (add them to the verb's line, or to its (also: ...)): {missing:?}");
}

#[test]
fn the_release_ships_every_setup_script() {
    // an installed raea finds a local model's setup.sh under share/raea/tools (raea_io_assets::setup_script):
    // each model named there must be in what tools/release.sh archives, or <Goo>, `raea splat` and <Matchmove>
    // work in a checkout and fail installed
    let release = include_str!("../../../tools/release.sh");
    let pat = regex::Regex::new(r#"setup_script\("([a-z]+)"\)"#).unwrap();
    let mut missing = Vec::new();
    for src in [
        include_str!("../../../io/assets/src/lib.rs"),
        include_str!("../../../io/assets/src/splat.rs"),
        include_str!("../../../runtime/react/src/scene/physics.rs"),
        include_str!("../../../apps/tools/src/sound.rs"),
    ] {
        for c in pat.captures_iter(src) {
            if !release.contains(&format!(" tools/{} ", &c[1])) {
                missing.push(c[1].to_string());
            }
        }
    }
    // sound.rs names its models through a variable: sao and tokensynth
    for m in ["sao", "tokensynth"] {
        if !release.contains(&format!(" tools/{m} ")) {
            missing.push(m.to_string());
        }
    }
    assert!(missing.is_empty(), "tools/release.sh doesn't ship these setup scripts: {missing:?}");
}

#[test]
fn guide_names() {
    // a path under sdk/guides/craft, then modes/, verbs/, motion/; the 3D guides with the shell's note
    assert!(guide(Some("bevy/games")).unwrap().starts_with(GUIDE_NOTE));
    assert!(guide(Some("motion/music")).unwrap() == guide(Some("music")).unwrap());
    assert!(!guide(Some("craft")).unwrap().starts_with(GUIDE_NOTE));
    for n in ["router", "rubric", "pitch", "quieter", "bevy", "modes/play", "coverage"] {
        assert!(guide(Some(n)).is_ok(), "{n}");
    }
    assert!(guide(Some("../craft/floor")).is_err());
    assert!(guide(None).unwrap().contains("bevy/games"));
}

#[test]
fn the_skill_states_the_core_rules_as_motion_md_does() {
    // motion.md's numbered core rules are the one text; SKILL.md points at them and repeats the must-know ones
    // under their own titles, naming only what motion.md's rules name
    let motion = include_str!("../../../sdk/guides/craft/motion.md");
    let core = motion.split("## The core rules").nth(1).unwrap().split("\n## ").next().unwrap();
    let titles: Vec<&str> = core.lines().filter_map(|l| l.split_once(". **").filter(|(n, _)| n.trim().parse::<u32>().is_ok())).filter_map(|(_, t)| t.split_once("**").map(|(t, _)| t)).collect();
    assert_eq!(titles.len(), 10, "{titles:?}");
    let skill = &std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../../skills/skills/raea/SKILL.md")).expect("raea-skills: git submodule update --init skills");
    let rules = skill.split("Rules that keep renders right").nth(1).unwrap().split("\n\n").next().unwrap();
    assert!(rules.contains("`raea guide motion` has all ten"), "{rules}");
    let mut bullets: Vec<String> = Vec::new();
    for l in rules.lines() {
        if l.starts_with("- ") {
            bullets.push(l.to_string());
        } else if let Some(b) = bullets.last_mut() {
            b.push(' ');
            b.push_str(l.trim());
        }
    }
    let held: Vec<&String> = bullets.iter().filter(|b| b.starts_with("- **")).collect();
    assert!(held.len() >= 3, "{bullets:?}");
    for b in held {
        let title = b[4..].split_once("**").unwrap().0;
        assert!(titles.contains(&title), "SKILL.md's rule {title:?} isn't one of motion.md's core rules {titles:?}");
        for code in b.split('`').skip(1).step_by(2) {
            assert!(core.contains(code), "SKILL.md's {title:?} names `{code}`, which motion.md's core rules don't");
        }
    }
}
