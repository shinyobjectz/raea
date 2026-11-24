//! A kit's steps (docs/make.feature MK10-MK12, from AnyKit): every object's reference made in one style from the
//! style images, checked on one sheet before any 3D; each reference to both generators (Tripo H3.1 and Meshy, on fal)
//! and the pick made on clay renders (kitout.rs draws the pick sheets); the pick fitted to its real size with a
//! quarter-triangle LOD beside it, each with a sidecar saying how it was made.
//!
//!   raea make refs [DIR] [--only ID,...] [--model M] [--dry]
//!   raea make sheet refs [DIR]
//!   raea make gen [DIR] [--only ID,...] [--tripo M] [--meshy M] [--dry]
//!   raea make pick ID GENERATOR "why"
//!   raea make prep [DIR] [--only ID,...]

use std::path::{Path, PathBuf};

use raea_core_brief::make::{Make, Object};
use serde_json::{Value, json};

use crate::sheet::{Tile, sheet};
use crate::shots::raea;

/// What every reference prompt ends with: one object, whole, on a plain ground, in even light (the scan template).
pub const SCAN: &str = "One object only, whole and centred, on a plain light grey background, soft even studio light, a three-quarter view from slightly above. Its own printed labels only, from the one maker named; no real brands, no text overlays.";
/// The two generators every reference goes to (MK11).
pub const TRIPO: &str = "tripo:H3.1";
pub const MESHY: &str = "fal:meshy/v7.1/image-to-3d";
/// Face limits: a hero, a small prop.
const HERO_FACES: u32 = 150_000;
const PROP_FACES: u32 = 60_000;

/// The objects named in `--only` (all when it's empty).
pub fn objects<'a>(m: &'a Make, only: &[String]) -> Vec<&'a Object> {
    crate::only(&m.objects, only, |o| o.id.as_str())
}

/// The prompt a reference is made from: the plan's style paragraph, the object's own words, the scan template.
pub fn ref_prompt(m: &Make, o: &Object) -> String {
    let style = m.style.as_deref().unwrap_or("").trim().trim_end_matches('.');
    let what = o.prompt.as_deref().unwrap_or(&o.label).trim().trim_end_matches('.');
    format!("{style}. {what}. {SCAN}")
}

/// `--h`, `--dims` or nothing: how gen check fits an object to its real size.
pub fn size_args(o: &Object) -> Vec<String> {
    if let Some([w, d, h]) = o.dims {
        return vec!["--dims".into(), format!("{w},{d},{h}")];
    }
    if let Some(h) = o.h {
        return vec!["--h".into(), h.to_string()];
    }
    Vec::new()
}

/// How `raea gen check -o` fits one generator's model to the object's size: its height (`--h`, which is what scales;
/// a boxy object's is the third of its dims) and, for a boxy one, a quarter turn when the model's long side lies
/// across (x) where the plan's lies front to back (z), or the other way: generators don't agree on which way is front.
/// `measured` is the raw model's [w, h, d] (`raea gen check --json`'s measured.size).
pub fn fit_args(o: &Object, measured: Option<[f64; 3]>) -> Vec<String> {
    let mut a = size_args(o);
    if let Some([w, d, h]) = o.dims {
        a.extend(["--h".into(), h.to_string()]);
        if let Some([mw, _, md]) = measured {
            let long = |x: f64, z: f64| if x > z * 1.2 { Some(true) } else if z > x * 1.2 { Some(false) } else { None };
            if let (Some(want), Some(got)) = (long(w, d), long(mw, md)) {
                if want != got {
                    a.extend(["--turn".into(), "90".into()]);
                }
            }
        }
    }
    a
}

/// The raw model's size [w, h, d], measured by `raea gen check --json`.
pub fn measured_size(model: &Path) -> Option<[f64; 3]> {
    let out = raea(&["gen".into(), "check".into(), model.display().to_string(), "--json".into()], &[]).ok()?;
    let v = json_of(&out)?;
    let s = v["measured"]["size"].as_array()?;
    Some([s.first()?.as_f64()?, s.get(1)?.as_f64()?, s.get(2)?.as_f64()?])
}

/// The file a single-shot `raea gen` wrote: its JSON's `out.out`, a path or the first of a list.
pub fn gen_file(out: &str) -> Option<PathBuf> {
    let v: Value = serde_json::from_str(out.trim()).ok()?;
    let o = &v["out"]["out"];
    o.as_str().or_else(|| o.as_array()?.first()?.as_str()).map(PathBuf::from)
}

