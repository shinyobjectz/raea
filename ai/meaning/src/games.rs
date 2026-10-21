//! The game library (sdk/motion/docs/game-library.md): a small catalogue of Bevy examples, open-source games, game-design
//! patterns and free 3D assets, searched by meaning. One JSON record per line in `catalogue.jsonl`; `index.bin` holds
//! two int8 SigLIP 2 vectors per record (its words, and its thumbnail when it has one), built by `games-index`.
//!
//!   raea-meaning games-find "low-poly knight with sword animations" [-k 8] [--kind asset] [--download]
//!   raea-meaning games-show ID                      the whole record
//!   raea-meaning games-fetch ID [--file REL] [--into DIR]  a downloadable record's files into ~/.raea/assets/games/ID/,
//!                                                   and its model (main, or REL in a pack) copied into DIR
//!   raea-meaning games-index [LIBDIR]               rebuild index.bin + index.json from the catalogue and thumbs/
//!
//! A query is scored three ways and the three are added as z-scores: its SigLIP text vector against each record's text
//! vector, the same against each thumbnail (SigLIP was trained text-to-image, so pictures answer looks: "low-poly",
//! "cartoon knight"), and BM25 over the title, tags and description (names: "breakout", "coyote time").

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Command;

use serde_json::{json, Value};

/// index.bin: "RGL2", n, dim (u32 LE); n text vectors (f32 scale + dim int8); m; m x (u32 record, f32 scale, dim int8)
const MAGIC: &[u8; 4] = b"RGL2";
/// Licences whose files may be fetched; everything else is linked only.
const DOWNLOADABLE: &[&str] = &["CC0-1.0", "CC-BY-3.0", "CC-BY-4.0", "MIT", "Apache-2.0", "Unlicense", "MIT OR Apache-2.0"];

/// The library folder: $RAEA_GAMES_LIB, else the repo's sdk/motion/library/games, else the installed ~/.raea/library/games.
pub fn lib_dir() -> PathBuf {
    if let Some(d) = std::env::var_os("RAEA_GAMES_LIB") {
        return PathBuf::from(d);
    }
    let repo = PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/../motion/library/games"));
    if repo.join("catalogue.jsonl").exists() {
        return repo;
    }
    home().join(".raea/library/games")
}

fn home() -> PathBuf {
    PathBuf::from(std::env::var("HOME").unwrap_or_default())
}

/// Where fetched files go: $RAEA_GAMES_ASSETS, else ~/.raea/assets/games.
pub fn assets_dir() -> PathBuf {
    std::env::var_os("RAEA_GAMES_ASSETS").map(PathBuf::from).unwrap_or_else(|| home().join(".raea/assets/games"))
}

pub fn load_catalogue(dir: &Path) -> Result<(Vec<Value>, u64), String> {
    let p = dir.join("catalogue.jsonl");
    let text = std::fs::read_to_string(&p).map_err(|e| format!("{}: {e}", p.display()))?;
    let mut out = Vec::new();
    for (i, line) in text.lines().enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        let v: Value = serde_json::from_str(line).map_err(|e| format!("{}:{}: {e}", p.display(), i + 1))?;
        if v["id"].as_str().is_none() || v["kind"].as_str().is_none() || v["title"].as_str().is_none() {
            return Err(format!("{}:{}: a record needs id, kind and title", p.display(), i + 1));
        }
        out.push(v);
    }
    Ok((out, fnv(text.as_bytes())))
}

fn fnv(b: &[u8]) -> u64 {
    b.iter().fold(0xcbf29ce484222325u64, |h, &x| (h ^ x as u64).wrapping_mul(0x100000001b3))
}

fn s<'a>(v: &'a Value, k: &str) -> &'a str {
    v[k].as_str().unwrap_or("")
}

fn tags(v: &Value) -> Vec<String> {
    v["tags"].as_array().map(|a| a.iter().filter_map(|t| t.as_str().map(String::from)).collect()).unwrap_or_default()
}

/// The two phrases a record's text vector averages: what it is, and what it's tagged. SigLIP reads 64 tokens.
fn phrases(v: &Value) -> [String; 2] {
    [format!("{}. {}", s(v, "title"), s(v, "description")), format!("{} {}: {}", s(v, "kind"), s(v, "title"), tags(v).join(", "))]
}

fn quantise(v: &[f32]) -> (f32, Vec<i8>) {
    let m = v.iter().fold(0f32, |m, x| m.max(x.abs())).max(1e-12);
    (m / 127.0, v.iter().map(|x| (x / m * 127.0).round() as i8).collect())
}

