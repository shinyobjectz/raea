//! raea's and Bevy's own shaders read from files when an Engine opens, in place of the ones compiled into this binary
//! (docs/nobuild.feature NB5b), so trying a change to one needs no build:
//! - a project's `engine-shaders/` folder (beside the edit, or up to three folders above it) mirrors the engine's
//!   embedded paths, `<crate>/<path under its src>`: `engine-shaders/bevy_solari/realtime/resolve_dlss_rr_textures.wesl`
//!   replaces Solari's. raea says which, once, since such a file is tied to this raea's version of the shader. An empty
//!   file there is filled with the shader this raea draws with, the copy to start from (an author has no sources);
//! - RAEA_ENGINE_SHADERS=DIR, a raea checkout: every embedded shader whose source is in DIR's git tree is read from
//!   that file, so a session editing an engine shader renders it at once (raea says how many differ from the binary's).
//!
//! A shader a crate passes to wgpu itself (include_str!, not one of Bevy's embedded assets) is read the same way when
//! the crate asks through `text` and names it with `declare` (raea-render-raytrace's); the rest of those (filters,
//! three-shader) don't yet.
//!
//! A shader its plugin loaded as it was built (load_shader_library!) is swapped in Assets<Shader> as its first load
//! lands, in the same update, so no frame draws with the binary's (an AssetServer::reload would land some frames later
//! and recompile every pipeline using it mid-render).
use bevy::asset::io::embedded::EmbeddedAssetRegistry;
use bevy::prelude::*;
use bevy::shader::{Shader, Source};
use raea_core_program::ir::Program;

// <ShaderConstants>: a const in an engine shader set from TypeScript when the Engine opens (NB1a)
pub mod consts;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// Replaces the embedded shaders a project's engine-shaders folder or RAEA_ENGINE_SHADERS has files for, then sets the
/// constants the program's first frame asks for (consts.rs). Call once every plugin is built, before app.finish.
pub fn shadow(app: &mut App, edit_dir: &Path, prog: &Program) {
    let mut done: Vec<String> = Vec::new();
    *PROJECT.lock().unwrap_or_else(std::sync::PoisonError::into_inner) = project_dir(edit_dir);
    let mut swaps = Vec::new();
    if let Some(dir) = project_dir(edit_dir) {
        let (mut ours, mut stray, mut filled) = (Vec::new(), Vec::new(), Vec::new());
        let declared = DECLARED.lock().unwrap_or_else(std::sync::PoisonError::into_inner).clone();
        for (rel, file) in walk(&dir) {
            if std::fs::metadata(&file).is_ok_and(|m| m.len() == 0) {
                let builtin = declared.get(rel.as_str()).map(|t| t.as_bytes().to_vec());
                let Some(text) = builtin.or_else(|| original(app, &rel)) else {
                    stray.push(rel);
                    continue;
                };
                match std::fs::write(&file, text) {
                    Ok(()) => filled.push(rel),
                    Err(e) => eprintln!("raea: {}: {e}", file.display()),
                }
                continue;
            }
            let fork = raea_sys_frame::bevy_api::swapped(&rel);
            match replace(app, &rel, &file, &mut swaps) {
                Some(_) => ours.push(if fork { format!("{rel} (over raea's fork of Bevy's)") } else { rel }),
                None if declared.contains_key(rel.as_str()) => ours.push(format!("{rel} (read as its pass is made)")),
                None => stray.push(rel),
            }
        }
        if !ours.is_empty() {
            eprintln!("raea: {} of the engine's shaders from {} in place of this raea's ({}); each is tied to this raea's version of it (docs/nobuild.feature NB5b)", ours.len(), dir.display(), ours.join(", "));
        }
        // the bare paths, without what was said of each
        for r in &mut ours {
            r.truncate(r.find(" (").unwrap_or(r.len()));
        }
        if !stray.is_empty() {
            eprintln!("raea: {} has no engine shader at {} (the path is <crate>/<path under its src>, as Bevy embeds it)", dir.display(), stray.join(", "));
        }
        if !filled.is_empty() {
            eprintln!("raea: filled {} empty file(s) in {} with the shader this raea draws with ({}); this render used them as they were, so change them and render again", filled.len(), dir.display(), filled.join(", "));
        }
        done = ours;
    }
    if let Some(root) = std::env::var_os("RAEA_ENGINE_SHADERS").map(PathBuf::from) {
        from_tree(app, &root, &done, &mut swaps);
    }
    app.register_type::<consts::ShaderConstants>();
    let set = consts::gather(prog);
    let constants = with_constants(app, &set, &mut swaps);
    CONSTS.lock().unwrap_or_else(std::sync::PoisonError::into_inner).clone_from(&set);
    app.insert_resource(consts::Loaded(set)).add_systems(PostUpdate, consts::changed_later);
    // what each shader unlike the binary's was compiled from, by its text's hash: two runs drew with the same shaders
    // when these agree (a TS constant against the same edit in engine-shaders/), whatever their pixels' noise
    if !swaps.is_empty() {
        let list: Vec<_> = swaps
            .iter()
            .map(|(rel, t)| serde_json::json!({ "shader": rel, "fnv": format!("{:#x}", raea_sys_frame::bevy_api::fnv1a(t.as_bytes())), "project": done.contains(rel), "constants": constants.contains(rel) }))
            .collect();
        raea_io_trace::event("engine_shaders", serde_json::json!({ "shaders": list }));
    }
    if !swaps.is_empty() {
        app.insert_resource(Swaps(swaps)).add_systems(PreUpdate, swap_loaded.after(bevy::asset::AssetTrackingSystems));
    }
}