fn cents(out: &str) -> f64 {
    json_of(out).and_then(|v| v["cents"].as_f64()).unwrap_or(0.0)
}

/// raea gen's JSON from its output (anything printed before it skipped).
fn json_of(out: &str) -> Option<Value> {
    serde_json::from_str(out[out.find('{')?..].trim()).ok()
}

/// A `--plan` or `gen plan` quote in a line: what each paid step runs on, and the total.
pub fn quote(out: &str) -> String {
    let Some(v) = json_of(out) else { return out.split_whitespace().collect::<Vec<_>>().join(" ") };
    let on: Vec<&str> = v["steps"].as_array().into_iter().flatten().filter_map(|s| s["runs_on"].as_str()).filter(|r| *r != "load").collect();
    format!("{} (about {}¢)", if on.is_empty() { "nothing paid".to_string() } else { on.join(" + ") }, v["cents"].as_f64().unwrap_or(0.0))
}

pub fn refs_dir(dir: &Path) -> PathBuf {
    dir.join("out/kit/refs")
}

/// Every object's reference, made from the style images (`references`) in the plan's style; then the sheet.
pub fn refs(dir: &Path, m: &Make, only: &[String], model: Option<&str>, dry: bool) -> Result<String, String> {
    if m.references.is_empty() {
        return Err("raea make refs: the plan has no `references` (the style images every reference is made from)".into());
    }
    let mut said = Vec::new();
    let mut spent = 0.0;
    for o in objects(m, only) {
        let work = refs_dir(dir).join("gen").join(&o.id);
        let mut args: Vec<String> = vec!["gen".into(), "image".into(), ref_prompt(m, o)];
        for r in &m.references {
            args.extend(["--image".into(), dir.join(r).display().to_string()]);
        }
        args.extend(["--aspect".into(), "1:1".into(), "-o".into(), work.display().to_string()]);
        if let Some(md) = model {
            args.extend(["--model".into(), md.to_string()]);
        }
        if dry {
            args.push("--plan".into());
            let out = raea(&args, &[])?;
            spent += cents(&out);
            said.push(format!("{}: {}", o.id, quote(&out)));
            continue;
        }
        eprintln!("raea make refs: {}", o.id);
        let out = raea(&args, &[])?;
        spent += cents(&out);
        let made = gen_file(&out).ok_or_else(|| format!("{}: raea gen image wrote no file: {}", o.id, out.trim()))?;
        let to = refs_dir(dir).join(format!("{}.png", o.id));
        std::fs::copy(&made, &to).map_err(|e| format!("{}: {e}", to.display()))?;
        said.push(format!("{}: {}", o.id, to.display()));
    }
    if dry {
        return Ok(format!("would make {} reference{} for about ${:.2} (nothing spent):\n{}", said.len(), if said.len() == 1 { "" } else { "s" }, spent / 100.0, said.join("\n")));
    }
    let sheet = sheet_refs(dir, m)?;
    Ok(format!("{}\n{sheet}\nspent about ${:.2}", said.join("\n"), spent / 100.0))
}

/// The reference sheet: the style images first, then every object's reference, labelled with its id and size.
pub fn sheet_refs(dir: &Path, m: &Make) -> Result<String, String> {
    let style: Vec<Tile> = m.references.iter().enumerate().map(|(i, r)| Tile { img: dir.join(r), label: format!("style {}", i + 1) }).collect();
    let refs: Vec<Tile> = m
        .objects
        .iter()
        .map(|o| {
            let size = o.h.map(|h| format!("{h} m tall")).or(o.dims.map(|d| format!("{} x {} x {} m", d[0], d[1], d[2]))).or(o.max.map(|x| format!("{x} m long"))).unwrap_or_default();
            Tile { img: refs_dir(dir).join(format!("{}.png", o.id)), label: format!("{} {size}", o.id) }
        })
        .collect();
    let mut rows = vec![style];
    rows.extend(refs.chunks(4).map(|c| c.iter().map(|t| Tile { img: t.img.clone(), label: t.label.clone() }).collect()));
    let file = dir.join("review/refs.png");
    sheet("Reference sheet: the style, then every object's reference", &rows, 320, &file)?;
    Ok(format!("wrote {}: regenerate an outlier before any 3D (raea make refs --only ID); ask it: raea ask {} --std make.refs.outlier --std make.refs.two_objects --std make.refs.brands", file.display(), file.display()))
}

