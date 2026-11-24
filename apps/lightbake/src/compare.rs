//! One set against Cycles (docs/bake.feature BK0): its Cycles still (blender/look.py) and Cycles bake
//! (blender/lightbake.py), kept in DIR/NAME until the set or the samples change, then raea's frames beside them: the set
//! unbaked, the set with Cycles' maps, and the set with raea's own bake. A frame is measured against the Cycles still by
//! its RMSE (sRGB 0..1 over every pixel and channel, as docs/blender-research.md's table) and in stops (raea light
//! check's distance); a lightmap against Cycles' in stops, texel by texel on the same UVs (map_stops).

use crate::{key, library, run, Ran, Set};
use raea_runtime_bundle::R;
use serde_json::{json, Value};
use std::path::{Path, PathBuf};

/// The bar (BK0): the frame with raea's bake within this RMSE of the Cycles still on the table set (lightbake.py's gives
/// 0.040), no mesh's map off by more than a third of a stop on average, in under a quarter of lightbake.py's time.
pub const BAR_RMSE: f64 = 0.045;
pub const BAR_STOPS: f64 = 1.0 / 3.0;
pub const BAR_TIME: f64 = 0.25;

/// The set's Cycles references, made: the export, look.py's still at `samples` (OUT/cycles.png), lightbake.py's bake at
/// `samples` into the set's media/baked/NAME/, which the set's `cycles` composition reads.
fn reference(set: &Set, samples: u32, out: &Path, baked: &Path) -> R<Value> {
    let bl = out.join("bl");
    let _ = std::fs::remove_dir_all(&bl);
    let _ = std::fs::remove_dir_all(baked);
    let stem = Path::new(&set.file).file_stem().map(|x| x.to_string_lossy().to_string()).unwrap_or_default();
    let open = run(&set.dir, &["blender".into(), "open".into(), set.file.clone(), "--no-window".into(), "-o".into(), format!("{}", bl.display())], &out.join("open.log"))?;
    let (blend, look) = (bl.join(format!("{stem}.blend")), bl.join("look.blend"));
    let mut a = vec!["blender".into(), "run".into(), format!("{}", library("look.py")?.display()), format!("{}", blend.display()), format!("{}", look.display())];
    a.extend(set.look.iter().cloned());
    a.extend(["--engine".into(), "cycles".into(), "--samples".into(), samples.to_string(), "--still".into(), format!("{}", out.join("cycles.png").display())]);
    let still = run(&set.dir, &a, &out.join("look.log"))?;
    let a = vec!["blender".into(), "run".into(), format!("{}", library("lightbake.py")?.display()), format!("{}", look.display()), format!("{}", baked.join("set.glb").display()), "--samples".into(), samples.to_string(), "--texels".into(), set.texels.to_string()];
    let bake = run(&set.dir, &a, &out.join("lightbake.log"))?;
    Ok(json!({ "export": open.json(), "still": still.json(), "lightbake": bake.json() }))
}

/// Frame 0 of `composition` in `file`, drawn by raea into OUT/LABEL/: the picture and what it cost.
fn frame(set: &Set, file: &str, composition: &str, out: &Path, label: &str) -> R<(PathBuf, Ran)> {
    let dir = out.join(label);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let a = ["render", file, "--composition", composition, "--at", "0", "-o", &format!("{}/", dir.display())].map(String::from);
    let ran = run(&set.dir, &a, &out.join(format!("{label}.log")))?;
    Ok((dir.join("frame-00000.png"), ran))
}

fn round(x: f64) -> f64 {
    (x * 10000.0).round() / 10000.0
}

/// RMSE of two pictures the same size, sRGB 0..1 over every pixel and channel.
pub fn rmse(a: &Path, b: &Path) -> R<f64> {
    let load = |p: &Path| image::open(p).map(|i| i.to_rgb8()).map_err(|e| format!("{}: {e}", p.display()));
    let (a, b) = (load(a)?, load(b)?);
    if a.dimensions() != b.dimensions() {
        return Err(format!("sizes differ: {:?} against {:?}", a.dimensions(), b.dimensions()));
    }
    let sum: f64 = a.as_raw().iter().zip(b.as_raw()).map(|(x, y)| ((*x as f64 - *y as f64) / 255.0).powi(2)).sum();
    Ok((sum / a.as_raw().len().max(1) as f64).sqrt())
}

