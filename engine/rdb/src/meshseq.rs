//! Mesh sequences (`.rseq`): a baked simulation as one mesh a frame (goo from tools/goo, a deformed part), played back
//! by the frame (`<MeshSequence>` in @raea/motion/3d; engine/meshseq.rs draws it). Each frame is its own triangles, so
//! the topology may change from frame to frame (a surface reconstructed from particles).
//!
//! The file (little-endian): `RSEQ`, u32 version (1), u32 header length, the header (JSON: `fps`, `frames`, `codec`
//! "zstd" or "raw", `attrs` (position, normal, and color and/or uv), `texture` (a picture beside the file, or null),
//! `index`: [offset, length] of each frame's blob), then the blobs. A blob (after zstd) is u32 verts, u32 tris, f32
//! lo[3], f32 hi[3] (the frame's bounds), u16 positions (verts x 3, lo + (hi - lo) * q / 65535), i16 normals (q /
//! 32767), u8 sRGB rgba (with color), f32 uv (verts x 2, with uv), u32 indices (tris x 3, counter-clockwise outside).

use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

/// A sequence's header, opened once; frames are read from the file as they're asked for.
#[derive(Debug, Clone)]
pub struct Seq {
    pub path: PathBuf,
    pub fps: f64,
    pub zstd: bool,
    pub color: bool,
    pub uv: bool,
    pub texture: Option<PathBuf>,
    pub index: Vec<(u64, u64)>,
}

/// One frame's mesh.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Frame {
    pub pos: Vec<[f32; 3]>,
    pub nrm: Vec<[f32; 3]>,
    /// linear rgba
    pub col: Vec<[f32; 4]>,
    pub uv: Vec<[f32; 2]>,
    pub idx: Vec<u32>,
}

fn srgb_to_linear(c: f32) -> f32 {
    if c <= 0.04045 { c / 12.92 } else { ((c + 0.055) / 1.055).powf(2.4) }
}

fn linear_to_srgb(c: f32) -> f32 {
    let c = c.clamp(0.0, 1.0);
    if c <= 0.0031308 { c * 12.92 } else { 1.055 * c.powf(1.0 / 2.4) - 0.055 }
}

impl Seq {
    pub fn open(path: &Path) -> Result<Seq, String> {
        let err = |e: &dyn std::fmt::Display| format!("{}: {e}", path.display());
        let mut f = std::fs::File::open(path).map_err(|e| err(&e))?;
        let mut head = [0u8; 12];
        f.read_exact(&mut head).map_err(|e| err(&e))?;
        if &head[0..4] != b"RSEQ" {
            return Err(err(&"not a mesh sequence (no RSEQ)"));
        }
        let version = u32::from_le_bytes(head[4..8].try_into().unwrap());
        if version != 1 {
            return Err(err(&format!("mesh sequence version {version}; this build reads 1")));
        }
        let n = u32::from_le_bytes(head[8..12].try_into().unwrap()) as usize;
        let mut json = vec![0u8; n];
        f.read_exact(&mut json).map_err(|e| err(&e))?;
        let h: serde_json::Value = serde_json::from_slice(&json).map_err(|e| err(&e))?;
        let attrs: Vec<&str> = h.get("attrs").and_then(|a| a.as_array()).map(|a| a.iter().filter_map(|x| x.as_str()).collect()).unwrap_or_default();
        let index = h.get("index").and_then(|a| a.as_array()).ok_or(err(&"no index"))?
            .iter()
            .map(|e| (e[0].as_u64().unwrap_or(0), e[1].as_u64().unwrap_or(0)))
            .collect();
        let texture = h.get("texture").and_then(|t| t.as_str()).filter(|t| !t.is_empty()).map(|t| path.parent().unwrap_or(Path::new(".")).join(t));
        Ok(Seq {
            path: path.to_path_buf(),
            fps: h.get("fps").and_then(|v| v.as_f64()).unwrap_or(30.0),
            zstd: h.get("codec").and_then(|v| v.as_str()) != Some("raw"),
            color: attrs.contains(&"color"),
            uv: attrs.contains(&"uv"),
            texture,
            index,
        })
    }

