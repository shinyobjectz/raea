//! Which of the plugin catalog's plugins an Engine adds (docs/nobuild.feature NB4a). A raea built with the catalog has
//! every plugin engine/bevy-plugins declares compiled in, and an Engine adds only the ones its project chooses: those
//! the nearest package.json at or above the edit's folder names under raea.plugins (a list of names, or declarations
//! keyed by name; raea.bevy.plugins, its first name, too), those a part's components or resources come from (a type
//! under a plugin's crate), and the plugins each of them needs. So two that conflict (avian and rapier3d) never meet in
//! one App unless a scene asks for both. A registered plugin outside the catalog (raea's own) is in every App, and a
//! build without the catalog adds every plugin it registered, as before.
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};

use bevy::prelude::*;
use raea_core_program::ir::{Program, V};
use serde_json::{Value, json};

/// One plugin of the catalog: its name, the ones added with it, and the type-path prefixes its types live under.
#[derive(Clone, Copy, Debug)]
pub struct Entry {
    pub name: &'static str,
    pub needs: &'static [&'static str],
    pub prefixes: &'static [&'static str],
}

static CATALOG: Mutex<Vec<Entry>> = Mutex::new(Vec::new());
static EVERY: AtomicBool = AtomicBool::new(false);

/// The catalog this build has: every declared plugin it compiled in (raea-bevy-plugins' install_all).
pub fn catalog(entries: Vec<Entry>) {
    *CATALOG.lock().unwrap() = entries;
}

/// Every compiled plugin in every App from now on (`on`), chosen or not: the generator walks all of their types
/// (raea-bevy-sdk); off again, only the chosen ones.
pub fn choose_every(on: bool) {
    EVERY.store(on, Ordering::Relaxed);
}

/// The catalog's plugins an Engine adds, by name.
#[derive(Resource, Clone, Debug, Default, PartialEq)]
pub struct Chosen {
    pub names: Vec<&'static str>,
}

impl Chosen {
    /// Whether a registered plugin goes in: raea's own always, a catalog plugin's (or one of its halves, "bevy_audio"
    /// of "bevy") when its plugin is chosen.
    pub fn adds(&self, registered: &str) -> bool {
        EVERY.load(Ordering::Relaxed) || owner(&CATALOG.lock().unwrap(), registered).is_none_or(|e| self.names.contains(&e.name))
    }
}

fn owner<'a>(cat: &'a [Entry], registered: &str) -> Option<&'a Entry> {
    cat.iter().filter(|e| registered.strip_prefix(e.name).is_some_and(|r| r.is_empty() || r.starts_with('_'))).max_by_key(|e| e.name.len())
}

/// The package.json that chooses a project's plugins (the nearest at or above `dir` naming any) and the names it gives,
/// leaving out raea's own (a declaration with `"raea": true`, which every build has).
pub fn named(dir: &Path) -> Option<(PathBuf, Vec<String>)> {
    for d in dir.ancestors() {
        let pkg = d.join("package.json");
        let Some(v) = std::fs::read_to_string(&pkg).ok().and_then(|t| serde_json::from_str::<Value>(&t).ok()) else { continue };
        let mut names: Vec<String> = Vec::new();
        let mut any = false;
        for p in [&v["raea"]["plugins"], &v["raea"]["bevy"]["plugins"]] {
            match p {
                Value::Array(a) => names.extend(a.iter().filter_map(Value::as_str).map(String::from)),
                Value::Object(o) => names.extend(o.iter().filter(|(_, s)| s["raea"] != true).map(|(k, _)| k.clone())),
                _ => continue,
            }
            any |= p.as_array().map_or(0, Vec::len) + p.as_object().map_or(0, |o| o.len()) > 0;
        }
        if any {
            names.dedup();
            return Some((pkg, names));
        }
    }
    None
}

/// Whether `s` names a type under `prefix` (at the start of a path, not inside another crate's name).
fn mentions(s: &str, prefix: &str) -> bool {
    // what comes before a crate's name inside a longer one: a letter, a digit, `_`, or the `::` of a path above it
    let inside = |c: char| c.is_alphanumeric() || c == '_' || c == ':';
    s.match_indices(prefix).any(|(i, _)| !s[..i].ends_with(inside))
}

/// The plugins an Engine opened on `program` in `dir` adds. Said once on stderr when the project names one this
/// build lacks; written to the run's trace (`plugins_chosen`) each time.
pub fn choose(program: &Program, dir: &Path) -> Chosen {
    let cat = CATALOG.lock().unwrap().clone();
    let mut chosen = Chosen::default();
    let mut why: Vec<Value> = Vec::new();
    let mut add = |e: &Entry, because: String, chosen: &mut Chosen| {
        if !chosen.names.contains(&e.name) {
            chosen.names.push(e.name);
            why.push(json!({ "plugin": e.name, "because": because }));
        }
    };
    let named = named(dir);
    let mut lacks = Vec::new();
    for n in named.iter().flat_map(|(_, l)| l) {
        match cat.iter().find(|e| e.name == n) {
            Some(e) => add(e, "package.json".into(), &mut chosen),
            None => lacks.push(n.clone()),
        }
    }
    // the types the scene's rows name (a component's or resource's `type`, or inline in a value as JSON)
    let paths: Vec<&str> = program.spawns.iter().flat_map(|(_, rows)| rows.iter().flat_map(|r| r.values())).filter_map(|v| if let V::S(s) = v { Some(s.as_str()) } else { None }).filter(|s| s.contains("::")).collect();
    // "bevy" is Bevy's own features: its prefixes are the crates every scene's types come from
    for e in cat.iter().filter(|e| e.name != "bevy") {
        if let Some(p) = e.prefixes.iter().find(|p| paths.iter().any(|s| mentions(s, p))) {
            add(e, format!("a type under {p}"), &mut chosen);
        }
    }
    let mut i = 0;
    while i < chosen.names.len() {
        let by = chosen.names[i];
        for n in cat.iter().find(|e| e.name == by).map_or(&[][..], |e| e.needs) {
            if let Some(e) = cat.iter().find(|e| e.name == *n) {
                add(e, format!("{by} needs it"), &mut chosen);
            }
        }
        i += 1;
    }
    if let (false, Some((pkg, _))) = (lacks.is_empty(), &named) {
        static SAID: std::sync::Once = std::sync::Once::new();
        SAID.call_once(|| {
            let has = if cat.is_empty() { "this raea has no Bevy plugins compiled in".to_string() } else { format!("this raea has {}", cat.iter().map(|e| e.name).collect::<Vec<_>>().join(", ")) };
            eprintln!("raea: {} names {}, which this raea doesn't have ({has}; docs/nobuild.feature NB4b)", pkg.display(), lacks.join(", "));
        });
    }
    raea_io_trace::event("plugins_chosen", json!({ "package": named.as_ref().map(|(p, _)| p.display().to_string()), "chosen": why, "lacks": lacks, "catalog": cat.len() }));
    chosen
}