pub struct Index {
    pub dim: usize,
    pub text: Vec<Vec<f32>>,
    pub image: Vec<Option<Vec<f32>>>,
}

fn read_index(dir: &Path, n: usize, hash: u64) -> Result<Index, String> {
    let meta: Value = serde_json::from_str(&std::fs::read_to_string(dir.join("index.json")).map_err(|e| format!("index.json: {e} (run games-index)"))?).map_err(|e| e.to_string())?;
    if meta["catalogue_fnv"].as_str() != Some(&format!("{hash:016x}")) {
        return Err("index.json is older than catalogue.jsonl: run `raea-meaning games-index`".into());
    }
    let b = std::fs::read(dir.join("index.bin")).map_err(|e| format!("index.bin: {e}"))?;
    if b.len() < 12 || &b[..4] != MAGIC {
        return Err("index.bin: not a game-library index (run games-index)".into());
    }
    let u32_at = |at: usize| b.get(at..at + 4).map(|x| u32::from_le_bytes(x.try_into().unwrap()) as usize);
    let (rn, dim) = (u32_at(4).unwrap(), u32_at(8).unwrap());
    let vec_at = |at: usize| -> Vec<f32> {
        let scale = f32::from_le_bytes(b[at..at + 4].try_into().unwrap());
        b[at + 4..at + 4 + dim].iter().map(|&x| x as i8 as f32 * scale).collect()
    };
    let m_at = 12 + n * (4 + dim);
    let m = u32_at(m_at).ok_or("index.bin: truncated")?;
    if rn != n || b.len() != m_at + 4 + m * (4 + 4 + dim) {
        return Err(format!("index.bin: {rn} records of {dim}, the catalogue has {n} (run games-index)"));
    }
    let text: Vec<Vec<f32>> = (0..n).map(|i| vec_at(12 + i * (4 + dim))).collect();
    let mut image: Vec<Option<Vec<f32>>> = vec![None; n];
    for j in 0..m {
        let at = m_at + 4 + j * (8 + dim);
        let i = u32_at(at).unwrap();
        if i < n {
            image[i] = Some(vec_at(at + 4));
        }
    }
    Ok(Index { dim, text, image })
}

pub fn build_index(dir: &Path) -> Result<(), String> {
    let (recs, hash) = load_catalogue(dir)?;
    let models = raea_embed::default_dir();
    let mut text = raea_embed::Text::load(&models)?;
    let mut vision = raea_embed::Vision::load(&models, false)?;
    let mut out: Vec<u8> = Vec::new();
    out.extend_from_slice(MAGIC);
    out.extend_from_slice(&(recs.len() as u32).to_le_bytes());
    out.extend_from_slice(&(raea_embed::DIM as u32).to_le_bytes());
    let mut ids = Vec::new();
    let mut images: Vec<(u32, f32, Vec<i8>)> = Vec::new();
    for (i, r) in recs.iter().enumerate() {
        let [a, b] = phrases(r);
        let (va, vb) = (text.embed(&a)?, text.embed(&b)?);
        let t = raea_embed::normalised(&va.iter().zip(&vb).map(|(x, y)| x + y).collect::<Vec<_>>());
        let (sc, q) = quantise(&t);
        out.extend_from_slice(&sc.to_le_bytes());
        out.extend(q.iter().map(|&x| x as u8));
        let thumb = r["thumbnail"].as_str().filter(|t| !t.starts_with("http")).map(|t| dir.join(t));
        if let Some(p) = thumb.filter(|p| p.exists()) {
            let (sc, q) = quantise(&vision.embed_file(&p)?);
            images.push((i as u32, sc, q));
        }
        ids.push(s(r, "id").to_string());
    }
    // then only the records that have a thumbnail: (record index, scale, vector)
    let thumbs = images.len();
    out.extend_from_slice(&(thumbs as u32).to_le_bytes());
    for (i, sc, q) in images {
        out.extend_from_slice(&i.to_le_bytes());
        out.extend_from_slice(&sc.to_le_bytes());
        out.extend(q.iter().map(|&x| x as u8));
    }
    std::fs::write(dir.join("index.bin"), &out).map_err(|e| e.to_string())?;
    let meta = json!({"model": raea_embed::MODEL_ID, "dim": raea_embed::DIM, "n": recs.len(), "thumbnails": thumbs, "quant": "int8, one f32 scale per vector; image vectors only for records with thumbnails",
                      "catalogue_fnv": format!("{hash:016x}"), "ids": ids});
    std::fs::write(dir.join("index.json"), serde_json::to_string(&meta).unwrap()).map_err(|e| e.to_string())?;
    eprintln!("{} records ({thumbs} with thumbnails) -> {} ({} bytes)", recs.len(), dir.join("index.bin").display(), out.len());
    Ok(())
}

