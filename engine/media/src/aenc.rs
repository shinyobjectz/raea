//! Packet-level AAC encoding on Apple (AudioToolbox through AVAudioConverter), for coders that hand PCM in and want
//! compressed packets out one at a time (mediabunny's CustomAudioEncoder): AAC-LC at the input's rate and channels,
//! 1024 frames a packet, the AudioSpecificConfig as the decoder description. The encoder's priming (2112 frames, as
//! AudioToolbox and the MP4 convention have it) is reported so a muxer can mark it to skip.

use std::ptr::NonNull;

use objc2::rc::Retained;
use objc2::AnyThread;
use objc2_avf_audio::{
    AVAudioBuffer, AVAudioCommonFormat, AVAudioCompressedBuffer, AVAudioConverter, AVAudioConverterInputStatus,
    AVAudioConverterOutputStatus, AVAudioFormat, AVAudioPCMBuffer, AVEncoderBitRateKey, AVFormatIDKey,
    AVNumberOfChannelsKey, AVSampleRateKey,
};
use objc2_foundation::{NSDictionary, NSNumber, NSString};

use crate::R;

/// AAC-LC's encoder delay in AudioToolbox (frames before the first sample).
pub const AAC_PRIMING: u32 = 2112;

pub struct AacPacket {
    pub data: Vec<u8>,
    /// seconds, from the first input sample (the priming makes the first packets earlier than 0)
    pub pts: f64,
    pub dur: f64,
}

pub struct AacEncoder {
    conv: Retained<AVAudioConverter>,
    pcm: Retained<AVAudioFormat>,
    aac: Retained<AVAudioFormat>,
    rate: f64,
    channels: u32,
    /// packets out so far
    n: u64,
    max_packet: usize,
}

impl AacEncoder {
    pub fn new(rate: u32, channels: u32, kbps: u32) -> R<AacEncoder> {
        if !(1..=2).contains(&channels) {
            return Err(format!("AAC: 1 or 2 channels (got {channels})"));
        }
        unsafe {
            let pcm = AVAudioFormat::initWithCommonFormat_sampleRate_channels_interleaved(AVAudioFormat::alloc(), AVAudioCommonFormat::PCMFormatFloat32, rate as f64, channels, false).ok_or("AVAudioFormat (PCM)")?;
            let num = |x: f64| NSNumber::new_f64(x);
            let settings: Retained<NSDictionary<NSString, objc2::runtime::AnyObject>> = NSDictionary::from_slices(
                &[AVFormatIDKey.ok_or("key")?, AVSampleRateKey.ok_or("key")?, AVNumberOfChannelsKey.ok_or("key")?],
                &[&*NSNumber::new_u32(u32::from_be_bytes(*b"aac ")) as &objc2::runtime::AnyObject, &*num(rate as f64) as &objc2::runtime::AnyObject, &*num(channels as f64) as &objc2::runtime::AnyObject],
            );
            let aac = AVAudioFormat::initWithSettings(AVAudioFormat::alloc(), &settings).ok_or("AVAudioFormat (AAC)")?;
            let conv = AVAudioConverter::initFromFormat_toFormat(AVAudioConverter::alloc(), &pcm, &aac).ok_or("AVAudioConverter")?;
            conv.setBitRate((kbps.max(16) * 1000) as isize);
            let _ = AVEncoderBitRateKey;
            let max_packet = (conv.maximumOutputPacketSize() as usize).max(1536 * channels as usize);
            Ok(AacEncoder { conv, pcm, aac, rate: rate as f64, channels, n: 0, max_packet })
        }
    }