    pub fn frames(&self) -> usize {
        self.index.len()
    }

    /// Frame `k` (clamped to the sequence).
    pub fn read(&self, k: usize) -> Result<Frame, String> {
        if self.index.is_empty() {
            return Ok(Frame::default());
        }
        let (off, len) = self.index[k.min(self.index.len() - 1)];
        let err = |e: &dyn std::fmt::Display| format!("{} frame {k}: {e}", self.path.display());
        let mut f = std::fs::File::open(&self.path).map_err(|e| err(&e))?;
        f.seek(SeekFrom::Start(off)).map_err(|e| err(&e))?;
        let mut blob = vec![0u8; len as usize];
        f.read_exact(&mut blob).map_err(|e| err(&e))?;
        let raw = if self.zstd {
            let mut out = Vec::new();
            let mut d = ruzstd::decoding::StreamingDecoder::new(&blob[..]).map_err(|e| err(&e))?;
            d.read_to_end(&mut out).map_err(|e| err(&e))?;
            out
        } else {
            blob
        };
        decode(&raw, self.color, self.uv).map_err(|e| err(&e))
    }
}

fn decode(b: &[u8], color: bool, uv: bool) -> Result<Frame, String> {
    let mut at = 0usize;
    let mut take = |n: usize| -> Result<&[u8], String> {
        let s = b.get(at..at + n).ok_or("blob too short")?;
        at += n;
        Ok(s)
    };
    let u32_ = |s: &[u8]| u32::from_le_bytes(s.try_into().unwrap());
    let f32_ = |s: &[u8]| f32::from_le_bytes(s.try_into().unwrap());
    let verts = u32_(take(4)?) as usize;
    let tris = u32_(take(4)?) as usize;
    let mut lo = [0f32; 3];
    let mut hi = [0f32; 3];
    for v in lo.iter_mut().chain(hi.iter_mut()) {
        *v = f32_(take(4)?);
    }
    let mut fr = Frame::default();
    let p = take(verts * 6)?;
    fr.pos = (0..verts).map(|i| std::array::from_fn(|a| {
        let q = u16::from_le_bytes([p[i * 6 + a * 2], p[i * 6 + a * 2 + 1]]) as f32 / 65535.0;
        lo[a] + (hi[a] - lo[a]) * q
    })).collect();
    let n = take(verts * 6)?;
    fr.nrm = (0..verts).map(|i| {
        let v: [f32; 3] = std::array::from_fn(|a| i16::from_le_bytes([n[i * 6 + a * 2], n[i * 6 + a * 2 + 1]]) as f32 / 32767.0);
        let l = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt().max(1e-9);
        [v[0] / l, v[1] / l, v[2] / l]
    }).collect();
    if color {
        let c = take(verts * 4)?;
        fr.col = (0..verts).map(|i| [srgb_to_linear(c[i * 4] as f32 / 255.0), srgb_to_linear(c[i * 4 + 1] as f32 / 255.0), srgb_to_linear(c[i * 4 + 2] as f32 / 255.0), c[i * 4 + 3] as f32 / 255.0]).collect();
    }
    if uv {
        let t = take(verts * 8)?;
        fr.uv = (0..verts).map(|i| [f32_(&t[i * 8..i * 8 + 4]), f32_(&t[i * 8 + 4..i * 8 + 8])]).collect();
    }
    let ix = take(tris * 12)?;
    fr.idx = (0..tris * 3).map(|i| u32_(&ix[i * 4..i * 4 + 4])).collect();
    if fr.idx.iter().any(|&i| i as usize >= verts) {
        return Err("an index past the vertices".into());
    }
    Ok(fr)
}