fn words(t: &str) -> Vec<String> {
    t.to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| w.len() > 1 && !STOP.contains(w))
        .map(|w| if w.len() > 3 && w.ends_with('s') && !w.ends_with("ss") { w[..w.len() - 1].to_string() } else { w.to_string() })
        .collect()
}

const STOP: &[&str] = &["a", "an", "the", "and", "or", "of", "to", "in", "on", "for", "with", "by", "is", "it", "its", "as", "at", "from", "that", "this", "be", "are", "can", "how"];

/// BM25 of the query over each record's title (x3), tags (x2), description and design-pattern text.
fn bm25(recs: &[Value], q: &str) -> Vec<f32> {
    let docs: Vec<Vec<String>> = recs
        .iter()
        .map(|r| {
            let mut w = Vec::new();
            for _ in 0..3 {
                w.extend(words(s(r, "title")));
            }
            for _ in 0..2 {
                w.extend(words(&tags(r).join(" ")));
            }
            w.extend(words(s(r, "description")));
            w.extend(words(s(r, "pattern")));
            w.extend(words(&r["patterns"].as_array().map(|a| a.iter().filter_map(|x| x.as_str()).collect::<Vec<_>>().join(" ")).unwrap_or_default()));
            w
        })
        .collect();
    let n = docs.len() as f32;
    let avg = docs.iter().map(|d| d.len()).sum::<usize>() as f32 / n.max(1.0);
    let mut df: HashMap<&str, f32> = HashMap::new();
    for d in &docs {
        let mut seen: Vec<&str> = d.iter().map(String::as_str).collect();
        seen.sort();
        seen.dedup();
        for w in seen {
            *df.entry(w).or_default() += 1.0;
        }
    }
    let qw = words(q);
    docs.iter()
        .map(|d| {
            qw.iter()
                .map(|w| {
                    let f = d.iter().filter(|x| *x == w).count() as f32;
                    if f == 0.0 {
                        return 0.0;
                    }
                    let idf = ((n - df[w.as_str()] + 0.5) / (df[w.as_str()] + 0.5) + 1.0).ln();
                    idf * f * 2.2 / (f + 1.2 * (0.25 + 0.75 * d.len() as f32 / avg))
                })
                .sum()
        })
        .collect()
}

fn z(x: &[f32]) -> Vec<f32> {
    let n = x.len().max(1) as f32;
    let m = x.iter().sum::<f32>() / n;
    let sd = (x.iter().map(|v| (v - m) * (v - m)).sum::<f32>() / n).sqrt().max(1e-6);
    x.iter().map(|v| (v - m) / sd).collect()
}

pub struct Hit {
    pub i: usize,
    pub score: f32,
    pub parts: [f32; 3],
}

/// Every record against the query, best first, after the kind and download filters.
pub fn search(recs: &[Value], idx: &Index, qv: &[f32], q: &str, kinds: &[String], download_only: bool, k: usize) -> Vec<Hit> {
    let dt: Vec<f32> = idx.text.iter().map(|v| crate::k::dot(v, qv)).collect();
    // a record without a thumbnail stands at its text score there, so pictures add evidence without being required
    let zt = z(&dt);
    let with: Vec<f32> = idx.image.iter().filter_map(|v| v.as_ref().map(|v| crate::k::dot(v, qv))).collect();
    let (m, sd) = {
        let n = with.len().max(1) as f32;
        let m = with.iter().sum::<f32>() / n;
        (m, (with.iter().map(|v| (v - m) * (v - m)).sum::<f32>() / n).sqrt().max(1e-6))
    };
    let zi: Vec<f32> = idx.image.iter().enumerate().map(|(i, v)| v.as_ref().map(|v| (crate::k::dot(v, qv) - m) / sd).unwrap_or(zt[i])).collect();
    let zb = z(&bm25(recs, q));
    let mut hits: Vec<Hit> = (0..recs.len())
        .filter(|&i| kinds.is_empty() || kinds.iter().any(|k| k == s(&recs[i], "kind")))
        .filter(|&i| !download_only || s(&recs[i], "ship") == "download")
        .map(|i| Hit { i, score: zt[i] + 0.6 * zi[i] + 0.5 * zb[i], parts: [dt[i], idx.image[i].as_ref().map(|v| crate::k::dot(v, qv)).unwrap_or(f32::NAN), zb[i]] })
        .collect();
    hits.sort_by(|a, b| b.score.total_cmp(&a.score).then(a.i.cmp(&b.i)));
    hits.truncate(k);
    hits
}

