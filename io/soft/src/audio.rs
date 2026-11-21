//! Sound through Symphonia (decode, a window of it, its length), the MP4 boxes that say how much of it to skip, and
//! Opus packets for muxing.

use super::*;

/// The rate a track's sound decodes at. Symphonia's AAC decoder reads HE-AAC's core alone (its spectral band
/// replication, the top octave, it doesn't), at half the rate the track states: an Instagram or Facebook reel's
/// 44.1 kHz sound comes out at 22.05 kHz, and read at the stated rate it lasted half as long (docs/videoform.feature
/// VF1). The AudioSpecificConfig's object type says so: 5 (SBR) or 29 (parametric stereo).
fn decoded_rate(p: &symphonia::core::codecs::CodecParameters) -> u32 {
    let rate = p.sample_rate.unwrap_or(0);
    let aot = p.extra_data.as_deref().and_then(|b| {
        let t = b.first()? >> 3;
        if t == 31 { Some(32 + (((b[0] & 7) << 3) | (b.get(1)? >> 5))) } else { Some(t) }
    });
    if p.codec == symphonia::core::codecs::CODEC_TYPE_AAC && matches!(aot, Some(5 | 29)) { rate / 2 } else { rate }
}

/// A count of the track's samples (priming, a stated length) in the decoded sound's
fn decoded(n: usize, stated: u32, rate: u32) -> usize {
    if stated == 0 || stated == rate { n } else { (n as u64 * rate as u64 / stated as u64) as usize }
}

pub fn decode_audio_symphonia(path: &Path) -> R<Audio> {
    use symphonia::core::audio::{AudioBufferRef, Signal};
    use symphonia::core::codecs::DecoderOptions;
    use symphonia::core::formats::FormatOptions;
    use symphonia::core::io::MediaSourceStream;
    use symphonia::core::meta::MetadataOptions;
    use symphonia::core::probe::Hint;
    let file = std::fs::File::open(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let mss = MediaSourceStream::new(Box::new(file), Default::default());
    let mut hint = Hint::new();
    if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
        hint.with_extension(ext);
    }
    let probed = symphonia::default::get_probe()
        .format(
            &hint,
            mss,
            &FormatOptions {
                enable_gapless: true,
                ..Default::default()
            },
            &MetadataOptions::default(),
        )
        .map_err(|e| format!("symphonia: {e}"))?;
    let mut format = probed.format;
    // the first track with a sample rate is the sound (a .mov's default track is often the picture)
    let track = format
        .tracks()
        .iter()
        .find(|t| t.codec_params.sample_rate.is_some())
        .ok_or("no audio track")?
        .clone();
    let mut dec = symphonia::default::get_codecs()
        .make(&track.codec_params, &DecoderOptions::default())
        .map_err(|e| format!("symphonia: {e}"))?;
    let stated = track.codec_params.sample_rate.unwrap_or(0);
    let mut rate = decoded_rate(&track.codec_params);
    let mut cols: Vec<Vec<f32>> = Vec::new();
    while let Ok(packet) = format.next_packet() {
        if packet.track_id() != track.id {
            continue;
        }
        let buf = match dec.decode(&packet) {
            Ok(b) => b,
            Err(symphonia::core::errors::Error::DecodeError(_)) => continue,
            Err(e) => return Err(format!("symphonia: {e}")),
        };
        // the track can say 0 (Symphonia's MP4 reader on a 96 kHz AAC track, void's colitt take09): the decoded
        // buffer knows its rate
        // and it's the decoder's own word on the rate it gives
        if buf.spec().rate > 0 {
            rate = buf.spec().rate;
        }
        let mut push = |planes: &[&[f32]]| {
            if cols.is_empty() {
                cols = vec![Vec::new(); planes.len()];
            }
            for (c, p) in cols.iter_mut().zip(planes) {
                c.extend_from_slice(p);
            }
        };
        match buf {
            AudioBufferRef::F32(b) => {
                let planes: Vec<&[f32]> =
                    (0..b.spec().channels.count()).map(|c| b.chan(c)).collect();
                push(&planes);
            }
            other => {
                let mut f = symphonia::core::audio::AudioBuffer::<f32>::new(
                    other.capacity() as u64,
                    *other.spec(),
                );
                other.convert(&mut f);
                let planes: Vec<&[f32]> =
                    (0..f.spec().channels.count()).map(|c| f.chan(c)).collect();
                push(&planes);
            }
        }
    }
    // MP4 priming: the audio track's edit list says where the sound starts (AAC encoders put 1024-2112 silent
    // samples first); Symphonia 0.5 doesn't apply it, ffmpeg and AVFoundation do
    // (Apple's encoders write it as an iTunSMPB tag instead: priming, padding, and the true length)
    // (both in the track's samples, so in the decoded sound's when HE-AAC decodes at half)
    if let Some((skip, len)) = itunsmpb(path) {
        let (skip, len) = (decoded(skip, stated, rate), decoded(len, stated, rate));
        for c in &mut cols {
            c.drain(..skip.min(c.len()));
            c.truncate(len);
        }
    } else if let Some(skip) = mp4_audio_skip(path) {
        let skip = decoded(skip, stated, rate);
        for c in &mut cols {
            c.drain(..skip.min(c.len()));
        }
    }
    if rate == 0 {
        return Err(format!("{}: the sound track has no sample rate", path.display()));
    }
    Ok(Audio {
        rate,
        channels: cols,
    })
}

