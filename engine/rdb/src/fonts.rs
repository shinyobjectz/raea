//! Fonts an edit names that this machine doesn't have. A missing family falls back silently (an agent's "Inter, 900"
//! came out as regular Helvetica), so apply says so and names families that are here.

use std::collections::BTreeSet;
use std::sync::{Mutex, OnceLock};

use fontique::{Collection, CollectionOptions};

use crate::eval::Tup;
use crate::ir::V;

/// Display families worth offering, in order, when they're installed.
const OFFER: &[&str] = &["Avenir Next", "Helvetica Neue", "Futura", "Gill Sans", "Montserrat", "Poppins", "Bebas Neue", "Arial Black",
    "Impact", "Georgia", "Didot", "Baskerville", "American Typewriter", "Menlo", "Rockwell", "Chalkboard SE"];

fn collection() -> &'static Mutex<Collection> {
    static C: OnceLock<Mutex<Collection>> = OnceLock::new();
    C.get_or_init(|| Mutex::new(Collection::new(CollectionOptions { system_fonts: true, ..Default::default() })))
}

pub fn installed(family: &str) -> bool {
    let generic = ["serif", "sans-serif", "monospace", "system-ui", "cursive", "fantasy"];
    generic.contains(&family.to_ascii_lowercase().as_str()) || collection().lock().unwrap().family_id(family).is_some()
}

/// The families that are here, from OFFER.
pub fn offered() -> Vec<&'static str> {
    OFFER.iter().copied().filter(|f| installed(f)).collect()
}

/// The text rows' font families (the first of a CSS list that's here counts) that aren't installed.
pub fn missing(rows: &[Tup]) -> Vec<String> {
    let mut out = BTreeSet::new();
    for t in rows {
        let Some(V::S(f)) = t.vals.get("font") else { continue };
        let names: Vec<String> = f.split(',').map(|n| n.trim().trim_matches(['"', '\'']).to_string()).filter(|n| !n.is_empty()).collect();
        if !names.is_empty() && !names.iter().any(|n| installed(n)) {
            out.insert(names[0].clone());
        }
    }
    out.into_iter().collect()
}

/// The warning apply prints, if any.
pub fn warning(rows: &[Tup]) -> Option<String> {
    let m = missing(rows);
    if m.is_empty() {
        return None;
    }
    Some(format!("warning: font {} isn't on this machine (it would fall back to Helvetica, at the wrong weight); fonts here: {}",
        m.iter().map(|f| format!("'{f}'")).collect::<Vec<_>>().join(", "), offered().join(", ")))
}