/// The fields an agent needs to choose, in a fixed order; long ones (tsx source, patterns) stay in games-show.
fn summary(r: &Value, score: f32) -> Value {
    let mut o = serde_json::Map::new();
    o.insert("score".into(), json!((score * 1000.0).round() / 1000.0));
    for k in ["id", "kind", "title", "description", "pattern", "licence", "attribution", "licence_note", "ship", "link_reason", "download_bytes", "source", "repo", "format", "textures", "polycount", "rig", "animations", "raea", "main", "contents", "tsx", "topic"] {
        if let Some(v) = r.get(k).filter(|v| !v.is_null() && v.as_str() != Some("")) {
            o.insert(k.into(), v.clone());
        }
    }
    let local = assets_dir().join(s(r, "id"));
    if local.exists() {
        o.insert("local".into(), json!(local.join(s(r, "main")).display().to_string()));
    }
    Value::Object(o)
}

pub fn find(args: &[String]) -> Result<(), String> {
    let flag = |n: &str| args.iter().position(|a| a == n).and_then(|i| args.get(i + 1)).cloned();
    let q = args.get(1).ok_or("games-find \"TEXT\" [-k 8] [--kind asset,animation,...] [--download]")?;
    let dir = flag("--lib").map(PathBuf::from).unwrap_or_else(lib_dir);
    let (recs, hash) = load_catalogue(&dir)?;
    let idx = read_index(&dir, recs.len(), hash)?;
    let mut text = raea_embed::Text::load(&raea_embed::default_dir())?;
    let qv = text.embed(q)?;
    if qv.len() != idx.dim {
        return Err("the index was built with another model size".into());
    }
    let kinds: Vec<String> = flag("--kind").map(|k| k.split(',').map(String::from).collect()).unwrap_or_default();
    let k = flag("-k").and_then(|v| v.parse().ok()).unwrap_or(8);
    let hits = search(&recs, &idx, &qv, q, &kinds, args.iter().any(|a| a == "--download"), k);
    let verbose = args.iter().any(|a| a == "--why");
    let out: Vec<Value> = hits
        .iter()
        .map(|h| {
            let mut v = summary(&recs[h.i], h.score);
            if verbose {
                v["why"] = json!({"text": h.parts[0], "image": if h.parts[1].is_nan() { Value::Null } else { json!(h.parts[1]) }, "bm25_z": h.parts[2]});
            }
            v
        })
        .collect();
    println!("{}", serde_json::to_string_pretty(&out).unwrap());
    Ok(())
}

fn record(dir: &Path, id: &str) -> Result<Value, String> {
    let (recs, _) = load_catalogue(dir)?;
    recs.into_iter().find(|r| s(r, "id") == id).ok_or_else(|| format!("no record {id} in {}", dir.join("catalogue.jsonl").display()))
}

pub fn show(args: &[String]) -> Result<(), String> {
    let dir = lib_dir();
    let r = record(&dir, args.get(1).ok_or("games-show ID")?)?;
    let mut r = r;
    // a TSX equivalent kept as a file comes inline, so one call gives the agent the code
    if let Some(t) = r["tsx"].as_str().map(String::from) {
        if let Ok(code) = std::fs::read_to_string(dir.join(&t)) {
            r["tsx_source"] = json!(code);
        }
    }
    println!("{}", serde_json::to_string_pretty(&r).unwrap());
    Ok(())
}

