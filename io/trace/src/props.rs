//! The variables that still override a component's props (docs/nobuild.feature NB1a): what was a build or a variable
//! is a prop now, and a variable left set wins over it, so a run says once which ones did.

use std::collections::BTreeSet;
use std::sync::Mutex;

/// The variables of `table` (each a variable and the prop it overrides) that are set, with their values. The first time
/// any is set in a run, stderr says which, over `component`'s props.
pub fn overrides(component: &str, table: &[(&'static str, &'static str)]) -> Vec<(&'static str, String, &'static str)> {
    static SAID: Mutex<BTreeSet<String>> = Mutex::new(BTreeSet::new());
    let set: Vec<_> = table.iter().filter_map(|&(n, prop)| std::env::var(n).ok().map(|v| (n, v, prop))).collect();
    if !set.is_empty() && SAID.lock().unwrap_or_else(|e| e.into_inner()).insert(component.to_owned()) {
        let list: Vec<String> = set.iter().map(|(n, v, prop)| format!("{n}={v} over {prop}")).collect();
        eprintln!("raea: <{component}> on every camera: {} (docs/nobuild.feature NB1a)", list.join(", "));
    }
    set
}
