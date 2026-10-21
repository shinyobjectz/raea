//! `raea-bevy-sdk [--prefix P]... [--type PATH]... [-o OUT.ts] [--schema OUT.json] [--title TEXT]`: TypeScript for a
//! Bevy plugin, generated from raea's own Bevy App (sdk/motion/docs/bevy-plugins.md). The plugins this build has (react's
//! `avian`, `hanabi`, `tweening` features, through raea-bevy-plugins) are registered, an Engine is made, and its
//! TypeRegistry is walked (rdb::engine::plugins::schema): every reflected component and resource under a prefix (by
//! default the compiled plugins' crates), or each named type, with its fields, enum variants and default. The output
//! is one typed TSX wrapper per component (a <BevyComponent> naming its type path, whose row rdb applies through
//! ReflectComponent::insert) and per resource, and a TypeScript type for every type their fields reach.
//! `--types-only` writes just the TypeScript types (sdk/motion/src/animate.gen.ts: `--type raea_animate::spec::Animation
//! --types-only`). `raea-bevy-sdk motion ARGS...` is raea-motion with the same plugins (built with features, this binary is the
//! plugin build: the default raea-motion stays plugin-free).
use std::collections::{BTreeMap, BTreeSet};

use serde_json::{Value, json};

fn flag_all(args: &[String], k: &str) -> Vec<String> {
    args.windows(2).filter(|w| w[0] == k).map(|w| w[1].clone()).collect()
}

/// The name a type gets in TypeScript: its short path with generics and punctuation folded away.
fn ts_name(short: &str) -> String {
    let mut out = String::new();
    let mut up = true;
    for ch in short.chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(if up { ch.to_ascii_uppercase() } else { ch });
            up = false;
        } else {
            up = true;
        }
    }
    out
}

/// The type argument(s) inside a path's outer angle brackets.
fn inner(path: &str) -> Vec<String> {
    let (Some(a), Some(b)) = (path.find('<'), path.rfind('>')) else { return Vec::new() };
    let mut out = Vec::new();
    let (mut depth, mut cur) = (0, String::new());
    for ch in path[a + 1..b].chars() {
        match ch {
            '<' | '(' | '[' => {
                depth += 1;
                cur.push(ch)
            }
            '>' | ')' | ']' => {
                depth -= 1;
                cur.push(ch)
            }
            ',' if depth == 0 => out.push(std::mem::take(&mut cur).trim().to_string()),
            _ => cur.push(ch),
        }
    }
    if !cur.trim().is_empty() {
        out.push(cur.trim().to_string());
    }
    out
}

struct Gen<'a> {
    types: &'a serde_json::Map<String, Value>,
    /// named types emitted (path -> TS name), and their declarations
    named: BTreeMap<String, String>,
    decls: Vec<String>,
    taken: BTreeSet<String>,
    /// why a root can't be written from TSX alone (an asset handle, an entity)
    blockers: BTreeSet<String>,
}