/// The relative URIs a .gltf or .glb names (buffers and images), percent-decoded for spaces; data URIs and anything
/// climbing out of the folder are left out.
fn external_uris(model: &Path) -> Result<Vec<String>, String> {
    let b = std::fs::read(model).map_err(|e| format!("{}: {e}", model.display()))?;
    let json = if b.starts_with(b"glTF") && b.len() > 20 {
        let n = u32::from_le_bytes(b[12..16].try_into().unwrap()) as usize;
        &b[20..(20 + n).min(b.len())]
    } else {
        &b[..]
    };
    let j: Value = serde_json::from_slice(json).map_err(|e| format!("{}: {e}", model.display()))?;
    Ok(j["buffers"].as_array().into_iter().flatten().chain(j["images"].as_array().into_iter().flatten())
        .filter_map(|x| x["uri"].as_str())
        .filter(|u| !u.starts_with("data:") && !u.contains("..") && !u.starts_with('/'))
        .map(|u| u.replace("%20", " "))
        .collect())
}

fn find_named(dir: &Path, name: &str) -> Option<PathBuf> {
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        for e in std::fs::read_dir(&d).into_iter().flatten().flatten() {
            let p = e.path();
            if p.is_dir() {
                stack.push(p);
            } else if p.file_name().is_some_and(|n| n == name) {
                return Some(p);
            }
        }
    }
    None
}

/// The glb and gltf files under a fetched record's folder, relative to it, sorted.
fn model_files(dir: &Path) -> Vec<String> {
    let mut out = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        for e in std::fs::read_dir(&d).into_iter().flatten().flatten() {
            let p = e.path();
            if p.is_dir() {
                stack.push(p);
            } else if p.extension().is_some_and(|x| x == "glb" || x == "gltf") {
                out.push(p.strip_prefix(dir).unwrap_or(&p).display().to_string());
            }
        }
    }
    out.sort();
    out
}

fn sha256(p: &Path) -> Result<String, String> {
    let out = Command::new("shasum").args(["-a", "256"]).arg(p).output().map_err(|e| format!("shasum: {e}"))?;
    Ok(String::from_utf8_lossy(&out.stdout).split_whitespace().next().unwrap_or("").to_string())
}

