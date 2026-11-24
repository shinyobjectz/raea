//! What a file is, cheaply: its size and a hash of its first and last MB. The ledger keeps it on every call
//! (apps/ask record.rs), so a render is known again by its copy in the feed, a later render to the same path isn't
//! taken for it, and `raea make status` knows an answer was about exactly the sheet on disk now (docs/make.feature MK4).

use std::io::{Read as _, Seek as _, SeekFrom};
use std::path::Path;

use sha2::{Digest, Sha256};

pub fn ident(path: &Path) -> Option<String> {
    const MB: u64 = 1 << 20;
    let mut f = std::fs::File::open(path).ok()?;
    let len = f.metadata().ok()?.len();
    let mut h = Sha256::new();
    let mut part = vec![0u8; len.min(MB) as usize];
    f.read_exact(&mut part).ok()?;
    h.update(&part);
    if len > 2 * MB {
        f.seek(SeekFrom::Start(len - MB)).ok()?;
        f.read_exact(&mut part).ok()?;
        h.update(&part);
    }
    let hex: String = h.finalize().iter().take(16).map(|b| format!("{b:02x}")).collect();
    Some(format!("{len}-{hex}"))
}

/// The size an ident names, to pass over files that can't be it without reading them.
pub fn ident_len(ident: &str) -> Option<u64> {
    ident.split_once('-')?.0.parse().ok()
}