/// RAEA_ENGINE_SHADERS: every embedded shader with a source file in `root`'s git tree, but those the project's folder
/// gave (`done`) and raea's forks of Bevy's.
fn from_tree(app: &mut App, root: &Path, done: &[String], swaps: &mut Vec<(String, String)>) {
    let (mut read, mut differ, mut forks) = (0, Vec::new(), 0);
    let files = tree(root);
    TREE.lock().unwrap_or_else(std::sync::PoisonError::into_inner).clone_from(&files);
    for (rel, file) in files {
        if done.contains(&rel) {
            continue;
        }
        // raea's forks of Bevy's shaders are made from Bevy's at load; the tree's file is Bevy's original
        if raea_sys_frame::bevy_api::swapped(&rel) {
            forks += 1;
            continue;
        }
        if let Some(same) = replace(app, &rel, &file, swaps) {
            read += 1;
            if !same {
                differ.push(rel);
            }
        }
    }
    let list = if differ.is_empty() { String::new() } else { format!(": {}", differ.join(", ")) };
    eprintln!("raea: RAEA_ENGINE_SHADERS: {read} engine shaders read from {}, {} unlike this binary's{list} ({forks} of raea's forks of Bevy's kept as made)", root.display(), differ.len());
}

/// The engine-shaders folder of the edit the last Engine opened (shadow), for `text`.
static PROJECT: std::sync::Mutex<Option<PathBuf>> = std::sync::Mutex::new(None);

/// RAEA_ENGINE_SHADERS' tree as the last Engine opened read it (from_tree): each shader's path and its file, for `text`.
static TREE: std::sync::Mutex<Vec<(String, PathBuf)>> = std::sync::Mutex::new(Vec::new());

/// The constants the last Engine opened with (shadow), for `text`.
static CONSTS: std::sync::Mutex<Vec<consts::ShaderConstant>> = std::sync::Mutex::new(Vec::new());

/// The program's shader constants written into the text each shader will compile from: an embedded one's in the
/// registry and `swaps`, a declared one's checked here and applied by `text`. Says what it set, and what it couldn't;
/// the embedded shaders it changed.
fn with_constants(app: &mut App, set: &[consts::ShaderConstant], swaps: &mut Vec<(String, String)>) -> Vec<String> {
    let mut shaders: Vec<&str> = set.iter().map(|c| c.shader.as_str()).collect();
    shaders.sort_unstable();
    shaders.dedup();
    let declared = DECLARED.lock().unwrap_or_else(std::sync::PoisonError::into_inner).clone();
    let (mut said, mut changed) = (Vec::new(), Vec::new());
    for shader in shaders {
        let builtin = declared.get(shader).copied();
        let base = match builtin {
            Some(b) => Some(source(shader, b).into_owned()),
            None => original(app, shader).and_then(|b| String::from_utf8(b).ok()),
        };
        let Some(mut s) = base else {
            eprintln!("raea: <ShaderConstants>: no engine shader at {shader} (<crate>/<path under its src>, as Bevy embeds it)");
            continue;
        };
        let before = s.clone();
        for c in set.iter().filter(|c| c.shader == shader) {
            match consts::rewrite(&s, &c.name, c.value, &c.list) {
                Ok((t, old)) => {
                    let v = if c.list.is_empty() { c.value.to_string() } else { format!("{:?}", c.list) };
                    said.push(format!("{shader} {} {old} -> {v}", c.name));
                    s = t;
                }
                Err(e) => eprintln!("raea: <ShaderConstants> {shader}: {e}; left as the shader has it"),
            }
        }
        if builtin.is_none() && s != before {
            let reg = app.world().resource::<EmbeddedAssetRegistry>();
            reg.remove_asset(Path::new(shader));
            reg.insert_asset(PathBuf::from(file!()), Path::new(shader), s.clone().into_bytes());
            match swaps.iter_mut().find(|(r, _)| r == shader) {
                Some(w) => w.1 = s,
                None => swaps.push((shader.to_string(), s)),
            }
            changed.push(shader.to_string());
        }
    }
    if !said.is_empty() {
        eprintln!("raea: shader constants from the scene, compiled in at load: {}", said.join(", "));
    }
    changed
}