/// Fetch a record's files by their recorded URLs, check sizes and hashes, unpack zips, and write ATTRIBUTION.txt.
/// Only records marked `ship: download` with a licence on the list are fetched.
pub fn fetch(args: &[String]) -> Result<(), String> {
    let flag = |n: &str| args.iter().position(|a| a == n).and_then(|i| args.get(i + 1)).cloned();
    let id = args.get(1).ok_or("games-fetch ID [--file REL] [--into DIR]")?;
    let r = record(&lib_dir(), id)?;
    let lic = s(&r, "licence");
    if s(&r, "ship") != "download" || !DOWNLOADABLE.contains(&lic) {
        return Err(format!("{id} is reference-only ({lic}, ship {}): open {} instead", s(&r, "ship"), s(&r, "source")));
    }
    let files = r["files"].as_array().filter(|f| !f.is_empty()).ok_or_else(|| format!("{id} records no files to fetch: get it from {}", s(&r, "source")))?;
    let dest = assets_dir().join(id);
    std::fs::create_dir_all(&dest).map_err(|e| format!("{}: {e}", dest.display()))?;
    for f in files {
        let url = f["url"].as_str().ok_or("a file without a url")?;
        if !url.starts_with("https://") {
            return Err(format!("{url}: only https URLs are fetched"));
        }
        let name = f["path"].as_str().map(String::from).unwrap_or_else(|| url.rsplit('/').next().unwrap_or("file").to_string());
        if name.contains("..") || name.starts_with('/') {
            return Err(format!("{name}: a file path must stay inside the record's folder"));
        }
        let out = dest.join(&name);
        let want = f["sha256"].as_str().unwrap_or("");
        if out.exists() && (want.is_empty() || sha256(&out)? == want) {
            continue;
        }
        // a zip already unpacked (an asset pack ships the files without their zip) isn't fetched again
        let marker = dest.join(format!(".unpacked-{}", name.replace('/', "_")));
        if name.ends_with(".zip") && marker.exists() {
            continue;
        }
        if let Some(p) = out.parent() {
            std::fs::create_dir_all(p).map_err(|e| e.to_string())?;
        }
        let part = out.with_extension("part");
        let st = Command::new("curl").args(["-fsSL", "--retry", "2", "--max-filesize", "500000000", "-o"]).arg(&part).arg(url).status().map_err(|e| format!("curl: {e}"))?;
        if !st.success() {
            let _ = std::fs::remove_file(&part);
            return Err(format!("{url}: download failed"));
        }
        let got = sha256(&part)?;
        if !want.is_empty() && got != want {
            let _ = std::fs::remove_file(&part);
            return Err(format!("{url}: sha256 {got}, the catalogue records {want}; not kept"));
        }
        std::fs::rename(&part, &out).map_err(|e| e.to_string())?;
        if name.ends_with(".zip") {
            let st = Command::new("unzip").args(["-q", "-o"]).arg(&out).arg("-d").arg(&dest).status().map_err(|e| format!("unzip: {e}"))?;
            if !st.success() {
                return Err(format!("{}: unzip failed", out.display()));
            }
            std::fs::write(&marker, got.as_bytes()).map_err(|e| e.to_string())?;
        }
    }
    let attribution = format!(
        "{}\nby {}\nlicence: {}\nsource: {}\nfetched by raea-meaning games-fetch from the URLs in sdk/motion/library/games/catalogue.jsonl\n",
        s(&r, "title"),
        if s(&r, "attribution").is_empty() { s(&r, "author") } else { s(&r, "attribution") },
        lic,
        s(&r, "source")
    );
    std::fs::write(dest.join("ATTRIBUTION.txt"), attribution).map_err(|e| e.to_string())?;
    // the model to use: --file REL picks one out of a pack, else the record's main
    let pick = flag("--file").unwrap_or_else(|| s(&r, "main").to_string());
    if pick.contains("..") {
        return Err(format!("{pick}: a file path must stay inside the record's folder"));
    }
    let main = dest.join(&pick);
    let models = model_files(&dest);
    let mut done = json!({"id": id, "dir": dest.display().to_string(), "licence": lic, "attribution": s(&r, "attribution")});
    if !pick.is_empty() && main.is_file() {
        done["main"] = json!(main.display().to_string());
    } else {
        // a pack: say which models it holds, so the caller can choose one with --file
        done["models"] = json!(models.iter().take(60).collect::<Vec<_>>());
        done["model_count"] = json!(models.len());
    }
    if let Some(into) = flag("--into") {
        if !main.is_file() {
            return Err(format!("{id} is a pack of {} models: choose one with --file (e.g. --file {})", models.len(), models.first().map(String::as_str).unwrap_or("REL")));
        }
        let name = main.file_name().ok_or("no file name")?.to_string_lossy().to_string();
        let mut copied = vec![name.clone()];
        let into = Path::new(&into);
        std::fs::create_dir_all(into).map_err(|e| format!("{}: {e}", into.display()))?;
        std::fs::copy(&main, into.join(&name)).map_err(|e| format!("{} -> {}: {e}", main.display(), into.display()))?;
        // a glTF names its external buffers and pictures by relative path (a .glb can too: Kenney's point at
        // Textures/colormap.png): they come along, found beside the model or, failing that, by name in the pack
        for u in external_uris(&main)? {
            let from = Some(main.parent().unwrap().join(&u)).filter(|p| p.is_file()).or_else(|| find_named(&dest, Path::new(&u).file_name()?.to_str()?));
            let Some(from) = from else { return Err(format!("{}: names {u}, which the pack doesn't have", main.display())) };
            let to = into.join(&u);
            if let Some(p) = to.parent() {
                std::fs::create_dir_all(p).map_err(|e| e.to_string())?;
            }
            std::fs::copy(&from, &to).map_err(|e| format!("{} -> {}: {e}", from.display(), to.display()))?;
            copied.push(u);
        }
        done["copied"] = json!(copied.iter().map(|c| into.join(c).display().to_string()).collect::<Vec<_>>());
        done["file"] = json!(name);
    }
    println!("{}", serde_json::to_string_pretty(&done).unwrap());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bm25_prefers_the_named_record() {
        let recs = vec![
            json!({"id": "a", "kind": "pattern", "title": "Coyote time", "tags": ["platformer", "jump"], "description": "Let the player jump shortly after leaving a ledge."}),
            json!({"id": "b", "kind": "game", "title": "Breakout", "tags": ["arcade"], "description": "Bounce a ball off a paddle to break bricks."}),
        ];
        let s = bm25(&recs, "coyote time jumps");
        assert!(s[0] > 0.0 && s[1] == 0.0);
    }

    #[test]
    fn quantised_vectors_keep_their_direction() {
        let v = raea_embed::normalised(&(0..768).map(|i| ((i * 37 % 101) as f32 - 50.0) / 50.0).collect::<Vec<_>>());
        let (sc, q) = quantise(&v);
        let back: Vec<f32> = q.iter().map(|&x| x as f32 * sc).collect();
        assert!(crate::k::dot(&v, &raea_embed::normalised(&back)) > 0.999);
    }
}