impl Gen<'_> {
    fn ty(&mut self, path: &str) -> String {
        let p = path.trim();
        let base = p.split('<').next().unwrap_or(p);
        match base {
            "bool" => return "boolean".into(),
            "f32" | "f64" | "i8" | "i16" | "i32" | "i64" | "i128" | "isize" | "u8" | "u16" | "u32" | "u64" | "u128" | "usize" | "core::num::NonZeroU32" | "core::num::NonZeroU64" => return "number".into(),
            "char" | "alloc::string::String" | "&str" | "alloc::borrow::Cow" | "std::path::PathBuf" | "bevy_platform::path::PathBuf" => return "string".into(),
            "core::option::Option" => {
                let i = inner(p).first().cloned().unwrap_or_default();
                return format!("{} | null", self.ty(&i));
            }
            "alloc::vec::Vec" | "smallvec::SmallVec" | "alloc::collections::vec_deque::VecDeque" | "bevy_platform::collections::hash_set::HashSet" => {
                let i = inner(p).first().cloned().unwrap_or_default();
                return format!("({})[]", self.ty(&i));
            }
            "bevy_platform::collections::hash_map::HashMap" | "std::collections::hash::map::HashMap" | "alloc::collections::btree::map::BTreeMap" => {
                let i = inner(p);
                return format!("Record<string, {}>", self.ty(i.get(1).map(String::as_str).unwrap_or("")));
            }
            "glam::Vec2" | "glam::IVec2" | "glam::UVec2" | "glam::DVec2" => return "[number, number]".into(),
            "glam::Vec3" | "glam::Vec3A" | "glam::IVec3" | "glam::UVec3" | "glam::DVec3" => return "[number, number, number]".into(),
            "glam::Vec4" | "glam::IVec4" | "glam::UVec4" | "glam::DVec4" | "glam::Quat" | "glam::DQuat" => return "[number, number, number, number]".into(),
            "glam::Mat2" | "glam::Mat3" | "glam::Mat3A" | "glam::Mat4" | "glam::Affine2" | "glam::Affine3A" | "glam::DMat4" => return "number[]".into(),
            "core::time::Duration" => return "{ secs: number; nanos: number }".into(),
            "bevy_asset::handle::Handle" => {
                self.blockers.insert(format!("an asset handle ({p}): a plugin asset needs a hand-written loader or builder"));
                return format!("never /* {p}: an asset handle, not data */");
            }
            "bevy_ecs::entity::Entity" => {
                self.blockers.insert("an entity reference: rows name rows, not entities".into());
                return "never /* an Entity */".into();
            }
            _ => {}
        }
        if p.starts_with('(') && p.ends_with(')') {
            let parts = inner(&format!("<{}>", &p[1..p.len() - 1]));
            return format!("[{}]", parts.iter().map(|x| self.ty(x)).collect::<Vec<_>>().join(", "));
        }
        if let Some(n) = self.named.get(p) {
            return n.clone();
        }
        let Some(t) = self.types.get(p).cloned() else { return format!("unknown /* {p} */") };
        let kind = t["kind"].as_str().unwrap_or("");
        if kind == "unregistered" {
            return format!("unknown /* {p}: not in the registry */");
        }
        if t.get("serde") == Some(&json!(true)) && !matches!(kind, "struct" | "enum" | "tuple_struct") {
            return format!("unknown /* {p}: its own serde form */");
        }
        match kind {
            "list" | "set" => return format!("({})[]", self.ty(t["item"].as_str().unwrap_or(""))),
            "array" => return format!("({})[]", self.ty(t["item"].as_str().unwrap_or(""))),
            "map" => return format!("Record<string, {}>", self.ty(t["value"].as_str().unwrap_or(""))),
            "tuple" => {
                let fs: Vec<String> = t["fields"].as_array().into_iter().flatten().map(|f| self.ty(f["type"].as_str().unwrap_or(""))).collect();
                return format!("[{}]", fs.join(", "));
            }
            "opaque" => return format!("unknown /* {p}: opaque */"),
            _ => {}
        }
        // a named type: declared once, under a free name
        let mut name = ts_name(t["short"].as_str().unwrap_or(p));
        while self.taken.contains(&name) {
            name.push('_');
        }
        self.taken.insert(name.clone());
        self.named.insert(p.to_string(), name.clone());
        let body = match kind {
            "struct" => {
                let fs: Vec<String> = t["fields"].as_array().into_iter().flatten().map(|f| format!("  {}: {};", f["name"].as_str().unwrap_or(""), self.ty(f["type"].as_str().unwrap_or("")))).collect();
                format!("export interface {name} {{\n{}\n}}", fs.join("\n"))
            }
            "tuple_struct" => {
                let fs: Vec<String> = t["fields"].as_array().into_iter().flatten().map(|f| self.ty(f["type"].as_str().unwrap_or(""))).collect();
                // a newtype is its inner value in reflect serde
                if fs.len() == 1 { format!("export type {name} = {};", fs[0]) } else { format!("export type {name} = [{}];", fs.join(", ")) }
            }
            "enum" => {
                let vs: Vec<String> = t["variants"].as_array().into_iter().flatten().map(|v| {
                    let vn = v["name"].as_str().unwrap_or("");
                    let fs = v["fields"].as_array().cloned().unwrap_or_default();
                    match v["kind"].as_str().unwrap_or("") {
                        "unit" => format!("\"{vn}\""),
                        "tuple" if fs.len() == 1 => format!("{{ {vn}: {} }}", self.ty(fs[0]["type"].as_str().unwrap_or(""))),
                        "tuple" => format!("{{ {vn}: [{}] }}", fs.iter().map(|f| self.ty(f["type"].as_str().unwrap_or(""))).collect::<Vec<_>>().join(", ")),
                        _ => format!("{{ {vn}: {{ {} }} }}", fs.iter().map(|f| format!("{}: {}", f["name"].as_str().unwrap_or(""), self.ty(f["type"].as_str().unwrap_or("")))).collect::<Vec<_>>().join("; ")),
                    }
                }).collect();
                format!("export type {name} =\n  | {};", if vs.is_empty() { "never".into() } else { vs.join("\n  | ") })
            }
            _ => format!("export type {name} = unknown;"),
        };
        self.decls.push(format!("/** {p} */\n{body}"));
        name
    }
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.iter().any(|a| a == "-h" || a == "--help") {
        eprintln!("raea-bevy-sdk [--prefix P]... [--type PATH]... [-o OUT.ts] [--schema OUT.json] [--title TEXT]\nraea-bevy-sdk motion ARGS...   raea-motion with this build's plugins (render, play, test)");
        return;
    }
    #[cfg(feature = "bevy-plugins")]
    raea_bevy_plugins::install();
    // raea-motion itself, with the plugins: a plugin build is its own binary, so the default raea-motion never changes
    if args.first().map(String::as_str) == Some("motion") {
        std::process::exit(raea_react::motion::cli(&args[1..]));
    }
    #[cfg(feature = "bevy-plugins")]
    let (compiled, default_prefixes) = (raea_bevy_plugins::compiled(), raea_bevy_plugins::prefixes());
    #[cfg(not(feature = "bevy-plugins"))]
    let (compiled, default_prefixes): (Vec<&str>, Vec<&str>) = (Vec::new(), Vec::new());
    let mut prefixes = flag_all(&args, "--prefix");
    let exact = flag_all(&args, "--type");
    if prefixes.is_empty() && exact.is_empty() {
        prefixes = default_prefixes.iter().map(|s| s.to_string()).collect();
    }
    // raea's App, as a render makes it: one camera row, a tiny picture
    let mut cam = BTreeMap::new();
    cam.insert("id".to_string(), rdb::ir::V::S("cam".into()));
    let prog = rdb::ir::Program { fps: 30.0, width: 64, height: 64, frames: 1, spawns: vec![("camera".into(), vec![cam])], ..Default::default() };
    let dir = std::env::temp_dir();
    let engine = match rdb::engine::Engine::new(&prog, &rdb::inserted_of(&prog), &dir) {
        Ok(e) => e,
        Err(e) => {
            eprintln!("raea-bevy-sdk: the engine didn't start: {e}");
            std::process::exit(1);
        }
    };
    // raea's own reflected types (the declared-animation wire format motion's <Animate> sends): `--type
    // raea_animate::spec::Animation --types-only` generates sdk/motion/src/animate.gen.ts from them
    engine.world().resource::<bevy::prelude::AppTypeRegistry>().write().register::<raea_animate::Animation>();
    let types_only = args.iter().any(|a| a == "--types-only");
    let keep = |p: &str| exact.iter().any(|e| e == p) || prefixes.iter().any(|x| p.starts_with(x.as_str()));
    let schema = rdb::engine::plugins::schema(engine.world(), &keep);
    if let Some(o) = flag_all(&args, "--schema").first() {
        std::fs::write(o, serde_json::to_string_pretty(&schema).unwrap()).expect("schema written");
    }
    let types = schema["types"].as_object().cloned().unwrap_or_default();
    let roots: Vec<String> = schema["roots"].as_array().into_iter().flatten().filter_map(|v| v.as_str().map(str::to_string)).collect();
    let mut g = Gen { types: &types, named: BTreeMap::new(), decls: Vec::new(), taken: BTreeSet::new(), blockers: BTreeSet::new() };
    let mut wrappers = Vec::new();
    let mut report = Vec::new();
    let mut taken_fns: BTreeSet<String> = BTreeSet::new();
    for path in &roots {
        let t = &types[path];
        if t["generic"] == json!(true) {
            report.push(format!("{path}: skipped (generic)"));
            continue;
        }
        g.blockers.clear();
        let tsty = g.ty(path);
        let resource = t["resource"] == json!(true);
        let has_default = t.get("default").is_some();
        let short = ts_name(t["short"].as_str().unwrap_or(path));
        let mut fname = if resource { format!("{short}Resource") } else { short.clone() };
        while taken_fns.contains(&fname) {
            fname.push('_');
        }
        taken_fns.insert(fname.clone());
        // a struct with a default may leave fields out; anything else is given whole
        let value_ty = if has_default && t["kind"] == "struct" { format!("Partial<{tsty}>") } else { tsty.clone() };
        let opt = if has_default { "?" } else { "" };
        let mut doc = format!("{path} ({})", if resource { "resource" } else { "component" });
        if let Some(d) = t.get("default") {
            let d = d.to_string();
            if d.len() < 200 {
                doc.push_str(&format!("; default {d}"));
            }
        }
        let blocked: Vec<String> = g.blockers.iter().cloned().collect();
        if !blocked.is_empty() {
            doc.push_str(&format!(". Not drivable from TSX alone: {}", blocked.join("; ")));
        }
        // left out, the value is the type's default (as reflect serde writes it)
        let fallback = t.get("default").map(|d| d.to_string()).unwrap_or_else(|| "null".into());
        let call = if resource {
            format!("export function {fname}(props: {{ value{opt}: {value_ty} }}) {{\n  return <BevyResource type=\"{path}\" value={{props.value ?? {fallback}}} />;\n}}")
        } else {
            format!("export function {fname}(props: {{ value{opt}: {value_ty}; from?: number }}) {{\n  return <BevyComponent type=\"{path}\" value={{props.value ?? {fallback}}} from={{props.from}} />;\n}}")
        };
        // the wrapper takes the type's name; its type stays reachable as the value's type
        wrappers.push(format!("/** {doc} */\n{call}"));
        report.push(format!("{path}: {} {}", if resource { "resource" } else { "component" }, if blocked.is_empty() { "ok" } else { "blocked" }));
    }
    if types_only {
        // the types alone, for a module that builds the values itself
        let mut out = format!("// Generated by raea-bevy-sdk --types-only from raea's Bevy App's TypeRegistry ({}); don't edit, regenerate:\n//   raea-bevy-sdk {}\n\n", exact.join(", "), args.join(" "));
        for d in &g.decls {
            out.push_str(d);
            out.push_str("\n\n");
        }
        match flag_all(&args, "-o").first() {
            Some(o) => std::fs::write(o, out.trim_end().to_string() + "\n").expect("written"),
            None => print!("{out}"),
        }
        eprintln!("raea-bevy-sdk: {} types declared", g.decls.len());
        return;
    }
    let title = flag_all(&args, "--title").first().cloned().unwrap_or_else(|| if compiled.is_empty() { "Bevy's own types".into() } else { compiled.join(", ") });
    let mut out = format!(
        "// Generated by raea-bevy-sdk from raea's Bevy App's TypeRegistry ({title}); don't edit, regenerate.\n// {} wrappers: each a <BevyComponent> (or <BevyResource>) naming its type path; rdb applies its row with\n// ReflectComponent::insert (engine/rdb/src/engine/plugins.rs). Put a component inside the <Mesh> it belongs to.\nimport {{ BevyComponent, BevyResource }} from \"@raea/motion/3d\";\n\n",
        wrappers.len()
    );
    for d in &g.decls {
        out.push_str(d);
        out.push_str("\n\n");
    }
    for w in &wrappers {
        out.push_str(w);
        out.push_str("\n\n");
    }
    out.push_str("void BevyResource;\nvoid BevyComponent;\n");
    match flag_all(&args, "-o").first() {
        Some(o) => std::fs::write(o, &out).expect("written"),
        None => print!("{out}"),
    }
    for r in &report {
        eprintln!("{r}");
    }
    eprintln!("raea-bevy-sdk: {} roots, {} types declared, {} wrappers (plugins compiled: {:?})", roots.len(), g.decls.len(), wrappers.len(), compiled);
}