/// The shaders crates read through `text` (`declare`), each path and its built-in text: a project's file for one isn't
/// stray, and an empty one is filled with it.
static DECLARED: std::sync::Mutex<std::collections::BTreeMap<&'static str, &'static str>> = std::sync::Mutex::new(std::collections::BTreeMap::new());

/// Names the shaders a crate reads through `text`: each as `<crate>/<path under its src>` with its built-in text. Call
/// before the Engine's plugins finish (a plugin's build).
pub fn declare(shaders: &[(&'static str, &'static str)]) {
    let mut d = DECLARED.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    for &(rel, text) in shaders {
        // a shader declared twice keeps its first text
        d.entry(rel).or_insert(text);
    }
}

/// The bytes this binary embeds at `rel` (raea's fork, where raea forks Bevy's), left in place.
fn original(app: &App, rel: &str) -> Option<Vec<u8>> {
    let reg = app.world().resource::<EmbeddedAssetRegistry>();
    let old = reg.remove_asset(Path::new(rel))?.value().to_vec();
    reg.insert_asset(PathBuf::from(file!()), Path::new(rel), old.clone());
    Some(old)
}

/// A shader a crate hands to wgpu itself, as `shadow` reads the embedded ones: the project's engine-shaders/ file at
/// `rel` (`<crate>/<path under its src>`, named first with `declare`), else RAEA_ENGINE_SHADERS' file for it, else
/// `builtin`. Says so when what it reads isn't `builtin`.
pub fn text(rel: &str, builtin: &'static str) -> std::borrow::Cow<'static, str> {
    let mut s = source(rel, builtin);
    for c in CONSTS.lock().unwrap_or_else(std::sync::PoisonError::into_inner).iter().filter(|c| c.shader == rel) {
        if let Ok((t, _)) = consts::rewrite(&s, &c.name, c.value, &c.list) {
            s = t.into();
        }
    }
    if s != builtin {
        raea_io_trace::event("engine_shader_text", serde_json::json!({ "shader": rel, "fnv": format!("{:#x}", raea_sys_frame::bevy_api::fnv1a(s.as_bytes())) }));
    }
    s
}

/// The text `text` starts from: a project's or the tree's file, else `builtin`.
fn source(rel: &str, builtin: &'static str) -> std::borrow::Cow<'static, str> {
    let project = PROJECT.lock().unwrap_or_else(std::sync::PoisonError::into_inner).clone().map(|d| d.join(rel)).filter(|f| f.is_file());
    let tree = || TREE.lock().unwrap_or_else(std::sync::PoisonError::into_inner).iter().find(|(r, _)| r == rel).map(|(_, f)| f.clone());
    let Some(file) = project.or_else(tree) else { return builtin.into() };
    match std::fs::read_to_string(&file) {
        Ok(s) if s != builtin => {
            eprintln!("raea: the engine's {rel} from {} in place of this raea's (docs/nobuild.feature NB5b)", file.display());
            s.into()
        }
        Ok(_) => builtin.into(),
        Err(e) => {
            eprintln!("raea: {}: {e}; using this raea's {rel}", file.display());
            builtin.into()
        }
    }
}

/// The engine's embedded shader at `rel` replaced by `file`'s bytes, for every load from now on, and added to `swaps`
/// for a load already started; whether they were the same (then nothing is touched). None when there's no embedded
/// asset at `rel` or the file can't be read.
fn replace(app: &mut App, rel: &str, file: &Path, swaps: &mut Vec<(String, String)>) -> Option<bool> {
    let bytes = std::fs::read(file).ok()?;
    let reg = app.world().resource::<EmbeddedAssetRegistry>();
    let old = reg.remove_asset(Path::new(rel))?;
    let same = old.value() == bytes.as_slice();
    if same {
        reg.insert_asset(PathBuf::from(file!()), Path::new(rel), old.value().to_vec());
        return Some(true);
    }
    if let Ok(text) = std::str::from_utf8(&bytes) {
        swaps.push((rel.to_string(), text.to_string()));
    }
    reg.insert_asset(file.to_path_buf(), Path::new(rel), bytes);
    Some(false)
}

/// The shaders `replace` changed whose plugin loaded them before they were: (path, text), until each is swapped.
#[derive(Resource)]
struct Swaps(Vec<(String, String)>);

/// A replaced shader whose load began before its file was read (load_shader_library! at a plugin's build), swapped in
/// Assets<Shader> as that load lands: right after the asset server hands it over, in the same update, so the events
/// PostUpdate sends carry the file's text to the pipelines' first compile. Its defs and dependencies are the loaded one's.
fn swap_loaded(server: Res<AssetServer>, mut shaders: ResMut<Assets<Shader>>, mut swaps: ResMut<Swaps>) {
    swaps.0.retain(|(rel, text)| {
        let Some(h) = server.get_handle::<Shader>(format!("embedded://{rel}")) else { return true };
        let Some(old) = shaders.get(&h) else { return true };
        let (Source::Wgsl(s) | Source::Wesl(s)) = &old.source else { return false };
        if s == text {
            return false;
        }
        let mut new = if matches!(old.source, Source::Wesl(_)) { Shader::from_wesl(text.clone(), old.path.clone()) } else { Shader::from_wgsl(text.clone(), old.path.clone()) };
        new.shader_defs.clone_from(&old.shader_defs);
        new.file_dependencies.clone_from(&old.file_dependencies);
        let _ = shaders.insert(h.id(), new);
        false
    });
}

/// The nearest engine-shaders folder: beside the edit, or up to three folders above it.
fn project_dir(edit_dir: &Path) -> Option<PathBuf> {
    edit_dir.ancestors().take(4).map(|d| d.join("engine-shaders")).find(|d| d.is_dir())
}

/// Every file under `dir`, with its path relative to it ('/'-separated).
fn walk(dir: &Path) -> Vec<(String, PathBuf)> {
    let mut out = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        let Ok(rd) = std::fs::read_dir(&d) else { continue };
        for e in rd.flatten() {
            let p = e.path();
            if p.is_dir() {
                stack.push(p);
            } else if let Ok(r) = p.strip_prefix(dir) {
                out.push((r.components().map(|c| c.as_os_str().to_string_lossy()).collect::<Vec<_>>().join("/"), p));
            }
        }
    }
    out.sort();
    out
}

/// Every .wgsl and .wesl file in `root`'s git tree under a crate's src, with the path Bevy embeds it at: the crate's
/// name (dashes as underscores), then its path under src.
fn tree(root: &Path) -> Vec<(String, PathBuf)> {
    let Ok(out) = std::process::Command::new("git").arg("-C").arg(root).args(["ls-files", "-z", "--", "*.wgsl", "*.wesl"]).output() else {
        eprintln!("raea: RAEA_ENGINE_SHADERS: git couldn't list {}", root.display());
        return Vec::new();
    };
    let mut crates: HashMap<PathBuf, Option<String>> = HashMap::new();
    let mut found = Vec::new();
    for rel in out.stdout.split(|b| *b == 0).filter(|r| !r.is_empty()) {
        let file = root.join(String::from_utf8_lossy(rel).as_ref());
        // the nearest folder above it with a Cargo.toml is its crate
        let Some((dir, name)) = file.ancestors().skip(1).take_while(|d| d.starts_with(root)).find_map(|d| crate_name(&mut crates, d).map(|n| (d.to_path_buf(), n))) else { continue };
        let Ok(under) = file.strip_prefix(dir.join("src")) else { continue };
        let under = under.components().map(|c| c.as_os_str().to_string_lossy()).collect::<Vec<_>>().join("/");
        found.push((format!("{}/{under}", name.replace('-', "_")), file));
    }
    found
}

fn crate_name(seen: &mut HashMap<PathBuf, Option<String>>, dir: &Path) -> Option<String> {
    seen.entry(dir.to_path_buf())
        .or_insert_with(|| {
            let toml = std::fs::read_to_string(dir.join("Cargo.toml")).ok()?;
            let package = toml.split("[package]").nth(1)?;
            let line = package.lines().find(|l| l.trim_start().starts_with("name"))?;
            Some(line.split('"').nth(1)?.to_string())
        })
        .clone()
}