/// A raea frame against the Cycles still: RMSE, and the mean and worst tile in stops.
fn measure(got: &(PathBuf, Ran), cycles: &Path) -> R<Value> {
    let d = raea_light::metric::distance(&raea_light::metric::load(&got.0)?, &raea_light::metric::load(cycles)?)?;
    Ok(json!({ "rmse": round(rmse(&got.0, cycles)?), "mean_stops": round(d.mean as f64), "worst_stops": round(d.worst as f64), "render": got.1.json() }))
}

/// A lightmap against another on the same UVs, each scaled by its lightmap_exposure: over the texels both cover, the
/// mean distance in stops, the mean signed (brighter is positive), and the worst 8 x 8 tile's mean distance.
pub fn map_stops(a: &Path, a_exposure: f64, b: &Path, b_exposure: f64) -> R<Value> {
    let lum = |p: &Path, k: f64| -> R<(u32, u32, Vec<f64>)> {
        let i = image::open(p).map_err(|e| format!("{}: {e}", p.display()))?.to_rgb32f();
        Ok((i.width(), i.height(), i.pixels().map(|q| k * (0.2126 * q[0] as f64 + 0.7152 * q[1] as f64 + 0.0722 * q[2] as f64)).collect()))
    };
    let ((w, h, a), (bw, bh, b)) = (lum(a, a_exposure)?, lum(b, b_exposure)?);
    if (w, h) != (bw, bh) {
        return Err(format!("map sizes differ: {w}x{h} against {bw}x{bh}"));
    }
    let (mut sum, mut signed, mut n) = (0.0, 0.0, 0usize);
    let mut tiles = vec![(0.0, 0usize); w.div_ceil(8) as usize * h.div_ceil(8) as usize];
    for (i, (x, y)) in a.iter().zip(&b).enumerate() {
        if *x <= 0.0 || *y <= 0.0 {
            continue;
        }
        let st = (x / y).log2();
        (sum, signed, n) = (sum + st.abs(), signed + st, n + 1);
        let t = &mut tiles[(i / w as usize / 8) * w.div_ceil(8) as usize + (i % w as usize) / 8];
        *t = (t.0 + st.abs(), t.1 + 1);
    }
    let worst = tiles.iter().filter(|t| t.1 >= 32).map(|t| t.0 / t.1 as f64).fold(0.0, f64::max);
    let k = n.max(1) as f64;
    Ok(json!({ "texels": n, "mean_stops": round(sum / k), "bias_stops": round(signed / k), "worst_tile_stops": round(worst) }))
}

/// Whether a set's numbers meet the bar, and why not.
fn bar(r: &Value) -> Value {
    let f = &r["frames"]["raea-bake"];
    if f["made"] == json!(false) {
        return json!({ "met": false, "why": f["why"] });
    }
    let rmse = f["rmse"].as_f64().is_some_and(|x| x <= BAR_RMSE);
    let meshes = r["meshes"].as_array().is_some_and(|m| m.iter().all(|m| m["raea"]["mean_stops"].as_f64().is_some_and(|x| x <= BAR_STOPS)));
    let time = match (f["bake"]["seconds"].as_f64(), r["reference"]["lightbake"]["seconds"].as_f64()) {
        (Some(a), Some(b)) => a < b * BAR_TIME,
        _ => false,
    };
    json!({ "met": rmse && meshes && time, "rmse": rmse, "meshes": meshes, "time": time })
}

pub fn set(set: &Set, samples: u32, out: &Path, fresh: bool) -> R<Value> {
    std::fs::create_dir_all(out).map_err(|e| format!("{}: {e}", out.display()))?;
    let mut r = json!({ "name": set.name, "samples": samples });
    if let Err(why) = &set.reference {
        r["reference"] = json!({ "made": false, "why": why });
        return Ok(r);
    }
    let (baked, cycles, made) = (set.dir.join("media/baked").join(&set.name), out.join("cycles.png"), out.join("made.json"));
    let key = key(set, samples);
    let kept = std::fs::read_to_string(&made).ok().and_then(|t| serde_json::from_str::<Value>(&t).ok());
    r["reference"] = match kept.filter(|k| !fresh && k["key"] == json!(key) && cycles.is_file() && baked.join("set.lightmaps.json").is_file()) {
        Some(k) => {
            let mut v = k["reference"].clone();
            v["kept"] = json!(true);
            v
        }
        None => {
            let v = reference(set, samples, out, &baked)?;
            std::fs::write(&made, json!({ "key": key, "reference": v }).to_string()).map_err(|e| e.to_string())?;
            v
        }
    };
    r["frames"]["raea"] = measure(&frame(set, &set.file, &set.composition, out, "raea")?, &cycles)?;
    if let Some((file, comp)) = &set.cycles {
        r["frames"]["cycles-bake"] = measure(&frame(set, file, comp, out, "cycles-bake")?, &cycles)?;
    }
    // rask: BK5 raea's own bake of the set (<LightBake>): its frame against the Cycles still, its time and memory, and each mesh's map against Cycles' by map_stops on lightbake.py's UVs
    r["frames"]["raea-bake"] = json!({ "made": false, "why": "raea's bake isn't built yet (BK1-BK5)" });
    let maps: Value = std::fs::read_to_string(baked.join("set.lightmaps.json")).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default();
    r["meshes"] = json!(maps["meshes"].as_array().into_iter().flatten().map(|m| json!({ "mesh": m["mesh"], "size": m["size"], "cycles_exposure": m["lightmap_exposure"] })).collect::<Vec<_>>());
    if set.bar {
        r["bar"] = bar(&r);
    }
    Ok(r)
}

