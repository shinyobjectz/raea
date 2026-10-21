//! Packet-level audio decoding (Symphonia), for coders handed compressed packets one at a time: AAC (with its
//! AudioSpecificConfig), MP3, FLAC, PCM, as Symphonia reads them. Planar f32 out.

use symphonia::core::audio::{AudioBufferRef, Signal};
use symphonia::core::codecs::{CodecParameters, Decoder, DecoderOptions};
use symphonia::core::formats::Packet;

use crate::R;

pub struct AudioPacketDecoder {
    dec: Box<dyn Decoder>,
    ts: u64,
}

/// The codec type a WebCodecs-style codec string names.
fn codec_type(codec: &str) -> R<symphonia::core::codecs::CodecType> {
    use symphonia::core::codecs::*;
    Ok(match codec.split('.').next().unwrap_or(codec) {
        "mp4a" | "aac" => CODEC_TYPE_AAC,
        "mp3" => CODEC_TYPE_MP3,
        "flac" => CODEC_TYPE_FLAC,
        o => return Err(format!("{o}: audio decodes here as aac, mp3 or flac")),
    })
}

impl AudioPacketDecoder {
    pub fn new(codec: &str, description: Option<&[u8]>, rate: u32, channels: u32) -> R<AudioPacketDecoder> {
        let mut p = CodecParameters::new();
        p.for_codec(codec_type(codec)?).with_sample_rate(rate);
        if let Some(d) = description.filter(|d| !d.is_empty()) {
            p.with_extra_data(d.to_vec().into_boxed_slice());
        }
        if let Some(layout) = match channels {
            1 => Some(symphonia::core::audio::Layout::Mono),
            2 => Some(symphonia::core::audio::Layout::Stereo),
            _ => None,
        } {
            p.with_channel_layout(layout);
        }
        let dec = symphonia::default::get_codecs().make(&p, &DecoderOptions::default()).map_err(|e| format!("symphonia: {e}"))?;
        Ok(AudioPacketDecoder { dec, ts: 0 })
    }

    /// One packet: (rate, planes of f32).
    pub fn decode(&mut self, data: &[u8]) -> R<(u32, Vec<Vec<f32>>)> {
        let pk = Packet::new_from_slice(0, self.ts, 0, data);
        self.ts += 1;
        let buf = match self.dec.decode(&pk) {
            Ok(b) => b,
            Err(symphonia::core::errors::Error::DecodeError(_)) => return Ok((0, Vec::new())),
            Err(e) => return Err(format!("symphonia: {e}")),
        };
        let rate = buf.spec().rate;
        let planes = match buf {
            AudioBufferRef::F32(b) => (0..b.spec().channels.count()).map(|c| b.chan(c).to_vec()).collect(),
            other => {
                let mut f = symphonia::core::audio::AudioBuffer::<f32>::new(other.capacity() as u64, *other.spec());
                other.convert(&mut f);
                (0..f.spec().channels.count()).map(|c| f.chan(c).to_vec()).collect()
            }
        };
        Ok((rate, planes))
    }
}
