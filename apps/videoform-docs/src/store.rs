//! What's kept of a document and of a reference's words (VF8f), beside the videoform's rows: a document's passages in
//! videoform/documents/HASH.json, with two EmbeddingGemma 2 vectors a passage in HASH.f16 (as a query, to find where
//! it's carried out; and for sentence similarity, to find another group's passage saying the same); and what each
//! shot of a reference says and shows, as documents, in videoform/words/HASH.json and HASH.f16. The document itself
//! is never needed again; neither is the reference.

use raea_sys_columns::cache::{f16s, f32s};
use raea_sys_columns::gemma::{self, DIM, Text};
use raea_sys_videoform::rows::Shot;
use raea_sys_videoform_briefs::link::shown;
use raea_sys_videoform_briefs::passages::Passage;
use serde::{Deserialize, Serialize};
use std::path::Path;

/// A document kept: its passages and their vectors
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Doc {
    pub hash: String,
    pub title: String,
    #[serde(default)]
    pub path: String,
    pub read: String,
    /// how its passages were split and embedded (VERSION): one older is read again from its file when that's there,
    /// else embedded again from its passages
    #[serde(default)]
    pub version: u32,
    pub passages: Vec<Passage>,
    #[serde(skip)]
    pub find: Vec<Vec<f32>>,
    #[serde(skip)]
    pub same: Vec<Vec<f32>>,
}

/// Bumped when passages are split or embedded otherwise (2: a passage set beside another group's by its own words,
/// not its heading; a bullet never a heading; 3: a heading starts as a title does)
pub const VERSION: u32 = 3;

/// EmbeddingGemma 2, loaded on first use
#[derive(Default)]
pub struct Gem(Option<Text>);

impl Gem {
    pub fn embed(&mut self, texts: &[String]) -> Result<Vec<Vec<f32>>, String> {
        if texts.is_empty() {
            return Ok(Vec::new());
        }
        if self.0.is_none() {
            eprintln!("raea videoform: loading EmbeddingGemma 2");
            let (m, t) = raea_sys_columns::models::gemma().map_err(|e| format!("EmbeddingGemma 2: {e:#}"))?;
            self.0 = Some(Text::load(&m, &t).map_err(|e| format!("EmbeddingGemma 2: {e:#}"))?);
        }
        let flat = self.0.as_mut().unwrap().embed(texts).map_err(|e| format!("EmbeddingGemma 2: {e:#}"))?;
        Ok(flat.chunks(DIM).map(<[f32]>::to_vec).collect())
    }

    /// Each passage's two vectors: as a query for what's said and shown, and for passages saying the same
    pub fn passages(&mut self, ps: &[Passage]) -> Result<(Vec<Vec<f32>>, Vec<Vec<f32>>), String> {
        let find: Vec<String> = ps.iter().map(|p| gemma::as_query(&p.said())).collect();
        // what a passage says, beside another group's, is its own words: two briefs' headings are the same template
        let same: Vec<String> = ps.iter().map(|p| format!("task: sentence similarity | query: {}", p.text)).collect();
        Ok((self.embed(&find)?, self.embed(&same)?))
    }
}

/// Where documents are kept
const DOCS: &str = "videoform/documents";

/// A JSON file read, when it's there and reads
pub fn json_at<T: serde::de::DeserializeOwned>(p: &Path) -> Option<T> {
    let bytes = std::fs::read(p).ok()?;
    serde_json::from_slice(&bytes).ok()
}

/// The sha256 of some bytes, in hex
pub fn sha(b: &[u8]) -> String {
    use sha2::Digest;
    sha2::Sha256::digest(b).iter().map(|x| format!("{x:02x}")).collect()
}

