//! Answers kept by what was asked (docs/ask.feature AK1b): the hash of everything the model reads (the state, the
//! questions as sent, the media), so the same questions on the same media cost nothing the second time. clef gives the
//! same answer to the same request, so a kept answer is the answer. Under RAEA_ASK_DIR, else ~/.raea/ask, in cache/.

use std::path::PathBuf;

use serde_json::{Map, Value};
use sha2::{Digest, Sha256};

use crate::client::Call;

/// raea ask's own folder: answers, kept media and this cache.
pub fn dir() -> PathBuf {
    raea_io_home::env_path("RAEA_ASK_DIR", ".raea/ask")
}

/// The request's hash.
pub fn key(call: &Call) -> String {
    let mut h = Sha256::new();
    h.update(call.clef_body().to_string().as_bytes());
    h.finalize().iter().map(|b| format!("{b:02x}")).collect()
}

pub fn get(key: &str) -> Option<Map<String, Value>> {
    let text = std::fs::read_to_string(dir().join("cache").join(format!("{key}.json"))).ok()?;
    serde_json::from_str(&text).ok()
}

/// Kept, unless nothing came back (an empty answer is never cached, so a fixed problem is asked again).
pub fn put(key: &str, answers: &Map<String, Value>) {
    if answers.is_empty() {
        return;
    }
    let d = dir().join("cache");
    if std::fs::create_dir_all(&d).is_ok() {
        let _ = std::fs::write(d.join(format!("{key}.json")), Value::Object(answers.clone()).to_string());
    }
}