/// `secs` seconds of a file's sound from `start` (seconds, in the same time as decode_audio_symphonia's: priming
/// already taken off): a seek and a short decode, for a player that streams a long take instead of decoding it whole
/// (a ten-minute take decoded whole cost a 1.9 s stall and 1.2 GB in live play).
pub fn decode_audio_window_symphonia(path: &Path, start: f64, secs: f64) -> R<Audio> {
    use symphonia::core::audio::{AudioBufferRef, Signal};
    use symphonia::core::codecs::DecoderOptions;
    use symphonia::core::formats::{FormatOptions, SeekMode, SeekTo};
    use symphonia::core::io::MediaSourceStream;
    use symphonia::core::meta::MetadataOptions;
    use symphonia::core::probe::Hint;
    use symphonia::core::units::Time;
    static SKIPS: std::sync::Mutex<Option<std::collections::HashMap<std::path::PathBuf, usize>>> = std::sync::Mutex::new(None);
    let skip = {
        let mut g = SKIPS.lock().unwrap();
        *g.get_or_insert_with(Default::default).entry(path.to_path_buf()).or_insert_with(|| itunsmpb(path).map(|(s, _)| s).or_else(|| mp4_audio_skip(path)).unwrap_or(0))
    };
    let file = std::fs::File::open(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let mss = MediaSourceStream::new(Box::new(file), Default::default());
    let mut hint = Hint::new();
    if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
        hint.with_extension(ext);
    }
    let probed = symphonia::default::get_probe()
        .format(&hint, mss, &FormatOptions { enable_gapless: true, ..Default::default() }, &MetadataOptions::default())
        .map_err(|e| format!("symphonia: {e}"))?;
    let mut format = probed.format;
    let track = format.tracks().iter().find(|t| t.codec_params.sample_rate.is_some()).ok_or("no audio track")?.clone();
    let mut dec = symphonia::default::get_codecs().make(&track.codec_params, &DecoderOptions::default()).map_err(|e| format!("symphonia: {e}"))?;
    let (stated, rate) = (track.codec_params.sample_rate.unwrap_or(0), decoded_rate(&track.codec_params));
    if rate == 0 {
        return Err(format!("{}: the sound track has no sample rate", path.display()));
    }
    // a packet's timestamp is in the track's samples; the window is counted in the decoded sound's
    let ts_of = |t: u64| decoded(t as usize, stated, rate) as u64;
    // samples in the decoded sound's count (priming included), where the window starts and how many it holds
    let first = (start.max(0.0) * rate as f64).round() as u64 + decoded(skip, stated, rate) as u64;
    let want = (secs * rate as f64).round() as usize;
    // seek a little before: AAC's first packet after a seek decodes without its overlap, so the decoder warms up on
    // packets that are then thrown away
    let t = first.saturating_sub(4096) as f64 / rate as f64;
    let seeked = format.seek(SeekMode::Accurate, SeekTo::Time { time: Time::new(t as u64, t.fract()), track_id: Some(track.id) });
    let mut at = seeked.map(|s| ts_of(s.actual_ts)).unwrap_or(0);
    dec.reset();
    let mut cols: Vec<Vec<f32>> = Vec::new();
    let mut have = 0usize;
    while have < want {
        let Ok(packet) = format.next_packet() else { break };
        if packet.track_id() != track.id {
            continue;
        }
        let buf = match dec.decode(&packet) {
            Ok(b) => b,
            Err(symphonia::core::errors::Error::DecodeError(_)) => continue,
            Err(e) => return Err(format!("symphonia: {e}")),
        };
        let mut f = symphonia::core::audio::AudioBuffer::<f32>::new(buf.capacity() as u64, *buf.spec());
        match buf {
            AudioBufferRef::F32(b) => b.convert(&mut f),
            other => other.convert(&mut f),
        }
        let n = f.frames();
        let ts = ts_of(packet.ts()).max(at);
        // the part of this packet inside the window
        let from = first.saturating_sub(ts) as usize;
        if from < n {
            if cols.is_empty() {
                cols = vec![Vec::new(); f.spec().channels.count()];
            }
            let take = (n - from).min(want - have);
            for (c, col) in cols.iter_mut().enumerate() {
                col.extend_from_slice(&f.chan(c)[from..from + take]);
            }
            have += take;
        }
        at = ts + n as u64;
    }
    Ok(Audio { rate, channels: cols })
}

