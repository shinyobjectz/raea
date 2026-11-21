//! A fragmented MP4's frame timing (the DASH streams Instagram and YouTube serve): its moov holds no samples, and each
//! moof's traf says its own, with its decode start (tfdt), each sample's duration and composition offset (trun), and
//! the defaults those fall back to (tfhd, then the moov's trex).

use super::probe::{be32, be64, boxes, child};
use std::path::Path;

/// A track's own id (tkhd)
pub(crate) fn track_id(trak: &[u8]) -> Option<u32> {
    let tkhd = child(trak, b"tkhd")?;
    be32(tkhd, if tkhd.first() == Some(&1) { 20 } else { 12 })
}

/// The track's default sample duration from the moov's mvex/trex
fn trex_duration(moov: &[u8], id: u32) -> Option<u32> {
    let mvex = child(moov, b"mvex")?;
    boxes(mvex).into_iter().filter(|(k, _)| k == b"trex").map(|(_, t)| t).find(|t| be32(t, 4) == Some(id)).and_then(|t| be32(t, 12))
}

/// Each moof's payload, read box by box from the file (seeking past each mdat)
fn moofs(path: &Path) -> Vec<Vec<u8>> {
    use std::io::{Read, Seek, SeekFrom};
    let mut out = Vec::new();
    let Ok(mut f) = std::fs::File::open(path) else { return out };
    let len = f.metadata().map(|m| m.len()).unwrap_or(0);
    let mut at = 0u64;
    while at + 8 <= len {
        let mut h = [0u8; 16];
        if f.seek(SeekFrom::Start(at)).is_err() || f.read_exact(&mut h[..8]).is_err() {
            break;
        }
        let mut size = u32::from_be_bytes([h[0], h[1], h[2], h[3]]) as u64;
        let mut hdr = 8u64;
        if size == 1 {
            if f.read_exact(&mut h[8..]).is_err() {
                break;
            }
            size = u64::from_be_bytes(h[8..].try_into().unwrap());
            hdr = 16;
        } else if size == 0 {
            size = len - at;
        }
        if size < hdr {
            break;
        }
        if &h[4..8] == b"moof" {
            let mut m = vec![0u8; (size - hdr).min(64 << 20) as usize];
            if f.read_exact(&mut m).is_err() {
                break;
            }
            out.push(m);
        }
        at += size;
    }
    out
}

/// The track's samples across the file's fragments: each one's duration, and its presentation time in media units
pub(crate) fn samples(path: &Path, moov: &[u8], id: u32) -> Option<(Vec<u32>, Vec<i64>)> {
    let fallback = trex_duration(moov, id).unwrap_or(0);
    let (mut deltas, mut times) = (Vec::new(), Vec::new());
    let mut next = 0i64;
    for moof in moofs(path) {
        for (_, traf) in boxes(&moof).into_iter().filter(|(k, _)| k == b"traf") {
            let tfhd = child(traf, b"tfhd")?;
            if be32(tfhd, 4) != Some(id) {
                continue;
            }
            let flags = be32(tfhd, 0)? & 0xff_ffff;
            // base data offset (8) and sample description index (4) come before the default duration
            let at = 8 + if flags & 0x1 != 0 { 8 } else { 0 } + if flags & 0x2 != 0 { 4 } else { 0 };
            let default = if flags & 0x8 != 0 { be32(tfhd, at)? } else { fallback };
            if let Some(tfdt) = child(traf, b"tfdt") {
                next = if tfdt.first() == Some(&1) { be64(tfdt, 4)? as i64 } else { be32(tfdt, 4)? as i64 };
            }
            for (_, trun) in boxes(traf).into_iter().filter(|(k, _)| k == b"trun") {
                let tf = be32(trun, 0)? & 0xff_ffff;
                let mut i = 8 + if tf & 0x1 != 0 { 4 } else { 0 } + if tf & 0x4 != 0 { 4 } else { 0 };
                for _ in 0..be32(trun, 4)? {
                    let mut field = |on: u32| -> Option<Option<u32>> {
                        if tf & on == 0 {
                            return Some(None);
                        }
                        let v = be32(trun, i)?;
                        i += 4;
                        Some(Some(v))
                    };
                    let d = field(0x100)?.unwrap_or(default);
                    let _size = field(0x200)?;
                    let _flags = field(0x400)?;
                    // signed, as ctts's are read
                    let off = field(0x800)?.map_or(0, |o| o as i32 as i64);
                    deltas.push(d);
                    times.push(next + off);
                    next += d as i64;
                }
            }
        }
    }
    (!deltas.is_empty()).then_some((deltas, times))
}

#[cfg(test)]
mod tests {
    fn boxed(kind: &[u8; 4], body: &[u8]) -> Vec<u8> {
        let mut b = ((body.len() + 8) as u32).to_be_bytes().to_vec();
        b.extend_from_slice(kind);
        b.extend_from_slice(body);
        b
    }

    #[test]
    fn a_fragmented_files_samples_come_from_its_moofs() {
        // track 1, a default duration of 512 in its tfhd; two fragments of 2 and 1 samples, the second with its own
        // duration and a composition offset of 1024
        let tfhd = |flags: u32| [flags.to_be_bytes(), 1u32.to_be_bytes(), 512u32.to_be_bytes()].concat();
        let frag = |start: u32, trun: Vec<u8>| boxed(b"moof", &boxed(b"traf", &[boxed(b"tfhd", &tfhd(0x8)), boxed(b"tfdt", &[[0u8; 4], start.to_be_bytes()].concat()), boxed(b"trun", &trun)].concat()));
        let plain = [0u32.to_be_bytes(), 2u32.to_be_bytes()].concat();
        let own = [0x900u32.to_be_bytes(), 1u32.to_be_bytes(), 256u32.to_be_bytes(), 1024u32.to_be_bytes()].concat();
        let file = [boxed(b"ftyp", b"iso6"), frag(0, plain), boxed(b"mdat", &[0; 16]), frag(1024, own), boxed(b"mdat", &[0; 8])].concat();
        let path = std::env::temp_dir().join(format!("raea-fragments-{}.mp4", std::process::id()));
        std::fs::write(&path, file).unwrap();
        let got = super::samples(&path, &[], 1);
        let _ = std::fs::remove_file(&path);
        assert_eq!(got, Some((vec![512, 512, 256], vec![0, 512, 2048])));
    }
}
