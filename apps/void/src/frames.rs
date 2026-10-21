//! Rendered frames in memory: an LRU under a byte budget (RAEA_VOID_FRAME_CACHE_MB, default 384), keyed by the edit's
//! version and the frame number, so a change to the program makes every old frame miss instead of showing stale
//! pictures. The webview loads frames from here through void://localhost/frame?k=<key> (main.rs), never from disk.

use std::collections::{HashMap, VecDeque};
use std::path::Path;
use std::sync::{Arc, Mutex};

/// (edit version, frame).
pub type Key = (u64, i64);

pub struct Cache {
    map: HashMap<Key, Arc<Vec<u8>>>,
    /// Least recently used first.
    order: VecDeque<Key>,
    bytes: usize,
    pub budget: usize,
    pub hits: u64,
    pub misses: u64,
}

pub static CACHE: Mutex<Option<Cache>> = Mutex::new(None);

fn with<R>(f: impl FnOnce(&mut Cache) -> R) -> R {
    let mut g = CACHE.lock().unwrap();
    let c = g.get_or_insert_with(|| Cache {
        map: HashMap::new(),
        order: VecDeque::new(),
        bytes: 0,
        budget: std::env::var("RAEA_VOID_FRAME_CACHE_MB").ok().and_then(|m| m.parse().ok()).unwrap_or(384usize) << 20,
        hits: 0,
        misses: 0,
    });
    f(c)
}

/// The edit's version: its path, size and modification time (rdb saves the whole file on every applied snippet).
pub fn version(edit: &Path) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    edit.hash(&mut h);
    if let Ok(m) = std::fs::metadata(edit) {
        m.len().hash(&mut h);
        m.modified().ok().hash(&mut h);
    }
    h.finish()
}

pub fn get(k: Key) -> Option<Arc<Vec<u8>>> {
    with(|c| match c.map.get(&k).cloned() {
        Some(v) => {
            c.hits += 1;
            if let Some(i) = c.order.iter().position(|o| *o == k) {
                c.order.remove(i);
            }
            c.order.push_back(k);
            Some(v)
        }
        None => {
            c.misses += 1;
            None
        }
    })
}

pub fn has(k: Key) -> bool {
    with(|c| c.map.contains_key(&k))
}

pub fn put(k: Key, png: Vec<u8>) {
    with(|c| {
        if let Some(old) = c.map.insert(k, Arc::new(png)) {
            c.bytes -= old.len();
            c.order.retain(|o| *o != k);
        }
        c.bytes += c.map[&k].len();
        c.order.push_back(k);
        while c.bytes > c.budget {
            let Some(old) = c.order.pop_front() else { break };
            if let Some(v) = c.map.remove(&old) {
                c.bytes -= v.len();
            }
        }
    })
}

/// Drop every frame not of `keep` (a version): after a change, or when memory runs short.
pub fn retain_version(keep: Option<u64>) {
    with(|c| {
        c.map.retain(|k, _| Some(k.0) == keep);
        c.order.retain(|k| Some(k.0) == keep);
        c.bytes = c.map.values().map(|v| v.len()).sum();
    })
}

/// (frames, bytes, budget, hits, misses).
pub fn stats() -> (usize, usize, usize, u64, u64) {
    with(|c| (c.map.len(), c.bytes, c.budget, c.hits, c.misses))
}

/// Bytes under a folder.
fn du(dir: &Path) -> u64 {
    std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| match e.file_type() {
            Ok(t) if t.is_dir() => du(&e.path()),
            _ => e.metadata().map(|m| m.len()).unwrap_or(0),
        })
        .sum()
}

/// Every cache void keeps, in a line each (`/cache`, the MCP `cache` tool).
pub fn report() -> String {
    let (n, bytes, budget, hits, misses) = stats();
    let models = raea_void::voice::models::root();
    let voices = raea_void::voice::Pocket::voice_dir().join(".state");
    format!(
        "frames in memory: {n} ({} of {} MB, {hits} hits, {misses} misses)\nmodels: {} MB in {}\nvoice states: {} MB in {}\nthis process: {} MB",
        bytes >> 20, budget >> 20, du(&models) >> 20, models.display(), du(&voices) >> 20, voices.display(),
        crate::mem::footprint() >> 20,
    )
}

/// Clears one cache: "frames" (memory), "previews" (videos, rendered again), "all" (both), "voices" (clip states, re-encoded on next use) or "models" (downloaded again
/// on next use).
pub fn clear(what: &str) -> anyhow::Result<String> {
    match what {
        "frames" | "" => retain_version(None),
        "previews" => {
            let _ = std::fs::remove_dir_all(crate::stage::previews_dir());
        }
        "all" => {
            retain_version(None);
            let _ = std::fs::remove_dir_all(crate::stage::previews_dir());
        }
        "voices" => {
            let _ = std::fs::remove_dir_all(raea_void::voice::Pocket::voice_dir().join(".state"));
        }
        "models" => {
            let root = raea_void::voice::models::root();
            anyhow::ensure!(root.ends_with("models"), "won't remove {}", root.display());
            std::fs::remove_dir_all(&root)?;
        }
        other => anyhow::bail!("no cache {other} (frames, previews, all, voices, models)"),
    }
    Ok(format!("cleared {}", if what.is_empty() { "frames" } else { what }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn evicts_least_recently_used_under_budget() {
        with(|c| c.budget = 30);
        put((1, 0), vec![0; 10]);
        put((1, 1), vec![0; 10]);
        put((1, 2), vec![0; 10]);
        assert!(get((1, 0)).is_some()); // 0 is now the most recent
        put((1, 3), vec![0; 10]);
        assert!(has((1, 0)) && !has((1, 1)) && has((1, 3)));
        retain_version(Some(2));
        assert_eq!(stats().0, 0);
    }
}