/// How long a file's sound is, from its container (frames over rate), without decoding it.
pub fn probe_seconds_symphonia(path: &Path) -> Option<f64> {
    use symphonia::core::formats::FormatOptions;
    use symphonia::core::io::MediaSourceStream;
    use symphonia::core::meta::MetadataOptions;
    use symphonia::core::probe::Hint;
    let file = std::fs::File::open(path).ok()?;
    let mss = MediaSourceStream::new(Box::new(file), Default::default());
    let mut hint = Hint::new();
    if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
        hint.with_extension(ext);
    }
    let probed = symphonia::default::get_probe().format(&hint, mss, &FormatOptions::default(), &MetadataOptions::default()).ok()?;
    let t = probed.format.tracks().iter().find(|t| t.codec_params.sample_rate.is_some())?.codec_params.clone();
    Some(t.n_frames? as f64 / t.sample_rate? as f64)
}

/// An MP4's `moov` box (header included), read by walking the top-level boxes with seeks: the tags and edit lists
/// live there, and reading the whole file for them (a ten-minute take is over a gigabyte) stalled a player for seconds.
pub(crate) fn moov(path: &Path) -> Option<Vec<u8>> {
    use std::io::{Read, Seek, SeekFrom};
    let mut f = std::fs::File::open(path).ok()?;
    let len = f.metadata().ok()?.len();
    let mut at = 0u64;
    while at + 8 <= len {
        f.seek(SeekFrom::Start(at)).ok()?;
        let mut h = [0u8; 16];
        f.read_exact(&mut h[..8]).ok()?;
        let mut size = u32::from_be_bytes(h[..4].try_into().unwrap()) as u64;
        if size == 1 {
            f.read_exact(&mut h[8..16]).ok()?;
            size = u64::from_be_bytes(h[8..16].try_into().unwrap());
        } else if size == 0 {
            size = len - at;
        }
        if size < 8 {
            return None;
        }
        if &h[4..8] == b"moov" {
            let mut b = vec![0u8; size.min(64 << 20) as usize];
            f.seek(SeekFrom::Start(at)).ok()?;
            f.read_exact(&mut b).ok()?;
            return Some(b);
        }
        at += size;
    }
    None
}

/// iTunes gapless info: " 00000000 PRIMING PADDING LENGTH ..." (hex), stored as text after the tag name.
pub(crate) fn itunsmpb(path: &Path) -> Option<(usize, usize)> {
    let b = moov(path)?;
    let at = b.windows(8).position(|w| w == b"iTunSMPB")?;
    let tail = &b[at + 8..(at + 8 + 200).min(b.len())];
    let start = tail.windows(9).position(|w| w == b" 00000000")?;
    let text: String = tail[start..]
        .iter()
        .take_while(|c| c.is_ascii_hexdigit() || **c == b' ')
        .map(|c| *c as char)
        .collect();
    let f: Vec<u64> = text
        .split_whitespace()
        .filter_map(|x| u64::from_str_radix(x, 16).ok())
        .collect();
    (f.len() >= 4 && f[3] > 0).then(|| (f[1] as usize, f[3] as usize))
}