/// The flow that sends one reference to both generators, each at its face limit.
pub fn gen_flow(m: &Make, o: &Object, reference: &Path, tripo: &str, meshy: &str) -> Value {
    let faces = if o.hero == Some(true) { HERO_FACES } else { PROP_FACES };
    let prompt = o.prompt.clone().unwrap_or_else(|| o.label.clone());
    let _ = m;
    json!({"flow": 1, "steps": {
        "ref": {"op": "media.load", "in": {"path": reference.display().to_string()}},
        "tripo": {"op": "mesh.generate", "model": tripo, "in": {"prompt": prompt, "image": "@ref", "x_face_limit": faces}},
        "meshy": {"op": "mesh.generate", "model": meshy, "in": {"prompt": prompt, "image": "@ref"}}
    }, "out": ["tripo", "meshy"]})
}

pub fn gen_dir(dir: &Path, id: &str) -> PathBuf {
    dir.join("out/kit/gen").join(id)
}

/// Every reference to both generators: out/kit/gen/ID/tripo.glb and meshy.glb.
pub fn generate(dir: &Path, m: &Make, only: &[String], tripo: &str, meshy: &str, dry: bool) -> Result<String, String> {
    let mut said = Vec::new();
    let mut spent = 0.0;
    for o in objects(m, only) {
        let reference = refs_dir(dir).join(format!("{}.png", o.id));
        if !reference.is_file() {
            return Err(format!("raea make gen: no reference for {} ({}): raea make refs --only {}", o.id, reference.display(), o.id));
        }
        let to = gen_dir(dir, &o.id);
        // both models already there: a rerun (after a stop, or for the pieces still missing) doesn't pay twice
        if !dry && to.join("tripo.glb").is_file() && to.join("meshy.glb").is_file() {
            said.push(format!("{}: already made ({})", o.id, to.display()));
            continue;
        }
        std::fs::create_dir_all(&to).map_err(|e| format!("{}: {e}", to.display()))?;
        if !dry {
            eprintln!("raea make gen: {} on {tripo} and {meshy} (a few minutes each)", o.id);
        }
        let flow = to.join("flow.json");
        std::fs::write(&flow, serde_json::to_string_pretty(&gen_flow(m, o, &reference, tripo, meshy)).unwrap_or_default()).map_err(|e| e.to_string())?;
        // quoted first: the run is capped at its own quote (a little over, for rounding), never gen's default cap
        let plan = raea(&["gen".into(), "plan".into(), flow.display().to_string()], &[])?;
        let out = if dry {
            plan
        } else {
            let cap = (cents(&plan) * 1.1).ceil().max(1.0) as u64;
            raea(&["gen".into(), "run".into(), flow.display().to_string(), "-o".into(), to.display().to_string(), "--max-cents".into(), cap.to_string()], &[])?
        };
        spent += cents(&out);
        said.push(if dry { format!("{}: {}", o.id, quote(&out)) } else { format!("{}: {}", o.id, to.display()) });
    }
    let tail = if dry { format!("about ${:.2} in all; nothing spent", spent / 100.0) } else { format!("spent about ${:.2}; next: raea make pick-sheet", spent / 100.0) };
    Ok(format!("{}\n{tail}", said.join("\n")))
}

/// The model a pick's generator ran (from its flow.json), for the sidecar.
fn pick_model(dir: &Path, id: &str, generator: &str) -> String {
    let flow: Option<Value> = std::fs::read_to_string(gen_dir(dir, id).join("flow.json")).ok().and_then(|t| serde_json::from_str(&t).ok());
    flow.and_then(|f| f["steps"][generator]["model"].as_str().map(String::from)).unwrap_or_else(|| generator.to_string())
}

pub fn picks_file(dir: &Path) -> PathBuf {
    dir.join("out/kit/picks.json")
}

/// The picks so far, by object id: {generator, why}.
pub fn picks(dir: &Path) -> serde_json::Map<String, Value> {
    std::fs::read_to_string(picks_file(dir)).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default()
}

