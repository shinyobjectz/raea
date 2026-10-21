//! A source's sound in 4 s blocks, decoded just ahead of the voices that read it and let go behind them (a ten-minute
//! take decoded whole stalled a frame 1.9 s and held 1.2 GB). Live, a loader thread decodes blocks and a voice whose
//! block hasn't arrived plays silence; offline (`sync`), a missing block is decoded where it's wanted, so a bounce never
//! misses one and is the same every time.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex};

pub const BLOCK: f64 = 4.0;

pub type Blocks = HashMap<u64, Arc<(Vec<f32>, Vec<f32>)>>;

pub struct Pcm {
    pub file: PathBuf,
    /// the source's own rate
    pub rate: f64,
    /// seconds of sound in the source
    pub len_s: f64,
    /// offline: missing blocks are decoded on the spot
    pub sync: bool,
    pub blocks: Mutex<Blocks>,
    asked: Mutex<HashSet<u64>>,
}

impl Pcm {
    /// Opens a source: its rate from a first short window, its length from the container.
    pub fn open(file: &Path, sync: bool) -> Option<Arc<Pcm>> {
        let a = raea_media::decode_audio_window(file, 0.0, 0.05).ok().filter(|a| a.len() > 0)?;
        let len_s = raea_media::probe_seconds(file).unwrap_or(f64::MAX);
        Some(Arc::new(Pcm { file: file.to_path_buf(), rate: a.rate as f64, len_s, sync, blocks: Mutex::new(HashMap::new()), asked: Mutex::new(HashSet::new()) }))
    }

    /// A source made in memory (an instrument's stem, a test tone): every block present.
    pub fn from_audio(name: &str, a: &raea_media::Audio) -> Arc<Pcm> {
        let rate = a.rate as f64;
        let bl = (BLOCK * rate) as usize;
        let l = a.channels.first().cloned().unwrap_or_default();
        let r = a.channels.get(1).cloned().unwrap_or_else(|| l.clone());
        let mut blocks = HashMap::new();
        for (k, start) in (0..l.len().max(1)).step_by(bl.max(1)).enumerate() {
            let end = (start + bl).min(l.len());
            blocks.insert(k as u64, Arc::new((l[start..end].to_vec(), r[start..end].to_vec())));
        }
        // a hair under its length, so a voice reading it ends on its last sample (len_s * rate rounded up read one past it)
        Arc::new(Pcm { file: PathBuf::from(format!("mem:{name}")), rate, len_s: (l.len() as f64 - 1e-6).max(0.0) / rate, sync: false, blocks: Mutex::new(blocks), asked: Mutex::new(HashSet::new()) })
    }

    pub fn block_len(&self) -> usize {
        (BLOCK * self.rate) as usize
    }

    fn decode(&self, k: u64) -> Option<Arc<(Vec<f32>, Vec<f32>)>> {
        let a = raea_media::decode_audio_window(&self.file, k as f64 * BLOCK, BLOCK).ok()?;
        let l = a.channels.first().cloned().unwrap_or_default();
        let r = a.channels.get(1).cloned().unwrap_or_else(|| l.clone());
        Some(Arc::new((l, r)))
    }

    /// The (linearly interpolated) frame at `pos` source samples, if its block is in. Offline a missing block is
    /// decoded into `blocks` first.
    pub fn sample(&self, blocks: &mut Blocks, pos: f64) -> Option<(f32, f32)> {
        let bl = self.block_len().max(1);
        let i = pos as usize;
        let k = (i / bl) as u64;
        let j = i % bl;
        if self.sync && !blocks.contains_key(&k) && (k as f64) * BLOCK < self.len_s {
            let b = self.decode(k)?;
            blocks.insert(k, b);
        }
        let b = blocks.get(&k)?;
        let (l, r) = (&b.0, &b.1);
        let t = (pos - i as f64) as f32;
        let (l0, r0) = (*l.get(j)?, r.get(j).copied().unwrap_or(0.0));
        if t == 0.0 {
            return Some((l0, r0));
        }
        // the next sample may sit in the next block
        let (l1, r1) = if j + 1 < l.len() {
            (l[j + 1], r[j + 1])
        } else {
            match blocks.get(&(k + 1)) {
                Some(n) if !n.0.is_empty() => (n.0[0], n.1[0]),
                _ => (l0, r0),
            }
        };
        Some((l0 * (1.0 - t) + l1 * t, r0 * (1.0 - t) + r1 * t))
    }