/// The first sound track's edit-list start (`elst` media_time, in its own sample rate's units), from the MP4 boxes.
pub(crate) fn mp4_audio_skip(path: &Path) -> Option<usize> {
    let b = moov(path)?;
    fn boxes(b: &[u8]) -> Vec<(&[u8], &[u8])> {
        let mut out = Vec::new();
        let mut i = 0;
        while i + 8 <= b.len() {
            let mut size = u32::from_be_bytes(b[i..i + 4].try_into().unwrap()) as usize;
            let kind = &b[i + 4..i + 8];
            let mut head = 8;
            if size == 1 && i + 16 <= b.len() {
                size = u64::from_be_bytes(b[i + 8..i + 16].try_into().unwrap()) as usize;
                head = 16;
            } else if size == 0 {
                size = b.len() - i;
            }
            if size < head || i + size > b.len() {
                break;
            }
            out.push((kind, &b[i + head..i + size]));
            i += size;
        }
        out
    }
    let moov = boxes(&b).into_iter().find(|(k, _)| *k == b"moov")?.1;
    for (k, trak) in boxes(moov) {
        if k != b"trak" {
            continue;
        }
        let kids = boxes(trak);
        let mdia = kids.iter().find(|(k, _)| *k == b"mdia")?.1;
        let hdlr = boxes(mdia).into_iter().find(|(k, _)| *k == b"hdlr")?.1;
        if hdlr.get(8..12) != Some(b"soun") {
            continue;
        }
        let edts = kids.iter().find(|(k, _)| *k == b"edts")?.1;
        let elst = boxes(edts).into_iter().find(|(k, _)| *k == b"elst")?.1;
        let version = elst[0];
        let count = u32::from_be_bytes(elst[4..8].try_into().ok()?);
        if count == 0 {
            return None;
        }
        // entry: segment duration, media time (signed), rate; an empty edit (media_time -1) first means a delay instead
        let media_time = if version == 1 {
            i64::from_be_bytes(elst[16..24].try_into().ok()?)
        } else {
            i32::from_be_bytes(elst[12..16].try_into().ok()?) as i64
        };
        return (media_time > 0).then_some(media_time as usize);
    }
    None
}

pub use raea_io_codec::soft_resample as resample;

/// Opus packets (20 ms) for the audio columns at 48 kHz: (pts seconds, packet).
pub fn opus_packets(a: &Audio) -> R<Vec<(f64, Vec<u8>)>> {
    let a = resample(a, 48000);
    let ch = a.channels.len().clamp(1, 2);
    let mut enc = opus::Encoder::new(
        48000,
        if ch == 2 {
            opus::Channels::Stereo
        } else {
            opus::Channels::Mono
        },
        opus::Application::Audio,
    )
    .map_err(|e| format!("opus: {e}"))?;
    enc.set_bitrate(opus::Bitrate::Bits(128_000))
        .map_err(|e| format!("opus: {e}"))?;
    let frame = 960;
    let mut out = Vec::new();
    let mut buf = vec![0u8; 4000];
    let mut i = 0;
    while i < a.len() {
        let mut pcm = Vec::with_capacity(frame * ch);
        for k in 0..frame {
            for c in 0..ch {
                pcm.push(a.channels[c].get(i + k).copied().unwrap_or(0.0));
            }
        }
        let n = enc
            .encode_float(&pcm, &mut buf)
            .map_err(|e| format!("opus: {e}"))?;
        out.push((i as f64 / 48000.0, buf[..n].to_vec()));
        i += frame;
    }
    Ok(out)
}


#[cfg(test)]
mod window_tests {
    /// A window of a file's sound equals that span of the whole decode (RAEA_TEST_AUDIO=FILE; skipped without it).
    #[test]
    fn a_window_matches_the_whole_decode() {
        let Some(f) = std::env::var_os("RAEA_TEST_AUDIO") else { return };
        let p = std::path::Path::new(&f);
        let whole = super::decode_audio_symphonia(p).unwrap();
        for start in [0.0, 10.0, 37.25] {
            let w = super::decode_audio_window_symphonia(p, start, 2.0).unwrap();
            let s0 = (start * whole.rate as f64).round() as usize;
            let n = w.channels[0].len().min(whole.channels[0].len() - s0);
            assert!(n > (whole.rate as usize) * 19 / 10, "window at {start} holds {n} samples");
            let err = (0..n).map(|i| (w.channels[0][i] - whole.channels[0][s0 + i]).abs()).fold(0.0f32, f32::max);
            assert!(err < 1e-4, "window at {start}: max difference {err}");
        }
    }
}


#[cfg(test)]
mod he_aac {
    use super::{decoded, decoded_rate};
    use symphonia::core::codecs::{CODEC_TYPE_AAC, CodecParameters};

    #[test]
    fn he_aac_decodes_at_half_the_rate_its_track_states() {
        let aac = |config: &[u8]| {
            let mut p = CodecParameters::new();
            p.for_codec(CODEC_TYPE_AAC).with_sample_rate(44100).with_extra_data(config.into());
            p
        };
        // an Instagram reel's: object type 5 (SBR)
        assert_eq!(decoded_rate(&aac(&[0x2b, 0x92, 0x08, 0x00])), 22050);
        // plain AAC (object type 2) decodes at its own
        assert_eq!(decoded_rate(&aac(&[0x12, 0x10])), 44100);
        // the track's 2112 samples of priming are 1056 of the decoded sound's
        assert_eq!(decoded(2112, 44100, 22050), 1056);
    }
}
