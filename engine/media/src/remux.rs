//! Muxing is pure Rust everywhere (muxide): platform encoders' elementary streams are re-muxed here, so one container
//! path serves every backend and platform.

use std::path::Path;

use crate::R;

pub(crate) fn avcc_to_annexb(sample: &[u8], prefix: Option<&[u8]>) -> Vec<u8> {
    let mut out = Vec::with_capacity(sample.len() + 64);
    if let Some(p) = prefix {
        out.extend_from_slice(p);
    }
    let mut i = 0;
    while i + 4 <= sample.len() {
        let n =
            u32::from_be_bytes([sample[i], sample[i + 1], sample[i + 2], sample[i + 3]]) as usize;
        i += 4;
        out.extend_from_slice(&[0, 0, 0, 1]);
        out.extend_from_slice(&sample[i..(i + n).min(sample.len())]);
        i += n;
    }
    out
}

fn avcc_params(avcc: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    let mut i = 5;
    let nsps = (avcc[i] & 0x1f) as usize;
    i += 1;
    for _ in 0..nsps {
        let n = u16::from_be_bytes([avcc[i], avcc[i + 1]]) as usize;
        out.extend_from_slice(&[0, 0, 0, 1]);
        out.extend_from_slice(&avcc[i + 2..i + 2 + n]);
        i += 2 + n;
    }
    let npps = avcc[i] as usize;
    i += 1;
    for _ in 0..npps {
        let n = u16::from_be_bytes([avcc[i], avcc[i + 1]]) as usize;
        out.extend_from_slice(&[0, 0, 0, 1]);
        out.extend_from_slice(&avcc[i + 2..i + 2 + n]);
        i += 2 + n;
    }
    out
}

/// One raw AAC frame wrapped in an ADTS header (LC profile).
fn adts(frame: &[u8], rate: u32, ch: u8) -> Vec<u8> {
    let rates = [
        96000, 88200, 64000, 48000, 44100, 32000, 24000, 22050, 16000, 12000, 11025, 8000, 7350,
    ];
    let sf = rates.iter().position(|r| *r == rate).unwrap_or(3) as u8;
    let len = frame.len() + 7;
    let mut h = vec![
        0xFF,
        0xF1,
        (1 << 6) | (sf << 2) | (ch >> 2),
        ((ch & 3) << 6) | ((len >> 11) as u8 & 3),
        (len >> 3) as u8,
        (((len & 7) << 5) as u8) | 0x1F,
        0xFC,
    ];
    h.extend_from_slice(frame);
    h
}

/// An H.264 MP4 (video only) and an AAC M4A, into one MP4.
/// The AAC encoder's delay in an .m4a, in samples: its iTunSMPB tag (Apple's encoders) or its edit list's media time,
/// else AudioToolbox's 2112. Carried into our mp4 as an edit list: without one, ffmpeg-based players played the
/// priming as sound and every export's audio came 44 ms after its picture (a click at 1.000 s decoded at 1.044 s).
/// The AAC track's channels, from its AudioSpecificConfig (else its sample entry; else stereo). The mp4 we write
/// must say what the packets hold: written as 1 for stereo packets, ffmpeg played both channels (it reads the packets)
/// but Apple's players, which believe the header, played nothing at all.
fn aac_channels(m: &re_mp4::Mp4, t: &re_mp4::Track) -> u16 {
    match &t.trak(m).mdia.minf.stbl.stsd.contents {
        re_mp4::StsdBoxContent::Mp4a(a) => match a.esds.as_ref().map(|e| e.es_desc.dec_config.dec_specific.chan_conf) {
            Some(c @ 1..=7) => c as u16,
            _ if a.channelcount > 0 => a.channelcount,
            _ => 2,
        },
        _ => 2,
    }
}

fn aac_priming(m4a: &[u8]) -> u32 {
    if let Some(at) = m4a.windows(8).position(|w| w == b"iTunSMPB") {
        let tail = &m4a[at + 8..(at + 208).min(m4a.len())];
        if let Some(st) = tail.windows(9).position(|w| w == b" 00000000") {
            let f: Vec<u32> = String::from_utf8_lossy(&tail[st..]).split_whitespace().take(3).filter_map(|x| u32::from_str_radix(x, 16).ok()).collect();
            if let Some(p) = f.get(1).filter(|p| **p > 0) {
                return *p;
            }
        }
    }
    if let Some(at) = m4a.windows(4).position(|w| w == b"elst") {
        let b = &m4a[at + 4..];
        let n = b.get(4..8).map(|x| u32::from_be_bytes(x.try_into().unwrap())).unwrap_or(0);
        let t = if b.first() == Some(&1) { b.get(16..24).map(|x| i64::from_be_bytes(x.try_into().unwrap())) } else { b.get(12..16).map(|x| i32::from_be_bytes(x.try_into().unwrap()) as i64) };
        if let Some(t) = t.filter(|t| n >= 1 && *t > 0) {
            return t as u32;
        }
    }
    2112
}