/// One object's pick and why, recorded (MK11).
pub fn choose(dir: &Path, m: &Make, id: &str, generator: &str, why: &str) -> Result<String, String> {
    if !m.objects.iter().any(|o| o.id == id) {
        return Err(format!("raea make pick: no object `{id}` in the plan"));
    }
    let glb = gen_dir(dir, id).join(format!("{generator}.glb"));
    if !glb.is_file() {
        return Err(format!("raea make pick: no {} (the generators are tripo and meshy: raea make gen --only {id})", glb.display()));
    }
    if why.trim().is_empty() {
        return Err("raea make pick: say why in one line (\"sharper dial numerals; Tripo fused the handset to the cradle\")".into());
    }
    let mut p = picks(dir);
    p.insert(id.into(), json!({"generator": generator, "why": why.trim()}));
    let f = picks_file(dir);
    std::fs::write(&f, serde_json::to_string_pretty(&p).unwrap_or_default()).map_err(|e| format!("{}: {e}", f.display()))?;
    Ok(format!("picked {generator} for {id}: {}", why.trim()))
}

/// The triangles gen check measured.
fn triangles(glb: &Path) -> Result<u64, String> {
    let out = raea(&["gen".into(), "check".into(), glb.display().to_string(), "--json".into()], &[]).or_else(|e| if e.contains('{') { Ok(e) } else { Err(e) })?;
    let v: Value = serde_json::from_str(out.trim()).map_err(|e| format!("gen check {}: {e}", glb.display()))?;
    v["measured"]["triangles"].as_u64().ok_or_else(|| format!("gen check couldn't read {}", glb.display()))
}

/// Each pick fitted to its real size (front -Y, bottom-centre, metres) and a quarter-triangle LOD1 beside it, each
/// with a sidecar saying how it was made (MK12).
pub fn prep(dir: &Path, m: &Make, only: &[String]) -> Result<String, String> {
    let p = picks(dir);
    let mut said = Vec::new();
    for o in objects(m, only) {
        let Some(pick) = p.get(&o.id) else {
            said.push(format!("{}: no pick yet (raea make pick {} tripo|meshy \"why\")", o.id, o.id));
            continue;
        };
        let generator = pick["generator"].as_str().unwrap_or("tripo");
        let src = gen_dir(dir, &o.id).join(format!("{generator}.glb"));
        let to = dir.join("out/kit").join(&o.id);
        std::fs::create_dir_all(&to).map_err(|e| format!("{}: {e}", to.display()))?;
        let full = to.join(format!("{}.glb", o.id));
        let mut args = vec!["gen".into(), "check".into(), src.display().to_string()];
        args.extend(fit_args(o, measured_size(&src)));
        args.extend(["-o".into(), full.display().to_string()]);
        // gen check exits non-zero on a finding it can't fix; the fitted copy is still written, and its findings said
        let _ = raea(&args, &[]);
        if !full.is_file() {
            return Err(format!("{}: gen check wrote no fitted copy of {}", o.id, src.display()));
        }
        let tris = triangles(&full)?;
        let lod = to.join(format!("{}_lod1.glb", o.id));
        let quarter = (tris / 4).max(500);
        let model = pick_model(dir, &o.id, generator);
        // the sidecar the Blender scripts carry along (source, licence, credit) and how it was made: ours, generated
        let made = |what: &str, tris: u64| {
            json!({"source": format!("generated by raea make: {model} from out/kit/refs/{}.png", o.id), "license": "ours (generated for this project)", "credit": "",
                "made": {"by": "raea make (docs/make.feature)", "route": "gen", "generator": generator, "model": model, "why": pick["why"], "prompt": o.prompt, "reference": format!("out/kit/refs/{}.png", o.id),
                "style": m.style, "what": what, "triangles": tris, "source": if o.source.is_some() { json!(o.source) } else { Value::Null }}})
        };
        std::fs::write(full.with_extension("glb.json"), serde_json::to_string_pretty(&made("fitted to its real size", tris)).unwrap_or_default()).map_err(|e| e.to_string())?;
        raea(&["blender".into(), "run".into(), "blender/decimate.py".into(), full.display().to_string(), lod.display().to_string(), quarter.to_string()], &[])?;
        // decimate.py wrote the LOD's sidecar from the model's, with its step; it says what it is, over the copy
        let side = lod.with_extension("glb.json");
        let mut v: Value = std::fs::read_to_string(&side).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_else(|| made("", 0));
        v["made"] = made("LOD1: a quarter of the fitted model's triangles", quarter)["made"].clone();
        std::fs::write(&side, serde_json::to_string_pretty(&v).unwrap_or_default()).map_err(|e| e.to_string())?;
        said.push(format!("{}: {} ({tris} triangles) and its LOD1 ({quarter})", o.id, full.display()));
    }
    Ok(said.join("\n"))
}