    /// The decoder description: AudioSpecificConfig for AAC-LC at this rate and channel count.
    pub fn description(&self) -> Vec<u8> {
        const RATES: [u32; 13] = [96000, 88200, 64000, 48000, 44100, 32000, 24000, 22050, 16000, 12000, 11025, 8000, 7350];
        let fi = RATES.iter().position(|r| *r == self.rate as u32).unwrap_or(15) as u16;
        if fi == 15 {
            // an explicit 24-bit rate after the escape index
            let r = self.rate as u32;
            let bits: u64 = (2u64 << 35) | (15u64 << 31) | ((r as u64) << 7) | ((self.channels as u64) << 3);
            return vec![(bits >> 32) as u8, (bits >> 24) as u8, (bits >> 16) as u8, (bits >> 8) as u8, bits as u8];
        }
        let v: u16 = (2 << 11) | (fi << 7) | ((self.channels as u16) << 3);
        vec![(v >> 8) as u8, v as u8]
    }

    fn pull(&mut self, input: Option<Retained<AVAudioPCMBuffer>>, end: bool) -> R<Vec<AacPacket>> {
        let mut out = Vec::new();
        let given = std::cell::Cell::new(false);
        let input_ref = &input;
        let block = block2::RcBlock::new(|_n: u32, status: NonNull<AVAudioConverterInputStatus>| -> *mut AVAudioBuffer {
            unsafe {
                if let (Some(b), false) = (input_ref, given.get()) {
                    given.set(true);
                    *status.as_ptr() = AVAudioConverterInputStatus::HaveData;
                    return Retained::as_ptr(b) as *const AVAudioBuffer as *mut AVAudioBuffer;
                }
                *status.as_ptr() = if end { AVAudioConverterInputStatus::EndOfStream } else { AVAudioConverterInputStatus::NoDataNow };
                std::ptr::null_mut()
            }
        });
        loop {
            let buf = unsafe { AVAudioCompressedBuffer::initWithFormat_packetCapacity_maximumPacketSize(AVAudioCompressedBuffer::alloc(), &self.aac, 16, self.max_packet as isize) };
            let mut err = None;
            let st = unsafe { self.conv.convertToBuffer_error_withInputFromBlock(&buf, Some(&mut err), &*block as *const _ as *mut _) };
            if st == AVAudioConverterOutputStatus::Error {
                return Err(format!("AAC encode: {}", err.map(|e| e.localizedDescription().to_string()).unwrap_or_default()));
            }
            unsafe {
                let n = buf.packetCount() as usize;
                let descs = buf.packetDescriptions();
                let data = buf.data().as_ptr() as *const u8;
                for k in 0..n {
                    let d = &*descs.add(k);
                    let bytes = std::slice::from_raw_parts(data.add(d.mStartOffset as usize), d.mDataByteSize as usize).to_vec();
                    let pts = (self.n as f64 * 1024.0 - AAC_PRIMING as f64) / self.rate;
                    out.push(AacPacket { data: bytes, pts, dur: 1024.0 / self.rate });
                    self.n += 1;
                }
                if n == 0 || st != AVAudioConverterOutputStatus::HaveData {
                    break;
                }
            }
        }
        Ok(out)
    }

    /// Planar f32 samples in (one slice per channel, the same length); the packets they complete out.
    pub fn encode(&mut self, planes: &[&[f32]]) -> R<Vec<AacPacket>> {
        let frames = planes.first().map(|p| p.len()).unwrap_or(0);
        if frames == 0 {
            return Ok(Vec::new());
        }
        let buf = unsafe { AVAudioPCMBuffer::initWithPCMFormat_frameCapacity(AVAudioPCMBuffer::alloc(), &self.pcm, frames as u32).ok_or("AVAudioPCMBuffer")? };
        unsafe {
            buf.setFrameLength(frames as u32);
            let data = buf.floatChannelData();
            for c in 0..self.channels as usize {
                let src = planes.get(c).or(planes.first()).unwrap();
                std::slice::from_raw_parts_mut((*data.add(c)).as_ptr(), frames).copy_from_slice(&src[..frames]);
            }
        }
        self.pull(Some(buf), false)
    }

    /// What the encoder still holds.
    pub fn finish(&mut self) -> R<Vec<AacPacket>> {
        self.pull(None, true)
    }
}
