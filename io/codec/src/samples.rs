//! A video encoder's samples kept on disk as they come out, not in memory, and read back one at a time for the mux
//! (remux::annexb_mux_from). A 7-minute film's 1 GB of H.264 grew its render by that much and was copied again
//! into the mux (docs/stage-guards.feature SG4a). The file is a temp file gone when raea is (raea_io_home::scratch_file),
//! so a render that dies leaves nothing behind.

use std::io::{Read, Seek, SeekFrom, Write};

type R<T> = Result<T, String>;

/// Each frame's sample: where it is in the file, its length, and whether it's a keyframe.
pub struct Samples {
    file: std::io::BufWriter<std::fs::File>,
    at: Vec<Option<(u64, u32, bool)>>,
    end: u64,
}

impl Samples {
    pub fn new() -> R<Samples> {
        let f = raea_io_home::scratch_file("samples")?;
        Ok(Samples { file: std::io::BufWriter::with_capacity(1 << 20, f), at: Vec::new(), end: 0 })
    }

    /// Frame `k`'s sample, in any order (an encoder hands them back as they finish).
    pub fn put(&mut self, k: usize, data: &[u8], key: bool) -> R<()> {
        self.file.write_all(data).map_err(|e| format!("keeping a sample on disk: {e}"))?;
        if self.at.len() <= k {
            self.at.resize(k + 1, None);
        }
        self.at[k] = Some((self.end, data.len() as u32, key));
        self.end += data.len() as u64;
        Ok(())
    }

    /// Frames up to the last one given.
    pub fn len(&self) -> usize {
        self.at.len()
    }

    pub fn is_empty(&self) -> bool {
        self.at.is_empty()
    }

    /// The first frame below len() with no sample.
    pub fn missing(&self) -> Option<usize> {
        self.at.iter().position(Option::is_none)
    }

    /// Frame k's sample into `buf`, and whether it's a keyframe: annexb_mux_from's reader.
    pub fn reader(self) -> R<impl FnMut(usize, &mut Vec<u8>) -> R<bool>> {
        let at = self.at;
        let mut f = self.file.into_inner().map_err(|e| format!("the samples on disk: {e}"))?;
        Ok(move |k: usize, buf: &mut Vec<u8>| -> R<bool> {
            let (o, n, key) = at.get(k).copied().flatten().ok_or(format!("frame {k} has no sample"))?;
            buf.resize(n as usize, 0);
            f.seek(SeekFrom::Start(o)).and_then(|_| f.read_exact(buf)).map_err(|e| format!("reading frame {k}'s sample back: {e}"))?;
            Ok(key)
        })
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn samples_come_back_by_frame_whatever_order_they_went_in() {
        let mut s = super::Samples::new().unwrap();
        s.put(1, b"second", false).unwrap();
        s.put(0, b"first!", true).unwrap();
        s.put(2, b"", false).unwrap();
        assert_eq!((s.len(), s.missing()), (3, None));
        let mut read = s.reader().unwrap();
        let mut buf = Vec::new();
        assert!(read(0, &mut buf).unwrap() && buf == b"first!");
        assert!(!read(1, &mut buf).unwrap() && buf == b"second");
        assert!(!read(2, &mut buf).unwrap() && buf.is_empty());
        assert!(read(3, &mut buf).is_err());
        let mut gap = super::Samples::new().unwrap();
        gap.put(2, b"x", true).unwrap();
        assert_eq!(gap.missing(), Some(0));
    }
}