    /// The blocks a voice at `pos` needs (this one and the next): offline they're decoded now, live they're asked of
    /// the loader. Returns them, so the caller can let the rest go.
    pub fn want(self: &Arc<Self>, pos: f64, loader: Option<&Sender<(Arc<Pcm>, u64)>>) -> [u64; 2] {
        let b = (pos.max(0.0) as usize / self.block_len().max(1)) as u64;
        let need = [b, b + 1];
        for k in need {
            if (k as f64) * BLOCK >= self.len_s || self.blocks.lock().unwrap().contains_key(&k) {
                continue;
            }
            match loader {
                Some(tx) => {
                    if self.asked.lock().unwrap().insert(k) {
                        let _ = tx.send((self.clone(), k));
                    }
                }
                None => {
                    if let Some(blk) = self.decode(k) {
                        self.blocks.lock().unwrap().insert(k, blk);
                    }
                }
            }
        }
        need
    }

    /// Lets go every block but those `keep` names and the one before each (a voice reading near a block's end). The
    /// blocks are freed after the lock is let go: the audio thread waits on it only for a map's insert or remove.
    pub fn retain(&self, keep: &[u64]) {
        if self.file.to_string_lossy().starts_with("mem:") {
            return;
        }
        let gone: Vec<Arc<(Vec<f32>, Vec<f32>)>> = {
            let mut b = self.blocks.lock().unwrap();
            let out: Vec<u64> = b.keys().copied().filter(|k| !(keep.contains(k) || keep.iter().any(|n| *k + 1 == *n))).collect();
            out.iter().filter_map(|k| b.remove(k)).collect()
        };
        drop(gone);
    }

    /// The block holding `pos` (source samples), decoded here if it isn't in (live: a voice starting or jumping, so it
    /// starts on its sample with its sound, as the bounce does). True when it had to be decoded.
    pub fn ensure(&self, pos: f64) -> bool {
        let bl = self.block_len().max(1);
        let k = (pos.max(0.0) as usize / bl) as u64;
        // within a second of the block's end the voice is in the next one before the loader can bring it (a cut
        // starting at 3.9 s of a source read 1271 samples of silence): that one too
        let near_end = (pos.max(0.0) as usize % bl) + self.rate as usize >= bl;
        let next = near_end && self.ensure_block(k + 1);
        self.ensure_block(k) || next
    }

    /// `ensure`, waiting at most `wait` (live: a cold disk, an external drive spinning up, took 6.9 s to give one block
    /// and the frame's thread stood still that long). Past it the block comes when it comes, the voice silent until
    /// then. True when a block was decoded in time.
    pub fn ensure_within(self: &Arc<Self>, pos: f64, wait: std::time::Duration) -> bool {
        let bl = self.block_len().max(1);
        let k = (pos.max(0.0) as usize / bl) as u64;
        let near_end = (pos.max(0.0) as usize % bl) + self.rate as usize >= bl;
        let need: Vec<u64> = [k, k + 1].into_iter().filter(|b| (*b == k || near_end) && (*b as f64) * BLOCK < self.len_s && !self.blocks.lock().unwrap().contains_key(b)).collect();
        if need.is_empty() {
            return false;
        }
        let (tx, rx) = std::sync::mpsc::channel();
        let me = self.clone();
        let _ = std::thread::Builder::new().name("raea mixer block".into()).spawn(move || {
            for b in need {
                me.ensure_block(b);
            }
            let _ = tx.send(());
        });
        rx.recv_timeout(wait).is_ok()
    }

    fn ensure_block(&self, k: u64) -> bool {
        if (k as f64) * BLOCK >= self.len_s || self.blocks.lock().unwrap().contains_key(&k) {
            return false;
        }
        match self.decode(k) {
            Some(b) => {
                self.blocks.lock().unwrap().insert(k, b);
                true
            }
            None => false,
        }
    }
}

/// The loader: blocks decoded off the frame and audio threads.
pub fn loader() -> Sender<(Arc<Pcm>, u64)> {
    let (tx, rx) = std::sync::mpsc::channel::<(Arc<Pcm>, u64)>();
    std::thread::Builder::new()
        .name("raea mixer loader".into())
        .spawn(move || {
            for (pcm, k) in rx {
                if let Some(b) = pcm.decode(k) {
                    pcm.blocks.lock().unwrap().insert(k, b);
                }
                pcm.asked.lock().unwrap().remove(&k);
            }
        })
        .expect("mixer loader thread");
    tx
}