pub fn h264_aac(video: &Path, aac: &Path, out: &Path, fps: f64) -> R<()> {
    let vb = std::fs::read(video).map_err(|e| e.to_string())?;
    let ab = std::fs::read(aac).map_err(|e| e.to_string())?;
    let vm = re_mp4::Mp4::read_bytes(&vb).map_err(|e| format!("{}: {e}", video.display()))?;
    let am = re_mp4::Mp4::read_bytes(&ab).map_err(|e| format!("{}: {e}", aac.display()))?;
    let (_, vt) = vm
        .tracks()
        .iter()
        .find(|(_, t)| t.kind == Some(re_mp4::TrackKind::Video))
        .ok_or("no video")?;
    let (_, at) = am
        .tracks()
        .iter()
        .find(|(_, t)| t.kind == Some(re_mp4::TrackKind::Audio))
        .ok_or("no audio")?;
    let params = avcc_params(&vt.raw_codec_config(&vm).ok_or("no avcC")?);
    let rate = at.timescale as u32;
    let file = std::fs::File::create(out).map_err(|e| e.to_string())?;
    let mut m = muxide::api::MuxerBuilder::new(std::io::BufWriter::new(file))
        .video(
            muxide::api::VideoCodec::H264,
            vt.width as u32,
            vt.height as u32,
            fps,
        )
        .audio(
            muxide::api::AudioCodec::Aac(muxide::api::AacProfile::Lc),
            rate,
            aac_channels(&am, at),
        )
        .audio_priming(aac_priming(&ab))
        .with_fast_start(true)
        .build()
        .map_err(|e| format!("muxide: {e}"))?;
    let mut audio: Vec<(f64, &[u8])> = at
        .samples
        .iter()
        .map(|s| {
            (
                s.composition_timestamp as f64 / s.timescale as f64,
                &ab[s.byte_range()],
            )
        })
        .collect();
    audio.sort_by(|a, b| a.0.total_cmp(&b.0));
    let mut ai = 0;
    let mut first = true;
    for s in &vt.samples {
        let pts = s.composition_timestamp as f64 / s.timescale as f64;
        while ai < audio.len() && audio[ai].0 < pts {
            m.write_audio(audio[ai].0, &adts(audio[ai].1, rate, 1))
                .map_err(|e| format!("muxide audio: {e}"))?;
            ai += 1;
        }
        let data = avcc_to_annexb(
            &vb[s.byte_range()],
            if first || s.is_sync {
                Some(&params)
            } else {
                None
            },
        );
        first = false;
        m.write_video(pts, &data, s.is_sync)
            .map_err(|e| format!("muxide video: {e}"))?;
    }
    let end = vt.samples.len() as f64 / fps;
    for (pts, d) in audio[ai..].iter().filter(|x| x.0 < end) {
        m.write_audio(*pts, &adts(d, rate, 1))
            .map_err(|e| format!("muxide audio: {e}"))?;
    }
    m.finish().map_err(|e| format!("muxide: {e}"))?;
    Ok(())
}

/// Annex B H.264 frames (display order = decode order: no B-frames; keyframes carry SPS/PPS) and, optionally, the AAC
/// track of an M4A, into one MP4.
pub fn annexb_aac(frames: &[(Vec<u8>, bool)], w: u32, h: u32, fps: f64, aac: Option<&Path>, out: &Path) -> R<()> {
    annexb_mux(muxide::api::VideoCodec::H264, frames, w, h, fps, aac, out)
}

/// As annexb_aac, for `codec`'s Annex B frames (H.265: keyframes carry VPS, SPS and PPS).
pub fn annexb_mux(codec: muxide::api::VideoCodec, frames: &[(Vec<u8>, bool)], w: u32, h: u32, fps: f64, aac: Option<&Path>, out: &Path) -> R<()> {
    let ab = match aac {
        Some(p) => std::fs::read(p).map_err(|e| e.to_string())?,
        None => Vec::new(),
    };
    let am = if aac.is_some() { Some(re_mp4::Mp4::read_bytes(&ab).map_err(|e| format!("aac: {e}"))?) } else { None };
    let at = match &am {
        Some(m) => Some(m.tracks().iter().find(|(_, t)| t.kind == Some(re_mp4::TrackKind::Audio)).map(|(_, t)| t).ok_or("no audio")?),
        None => None,
    };
    let file = std::fs::File::create(out).map_err(|e| e.to_string())?;
    let mut b = muxide::api::MuxerBuilder::new(std::io::BufWriter::new(file)).video(codec, w, h, fps).with_fast_start(true);
    let rate = at.map(|t| t.timescale as u32).unwrap_or(48000);
    if at.is_some() {
        b = b.audio(muxide::api::AudioCodec::Aac(muxide::api::AacProfile::Lc), rate, aac_channels(am.as_ref().unwrap(), at.unwrap())).audio_priming(aac_priming(&ab));
    }
    let mut m = b.build().map_err(|e| format!("muxide: {e}"))?;
    let mut audio: Vec<(f64, &[u8])> = at
        .map(|t| t.samples.iter().map(|s| (s.composition_timestamp as f64 / s.timescale as f64, &ab[s.byte_range()])).collect())
        .unwrap_or_default();
    audio.sort_by(|a, b| a.0.total_cmp(&b.0));
    let mut ai = 0;
    for (k, (data, key)) in frames.iter().enumerate() {
        let pts = k as f64 / fps;
        while ai < audio.len() && audio[ai].0 < pts {
            m.write_audio(audio[ai].0, &adts(audio[ai].1, rate, 1)).map_err(|e| format!("muxide audio: {e}"))?;
            ai += 1;
        }
        m.write_video(pts, data, *key).map_err(|e| format!("muxide video: {e}"))?;
    }
    let end = frames.len() as f64 / fps;
    for (pts, d) in audio[ai..].iter().filter(|x| x.0 < end) {
        m.write_audio(*pts, &adts(d, rate, 1)).map_err(|e| format!("muxide audio: {e}"))?;
    }
    m.finish().map_err(|e| format!("muxide: {e}"))?;
    Ok(())
}
