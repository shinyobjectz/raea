//! Where models' weights live: ~/.cache/raea-void/models/<name>/, each file fetched once from Hugging Face (the folder
//! void's voice has always used, so its downloads are shared). RAEA_VOID_MODELS points somewhere else (a bench, a
//! bundled app).

use std::path::{Path, PathBuf};

/// One model: a folder name and the Hugging Face files it needs (repo, path in the repo, local file name).
pub struct Spec {
    pub name: &'static str,
    pub files: &'static [(&'static str, &'static str, &'static str)],
}

pub fn root() -> PathBuf {
    if let Some(p) = std::env::var_os("RAEA_VOID_MODELS") {
        return PathBuf::from(p);
    }
    let home = std::env::var_os("HOME").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("."));
    home.join(".cache/raea-void/models")
}

/// The model's folder with every file present, downloading what's missing. `progress` gets (file, bytes so far).
pub fn ensure(spec: &Spec, progress: &mut dyn FnMut(&str, u64)) -> anyhow::Result<PathBuf> {
    let dir = root().join(spec.name);
    std::fs::create_dir_all(&dir)?;
    for (repo, path, local) in spec.files {
        let dest = dir.join(local);
        if dest.exists() {
            continue;
        }
        let url = format!("https://huggingface.co/{repo}/resolve/main/{path}");
        fetch(&url, &dest, &mut |n| progress(local, n))?;
    }
    Ok(dir)
}

fn fetch(url: &str, dest: &Path, progress: &mut dyn FnMut(u64)) -> anyhow::Result<()> {
    use std::io::{Read, Write};
    let mut req = ureq::get(url);
    if let Ok(token) = std::env::var("HF_TOKEN") {
        req = req.set("Authorization", &format!("Bearer {token}"));
    }
    let resp = req.call().map_err(|e| anyhow::anyhow!("{url}: {e}"))?;
    let part = dest.with_extension("part");
    let mut out = std::fs::File::create(&part)?;
    let mut body = resp.into_reader();
    let mut buf = vec![0u8; 1 << 16];
    let mut total = 0u64;
    loop {
        let n = body.read(&mut buf)?;
        if n == 0 {
            break;
        }
        out.write_all(&buf[..n])?;
        total += n as u64;
        progress(total);
    }
    out.flush()?;
    std::fs::rename(&part, dest)?;
    Ok(())
}
