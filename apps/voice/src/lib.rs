//! `raea voice`: speech to speech on this computer, for an agent that talks (the app's ideation session). Silero VAD
//! says when the person starts and stops speaking, Parakeet says what they said, and Pocket TTS says the agent's
//! words as they arrive, sentence by sentence. Nothing leaves the computer and nothing is paid for; the models are
//! fetched the first time (about 800 MB) into raea's model folder.
//!
//! `raea voice` is a session over lines of JSON (session.rs has them). `raea voice hear FILE.wav` and
//! `raea voice say "WORDS" -o OUT.wav` are one hearing and one saying, with how long each took.

use std::path::PathBuf;
use std::time::Instant;

use serde_json::json;

pub mod asr;
pub mod audio;
pub mod echo;
pub mod session;
pub mod speak;
pub mod turns;

pub const USAGE: &str = "raea voice [--voice NAME] [--duplex] [--over] [--pause MS] [--from IN.wav] [--to OUT.wav] [--room GAIN] [--fast] | raea voice hear FILE.wav | raea voice say \"WORDS\" -o OUT.wav [--voice NAME]";
pub const ABOUT: &str = "speech in and out on this computer, as lines of JSON on stdin and stdout: what raea.app's Ideate hears and speaks with. While it speaks it doesn't listen, unless --duplex (headphones: it listens all the time) or --over (speakers: its own voice is taken out of the microphone's sound, new and not yet tried across real rooms); --room GAIN is a simulated room for checking --over with files; hear writes a recording's words, say speaks words to a file";

/// A model's download told as it goes, once every 20 MB of each file.
pub fn fetching(emit: fn(serde_json::Value)) -> impl FnMut(&str, u64) {
    let mut last = (String::new(), 0u64);
    move |file: &str, bytes: u64| {
        let mb = bytes / 20_000_000 * 20;
        if last.0 != file || last.1 != mb {
            last = (file.to_string(), mb);
            emit(json!({ "fetch": { "file": file, "mb": mb } }));
        }
    }
}

/// The word after `name` on the command line.
fn flag(args: &[String], name: &str) -> Option<String> {
    args.windows(2).find(|w| w[0] == name).map(|w| w[1].clone())
}

fn quiet(_: serde_json::Value) {}

/// `raea voice hear FILE.wav`: what the file says, with how long loading and hearing took.
fn hear(file: &str) -> anyhow::Result<String> {
    let (x, rate) = audio::read_wav(std::path::Path::new(file))?;
    let mut heard = Vec::with_capacity(x.len());
    audio::Resample::new(rate, turns::RATE as u32).push(&x, &mut heard);
    let t = Instant::now();
    let mut asr = asr::Asr::load(&raea_sys_ml::models::ensure(&asr::SPEC, &mut fetching(quiet))?)?;
    let load_ms = t.elapsed().as_millis() as u64;
    let t = Instant::now();
    let text = asr.hear(&heard)?;
    Ok(json!({ "text": text, "seconds": heard.len() as f64 / turns::RATE as f64, "load_ms": load_ms, "ms": t.elapsed().as_millis() as u64 }).to_string())
}

/// `raea voice say "WORDS" -o OUT.wav`: the words said into a file, with how long the first sound and the whole took.
fn say(text: &str, out: &str, voice: Option<String>) -> anyhow::Result<String> {
    use raea_sys_speech::pocket::{Out, Pocket};
    let t = Instant::now();
    let mut pocket = Pocket::load(&raea_sys_ml::models::ensure(&Pocket::SPEC, &mut fetching(quiet))?)?;
    // Pocket's stock voice, as a session's: never the clip the library's own default takes from the person's voice folder
    let voice = voice.unwrap_or_else(|| Pocket::VOICES[0].to_string());
    pocket.begin("", &voice, 1, 0.7)?;
    let load_ms = t.elapsed().as_millis() as u64;
    let t = Instant::now();
    let (mut sound, mut first_ms) = (Vec::new(), None);
    let mut g = pocket.begin(text, &voice, 7, 0.7)?;
    while let Some(o) = pocket.step(&mut g)? {
        if let Out::Audio(a) = o {
            first_ms.get_or_insert(t.elapsed().as_millis() as u64);
            sound.extend(a);
        }
    }
    let ms = t.elapsed().as_millis() as u64;
    audio::write_wav(std::path::Path::new(out), &sound, speak::RATE)?;
    Ok(json!({ "voice": voice, "seconds": sound.len() as f64 / speak::RATE as f64, "load_ms": load_ms, "first_ms": first_ms, "ms": ms }).to_string())
}

pub fn cli(args: &[String]) -> Result<String, String> {
    let said = |e: anyhow::Error| format!("raea voice: {e:#}");
    match args.get(1).map(String::as_str) {
        Some("hear") => hear(args.get(2).ok_or(USAGE)?).map_err(said),
        Some("say") => say(args.get(2).ok_or(USAGE)?, &flag(args, "-o").ok_or(USAGE)?, flag(args, "--voice")).map_err(said),
        Some("help" | "--help" | "-h") => Ok(format!("usage: {USAGE}\n({ABOUT})")),
        Some(a) if !a.starts_with("--") => Err(USAGE.into()),
        _ => {
            let pause_ms = match flag(args, "--pause") {
                Some(p) => p.parse().map_err(|_| "--pause takes milliseconds".to_string())?,
                None => 700,
            };
            let o = session::Opts {
                from: flag(args, "--from").map(PathBuf::from),
                to: flag(args, "--to").map(PathBuf::from),
                voice: flag(args, "--voice"),
                duplex: args.iter().any(|a| a == "--duplex"),
                over: args.iter().any(|a| a == "--over"),
                room: match flag(args, "--room") {
                    Some(g) => Some(g.parse().map_err(|_| "--room takes a gain, like 0.5".to_string())?),
                    None => None,
                },
                fast: args.iter().any(|a| a == "--fast"),
                pause_ms,
            };
            session::run(o).map(|_| String::new()).map_err(|e| {
                session::emit(json!({ "error": format!("{e:#}") }));
                said(e)
            })
        }
    }
}
