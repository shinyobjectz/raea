//! Every answer kept (docs/ask.feature AK3), so a question can be labelled, calibrated and reworded later (AK6, AK7):
//! each call in calls.jsonl (the file, the window, the state, the clips it sent by hash, the route and the cost) and each
//! answer in answers.jsonl (the call, the question and its version, the reading). The clips themselves are kept by hash
//! in media/, at most 2 GB, the oldest out first, so new words can be asked of the same media. A call keeps what its
//! file was (`ident`), so a note on the render's copy in the feed labels it (labels.rs), and an answer keeps its whole
//! question, so a verdict reads in its terms and `raea ask try` can reword it.

use std::io::Write as _;
use std::path::{Path, PathBuf};

use raea_core_ask::{Question, Reading};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

/// How much cut media is kept.
pub const MEDIA_CAP: u64 = 2 << 30;

pub fn dir() -> PathBuf {
    raea_io_clef::cache::dir()
}

/// A new id: a prefix and twelve hex digits, from the time, the process and a counter.
pub fn new_id(prefix: &str) -> String {
    static N: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let t = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0);
    let seed = format!("{t}-{}-{}", std::process::id(), N.fetch_add(1, std::sync::atomic::Ordering::Relaxed));
    let h = seed.bytes().fold(0xcbf2_9ce4_8422_2325u64, |h, b| (h ^ b as u64).wrapping_mul(0x0100_0000_01b3));
    format!("{prefix}-{:012x}", h & 0xffff_ffff_ffff)
}

pub fn now() -> String {
    let s = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    s.to_string()
}

fn append(file: &str, row: &Value) {
    let d = dir();
    if std::fs::create_dir_all(&d).is_err() {
        return;
    }
    // one write a row: several runs (a script's, two at once) append to the same file, and a row written in pieces
    // could be split by another's
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(d.join(file)) {
        let _ = f.write_all(format!("{row}\n").as_bytes());
    }
}

fn hex(digest: &[u8]) -> String {
    digest.iter().take(16).map(|b| format!("{b:02x}")).collect()
}

/// What a file is, cheaply, and the size an ident names (raea-core-ask, so `raea make status` reads the same).
pub use raea_core_ask::ident::{ident, ident_len};

/// One of the record's files written whole, through a temporary file, so a reader never sees half of it.
pub fn write_whole(file: &str, text: &str) -> Result<(), String> {
    let d = dir();
    let tmp = d.join(format!("{file}.{}.tmp", std::process::id()));
    let put = || -> std::io::Result<()> {
        std::fs::create_dir_all(&d)?;
        std::fs::write(&tmp, text)?;
        std::fs::rename(&tmp, d.join(file))
    };
    put().map_err(|e| format!("{}: {e}", d.display()))
}

/// A clip kept by its hash, as `HASH.EXT` (already kept is left alone).
pub fn keep_media(bytes: &[u8], ext: &str) -> String {
    let name = format!("{}.{ext}", hex(&Sha256::digest(bytes)));
    let d = dir().join("media");
    let p = d.join(&name);
    if !p.is_file() && std::fs::create_dir_all(&d).is_ok() {
        let _ = std::fs::write(&p, bytes);
        prune(MEDIA_CAP);
    }
    name
}

/// A kept clip's bytes, by its name.
pub fn media(name: &str) -> Option<Vec<u8>> {
    std::fs::read(dir().join("media").join(name)).ok()
}

/// The kept media held to `cap` bytes, the least recently written going first.
pub fn prune(cap: u64) {
    let Ok(rd) = std::fs::read_dir(dir().join("media")) else { return };
    let mut files: Vec<(std::time::SystemTime, u64, PathBuf)> = rd
        .flatten()
        .filter_map(|e| e.metadata().ok().and_then(|m| Some((m.modified().ok()?, m.len(), e.path()))))
        .collect();
    let mut total: u64 = files.iter().map(|f| f.1).sum();
    files.sort();
    for (_, len, p) in files {
        if total <= cap {
            break;
        }
        if std::fs::remove_file(&p).is_ok() {
            total -= len;
        }
    }
}

/// One call kept; its id.
#[allow(clippy::too_many_arguments)]
pub fn call(file: &str, project: Option<&str>, from: f64, to: Option<f64>, state: &str, media: &[String], route: &str, cost: f64, cached: bool) -> String {
    let id = new_id("c");
    let row = json!({
        "id": id, "at": now(), "file": file, "ident": ident(Path::new(file)), "project": project, "from": from, "to": to, "state": state,
        "media": media, "route": route, "cost": cost, "cached": cached,
    });
    append("calls.jsonl", &row);
    id
}

/// One answer kept; its id.
pub fn answer(call: &str, file: &str, q: &Question, r: &Reading, flagged: bool) -> String {
    let id = new_id("a");
    let whole = serde_json::to_value(Question { calibration: None, ..q.clone() }).unwrap_or_default();
    let row = json!({
        "id": id, "at": now(), "call": call, "file": file, "question": q.id, "version": q.version(), "ask": q.ask,
        "value": r.value, "problem": r.problem, "choice": r.choice, "says": r.says, "flagged": flagged, "calibrated": q.calibrated(),
        "q": whole,
    });
    append("answers.jsonl", &row);
    id
}

/// Every row of one of the record's files.
pub fn rows(file: &str) -> Vec<Value> {
    std::fs::read_to_string(dir().join(file)).unwrap_or_default().lines().filter_map(|l| serde_json::from_str(l).ok()).collect()
}