/// The table a person reads: a line a set and frame, then the Cycles bake's cost and the bar.
pub fn table(reports: &[Value]) -> String {
    let mut t = format!("{:<10} {:<12} {:>7} {:>7} {:>7}\n", "set", "frame", "rmse", "stops", "worst");
    for r in reports {
        let name = r["name"].as_str().unwrap_or("");
        if r["reference"]["made"] == json!(false) {
            t += &format!("{name:<10} (no reference: {})\n", r["reference"]["why"].as_str().unwrap_or(""));
            continue;
        }
        for (label, f) in r["frames"].as_object().into_iter().flatten() {
            let n = |k: &str| match f[k].as_f64() {
                Some(x) => format!("{x:.4}"),
                None => "-".into(),
            };
            let why = f["why"].as_str().map(|w| format!("  ({w})")).unwrap_or_default();
            t += &format!("{name:<10} {label:<12} {:>7} {:>7} {:>7}{why}\n", n("rmse"), n("mean_stops"), n("worst_stops"));
        }
        let (lb, st) = (&r["reference"]["lightbake"], &r["reference"]["still"]);
        t += &format!("{name:<10} Cycles at {} samples: lightbake.py {}s (peak {} MB) for {} meshes, the still {}s\n", r["samples"], lb["seconds"], lb["peak_mb"], r["meshes"].as_array().map_or(0, Vec::len), st["seconds"]);
        if let Some(b) = r.get("bar") {
            t += &format!("{name:<10} bar: {}\n", if b["met"] == json!(true) { "met".to_string() } else { format!("not met: {}", b["why"].as_str().unwrap_or("see report.json")) });
        }
    }
    t
}

#[cfg(test)]
mod tests {
    use super::*;

    fn png16(path: &Path, w: u32, h: u32, v: u16) {
        image::ImageBuffer::<image::Rgb<u16>, _>::from_pixel(w, h, image::Rgb([v, v, v])).save(path).unwrap();
    }

    #[test]
    fn a_map_twice_as_bright_is_one_stop_off_and_its_exposure_brings_it_back() {
        let d = std::env::temp_dir().join(format!("raea-lightbake-{}", std::process::id()));
        std::fs::create_dir_all(&d).unwrap();
        let (a, b) = (d.join("a.png"), d.join("b.png"));
        png16(&a, 16, 16, 40000);
        png16(&b, 16, 16, 20000);
        let off = map_stops(&a, 1.0, &b, 1.0).unwrap();
        assert_eq!((off["mean_stops"].as_f64(), off["bias_stops"].as_f64(), off["texels"].as_u64()), (Some(1.0), Some(1.0), Some(256)));
        assert_eq!(map_stops(&a, 1.0, &b, 2.0).unwrap()["mean_stops"].as_f64(), Some(0.0));
        assert!(rmse(&a, &b).unwrap() > 0.3 && rmse(&a, &a).unwrap() == 0.0);
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn the_bar_is_unmet_until_raeas_bake_exists() {
        let r = json!({ "frames": { "raea-bake": { "made": false, "why": "not yet" } } });
        assert_eq!(bar(&r)["met"], json!(false));
        let r = json!({ "frames": { "raea-bake": { "rmse": 0.04, "bake": { "seconds": 4.0 } } }, "meshes": [{ "raea": { "mean_stops": 0.2 } }], "reference": { "lightbake": { "seconds": 20.0 } } });
        assert_eq!(bar(&r)["met"], json!(true));
    }
}