/// A frame as a blob (uncompressed), as `decode` reads it.
pub fn encode(fr: &Frame) -> Vec<u8> {
    let verts = fr.pos.len();
    let mut lo = [f32::MAX; 3];
    let mut hi = [f32::MIN; 3];
    for p in &fr.pos {
        for a in 0..3 {
            lo[a] = lo[a].min(p[a]);
            hi[a] = hi[a].max(p[a]);
        }
    }
    if verts == 0 {
        lo = [0.0; 3];
        hi = [0.0; 3];
    }
    let mut b = Vec::with_capacity(32 + verts * 24 + fr.idx.len() * 4);
    b.extend((verts as u32).to_le_bytes());
    b.extend(((fr.idx.len() / 3) as u32).to_le_bytes());
    for v in lo.iter().chain(hi.iter()) {
        b.extend(v.to_le_bytes());
    }
    for p in &fr.pos {
        for a in 0..3 {
            let span = (hi[a] - lo[a]).max(1e-12);
            b.extend((((p[a] - lo[a]) / span * 65535.0).round().clamp(0.0, 65535.0) as u16).to_le_bytes());
        }
    }
    for i in 0..verts {
        let n = fr.nrm.get(i).copied().unwrap_or([0.0, 1.0, 0.0]);
        for a in 0..3 {
            b.extend(((n[a].clamp(-1.0, 1.0) * 32767.0).round() as i16).to_le_bytes());
        }
    }
    if !fr.col.is_empty() {
        for c in &fr.col {
            b.extend([linear_to_srgb(c[0]), linear_to_srgb(c[1]), linear_to_srgb(c[2]), c[3].clamp(0.0, 1.0)].map(|x| (x * 255.0).round() as u8));
        }
    }
    for t in &fr.uv {
        b.extend(t[0].to_le_bytes());
        b.extend(t[1].to_le_bytes());
    }
    for i in &fr.idx {
        b.extend(i.to_le_bytes());
    }
    b
}

/// Frames written as a sequence (uncompressed: what raea writes itself is small; tools/goo writes zstd).
pub fn write(path: &Path, fps: f64, frames: &[Frame], texture: Option<&str>) -> Result<(), String> {
    let color = frames.iter().any(|f| !f.col.is_empty());
    let uv = frames.iter().any(|f| !f.uv.is_empty());
    let blobs: Vec<Vec<u8>> = frames.iter().map(encode).collect();
    let mut attrs = vec!["position", "normal"];
    if color {
        attrs.push("color");
    }
    if uv {
        attrs.push("uv");
    }
    // the header's length depends on the offsets, which depend on the header's length: fixed-width offsets
    let header = |base: u64| {
        let mut at = base;
        let index: Vec<String> = blobs.iter().map(|b| {
            let e = format!("[{:>12},{:>10}]", at, b.len());
            at += b.len() as u64;
            e
        }).collect();
        format!(
            "{{\"fps\":{fps},\"frames\":{},\"codec\":\"raw\",\"attrs\":{},\"texture\":{},\"index\":[{}]}}",
            frames.len(),
            serde_json::to_string(&attrs).unwrap(),
            texture.map(|t| serde_json::to_string(t).unwrap()).unwrap_or("null".into()),
            index.join(",")
        )
    };
    let len = header(0).len() as u64;
    let h = header(12 + len);
    let mut out = Vec::new();
    out.extend(b"RSEQ");
    out.extend(1u32.to_le_bytes());
    out.extend((h.len() as u32).to_le_bytes());
    out.extend(h.as_bytes());
    for b in &blobs {
        out.extend(b);
    }
    std::fs::write(path, out).map_err(|e| format!("{}: {e}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_written_sequence_reads_back() {
        let tri = Frame { pos: vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 2.0, -1.0]], nrm: vec![[0.0, 0.0, 1.0]; 3], col: vec![[1.0, 0.5, 0.0, 1.0]; 3], uv: vec![], idx: vec![0, 1, 2] };
        let dir = std::env::temp_dir().join(format!("rseq-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join("t.rseq");
        write(&p, 30.0, &[Frame::default(), tri.clone()], None).unwrap();
        let s = Seq::open(&p).unwrap();
        assert_eq!(s.frames(), 2);
        assert!(s.read(0).unwrap().pos.is_empty());
        let f = s.read(1).unwrap();
        assert_eq!(f.idx, tri.idx);
        for (a, b) in f.pos.iter().zip(&tri.pos) {
            for k in 0..3 {
                assert!((a[k] - b[k]).abs() < 1e-4);
            }
        }
        assert!((f.col[0][1] - 0.5).abs() < 0.01);
        // clamped past the end
        assert_eq!(s.read(9).unwrap().idx, tri.idx);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