impl Doc {
    pub fn save(&self, dir: &Path) -> Result<(), String> {
        let d = dir.join(DOCS);
        std::fs::create_dir_all(&d).map_err(|e| e.to_string())?;
        std::fs::write(d.join(format!("{}.json", self.hash)), serde_json::to_vec(self).unwrap()).map_err(|e| e.to_string())?;
        let flat: Vec<f32> = self.find.iter().chain(&self.same).flatten().copied().collect();
        std::fs::write(d.join(format!("{}.f16", self.hash)), f16s(&flat)).map_err(|e| e.to_string())
    }

    /// Whether a document is kept
    pub fn kept(dir: &Path, hash: &str) -> bool {
        dir.join(DOCS).join(format!("{hash}.json")).is_file()
    }

    /// A document kept, with its vectors; one kept at an older VERSION read again (or embedded again) now
    pub fn load(dir: &Path, hash: &str, gem: &mut Gem) -> Option<Doc> {
        let d = dir.join(DOCS);
        let mut doc: Doc = json_at(&d.join(format!("{hash}.json")))?;
        let v = std::fs::read(d.join(format!("{hash}.f16"))).map(|b| f32s(&b)).unwrap_or_default();
        let n = doc.passages.len();
        if doc.version != VERSION || v.len() != 2 * n * DIM {
            if let Ok(ps) = crate::read::passages(Path::new(&doc.path)) {
                doc.passages = ps;
            }
            eprintln!("raea videoform: {} read again (passages {VERSION})", doc.title);
            return Doc::make(dir, doc.hash, doc.title, doc.path, doc.passages, gem, &doc.read).ok();
        }
        let rows: Vec<Vec<f32>> = v.chunks(DIM).map(<[f32]>::to_vec).collect();
        doc.same = rows[n..].to_vec();
        doc.find = rows[..n].to_vec();
        Some(doc)
    }

    /// Read and embedded now, kept
    pub fn make(dir: &Path, hash: String, title: String, path: String, passages: Vec<Passage>, gem: &mut Gem, today: &str) -> Result<Doc, String> {
        let (find, same) = gem.passages(&passages)?;
        let d = Doc { hash, title, path, read: today.into(), version: VERSION, passages, find, same };
        d.save(dir)?;
        Ok(d)
    }
}

/// What each shot of a reference says and shows, as vectors (empty where nothing), from videoform/words/ when it was
/// made from these words, else embedded now and kept
pub fn words(dir: &Path, media: &str, shots: &[Shot], gem: &mut Gem) -> Result<(Vec<Vec<f32>>, Vec<Vec<f32>>), String> {
    let texts: Vec<String> = shots.iter().map(|s| s.words.trim().to_string()).chain(shots.iter().map(shown)).collect();
    let d = dir.join("videoform/words");
    let (json, halfs) = (d.join(format!("{media}.json")), d.join(format!("{media}.f16")));
    let kept: Option<Vec<String>> = json_at(&json);
    let said: Vec<usize> = (0..texts.len()).filter(|&i| !texts[i].is_empty()).collect();
    let vecs: Vec<Vec<f32>> = match (kept.as_ref() == Some(&texts), std::fs::read(&halfs).ok().map(|b| f32s(&b))) {
        (true, Some(v)) if v.len() == said.len() * DIM => v.chunks(DIM).map(<[f32]>::to_vec).collect(),
        _ => {
            let docs: Vec<String> = said.iter().map(|&i| gemma::as_document(&texts[i], "")).collect();
            let v = gem.embed(&docs)?;
            std::fs::create_dir_all(&d).map_err(|e| e.to_string())?;
            std::fs::write(&halfs, f16s(&v.concat())).map_err(|e| e.to_string())?;
            std::fs::write(&json, serde_json::to_vec(&texts).unwrap()).map_err(|e| e.to_string())?;
            v
        }
    };
    let mut all = vec![Vec::new(); texts.len()];
    for (k, &i) in said.iter().enumerate() {
        all[i] = vecs[k].clone();
    }
    let shown = all.split_off(shots.len());
    Ok((all, shown))
}
