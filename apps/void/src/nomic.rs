//! Text by meaning, for searching a space's assets (assets.rs): nomic-embed-text v1.5 (Apache-2.0), its int8 ONNX in
//! raea's model folder (~/.local/share/raea-models/nomic-embed-v1.5/text, RAEA_MODELS moves it), on the same ONNX
//! Runtime as the voice. Transcript windows and documents are embedded as "search_document: …", a query as
//! "search_query: …", mean-pooled over the tokens and unit length, 768 numbers.
//!
//! It runs in a child process (`void embed-text doc|query`, JSON strings in on stdin, one vector per line out), so the
//! window never holds the model: a reindex or a search starts one, and it exits when its input ends.

use std::io::{BufRead, Write};
use std::path::PathBuf;

use ort::session::Session;
use ort::value::Tensor;
use tokenizers::{Tokenizer, TruncationParams};

/// Longer text is cut here (windows are short; the model reads up to 8192, slowly).
const MAX_TOKENS: usize = 512;

pub fn dir() -> PathBuf {
    std::env::var_os("RAEA_MODELS")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(std::env::var("HOME").unwrap_or_default()).join(".local/share/raea-models"))
        .join("nomic-embed-v1.5/text")
}

pub fn available() -> bool {
    dir().join("onnx/model_int8.onnx").exists() || dir().join("onnx/model.onnx").exists()
}

pub struct Nomic {
    session: Session,
    tokenizer: Tokenizer,
    inputs: Vec<String>,
}

impl Nomic {
    pub fn load() -> anyhow::Result<Nomic> {
        let d = dir();
        let file = ["model_int8.onnx", "model.onnx"].iter().map(|f| d.join("onnx").join(f)).find(|f| f.exists())
            .ok_or_else(|| anyhow::anyhow!("no nomic-embed model in {}", d.display()))?;
        let session = raea_void::voice::session(&file)?;
        let mut tokenizer = Tokenizer::from_file(d.join("tokenizer.json")).map_err(|e| anyhow::anyhow!("tokenizer.json: {e}"))?;
        tokenizer.with_padding(None);
        tokenizer.with_truncation(Some(TruncationParams { max_length: MAX_TOKENS, ..Default::default() })).map_err(|e| anyhow::anyhow!("{e}"))?;
        let inputs = session.inputs().iter().map(|i| i.name().to_string()).collect();
        Ok(Nomic { session, tokenizer, inputs })
    }

    /// One text (already prefixed) to its unit vector.
    pub fn embed(&mut self, text: &str) -> anyhow::Result<Vec<f32>> {
        let enc = self.tokenizer.encode(text, true).map_err(|e| anyhow::anyhow!("{e}"))?;
        let n = enc.get_ids().len().max(1);
        let ids: Vec<i64> = enc.get_ids().iter().map(|x| *x as i64).collect();
        let mask: Vec<i64> = vec![1; n];
        let mut feed: Vec<(String, ort::session::SessionInputValue)> = Vec::new();
        for name in &self.inputs {
            let v = match name.as_str() {
                "input_ids" => ids.clone(),
                "attention_mask" => mask.clone(),
                "token_type_ids" => vec![0; n],
                other => anyhow::bail!("nomic-embed wants an input void doesn't know: {other}"),
            };
            feed.push((name.clone(), Tensor::from_array(([1usize, n], v))?.into()));
        }
        let out = self.session.run(feed)?;
        let (shape, data) = out[0].try_extract_tensor::<f32>()?;
        let dim = *shape.last().unwrap_or(&768) as usize;
        // [1, n, dim] token states are mean-pooled; a [1, dim] output is already pooled
        let pooled: Vec<f32> = if shape.len() == 3 {
            let tokens = data.len() / dim;
            (0..dim).map(|j| (0..tokens).map(|t| data[t * dim + j]).sum::<f32>() / tokens as f32).collect()
        } else {
            data[..dim].to_vec()
        };
        let norm = pooled.iter().map(|x| x * x).sum::<f32>().sqrt().max(1e-12);
        Ok(pooled.iter().map(|x| x / norm).collect())
    }
}

/// `void embed-text doc|query`: JSON strings in, one JSON vector (or null) per line out.
pub fn serve(role: &str) -> anyhow::Result<()> {
    let prefix = if role == "query" { "search_query: " } else { "search_document: " };
    let mut model = Nomic::load()?;
    let stdout = std::io::stdout();
    let mut out = stdout.lock();
    for line in std::io::stdin().lock().lines() {
        let text: String = serde_json::from_str(&line?).unwrap_or_default();
        let v = model.embed(&format!("{prefix}{text}")).ok();
        writeln!(out, "{}", serde_json::to_string(&v)?)?;
    }
    Ok(())
}

/// Many texts embedded in one child process (`role` doc or query); each one's vector, or None.
pub fn embed_all(role: &str, texts: &[String]) -> Vec<Option<Vec<f32>>> {
    let none = || vec![None; texts.len()];
    if texts.is_empty() || !available() {
        return none();
    }
    // the child is void itself; a test binary (or anything else) has no `embed-text`
    let Ok(exe) = std::env::current_exe() else { return none() };
    if exe.file_name().is_none_or(|n| n != "void") {
        return none();
    }
    let Ok(mut child) = std::process::Command::new(exe).args(["embed-text", role])
        .stdin(std::process::Stdio::piped()).stdout(std::process::Stdio::piped()).stderr(std::process::Stdio::null()).spawn() else { return none() };
    let input: String = texts.iter().map(|t| serde_json::to_string(t).unwrap_or_else(|_| "\"\"".into()) + "\n").collect();
    // written from a thread, so a long input can't fill the pipe while the child's output waits to be read
    let mut stdin = child.stdin.take();
    let writer = std::thread::spawn(move || { if let Some(s) = stdin.as_mut() { let _ = s.write_all(input.as_bytes()); } });
    let Ok(o) = child.wait_with_output() else { return none() };
    let _ = writer.join();
    let mut lines = String::from_utf8_lossy(&o.stdout).lines().map(|l| serde_json::from_str::<Option<Vec<f32>>>(l).ok().flatten()).collect::<Vec<_>>();
    lines.resize(texts.len(), None);
    lines
}
