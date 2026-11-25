//! An MP4's or QuickTime file's boxes read directly (docs/media.md): a box's children, its big-endian fields, and a
//! fragmented MP4's frame timing (fragments.rs), for `raea media probe` and the streams raea reads as it holds them.

pub mod fragments;

/// The boxes inside `b` (a box's payload): (type, payload).
pub fn boxes(b: &[u8]) -> Vec<([u8; 4], &[u8])> {
    let mut out = Vec::new();
    let mut i = 0usize;
    while i + 8 <= b.len() {
        let mut size = u32::from_be_bytes(b[i..i + 4].try_into().unwrap()) as usize;
        let kind: [u8; 4] = b[i + 4..i + 8].try_into().unwrap();
        let mut hdr = 8;
        if size == 1 && i + 16 <= b.len() {
            size = u64::from_be_bytes(b[i + 8..i + 16].try_into().unwrap()) as usize;
            hdr = 16;
        } else if size == 0 {
            size = b.len() - i;
        }
        if size < hdr || i + size > b.len() {
            break;
        }
        out.push((kind, &b[i + hdr..i + size]));
        i += size;
    }
    out
}

pub fn child<'a>(b: &'a [u8], kind: &[u8; 4]) -> Option<&'a [u8]> {
    boxes(b).into_iter().find(|(k, _)| k == kind).map(|(_, p)| p)
}

pub fn be32(b: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_be_bytes(b.get(at..at + 4)?.try_into().ok()?))
}

pub fn be64(b: &[u8], at: usize) -> Option<u64> {
    Some(u64::from_be_bytes(b.get(at..at + 8)?.try_into().ok()?))
}
