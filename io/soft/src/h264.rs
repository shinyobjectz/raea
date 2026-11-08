//! H.264 through openh264: decoding a mezzanine, encoding, and muxing what it encodes.

use super::*;

#[cfg(feature = "h264")]
pub(crate) struct Decoder {
    pub(crate) track: Track,
    pub(crate) info: Info,
    pub(crate) pts: Vec<f64>,
    pub(crate) params: Vec<u8>,
    pub(crate) dec: Option<H264Decoder>,
    /// the index (decode order = display order) of the last frame decoded
    pub(crate) last: Option<usize>,
    /// RGBA, or NV12 when `nv12`
    pub(crate) out: Vec<u8>,
    pub(crate) nv12: bool,
}

#[cfg(feature = "h264")]
impl Decoder {
    /// The decoded picture at display orientation.
    pub(crate) fn display(&self) -> Vec<u8> {
        let (w, h, r) = (self.track.width as usize, self.track.height as usize, self.track.rotation);
        if self.nv12 { raea_io_codec::yuv::rotate_nv12(&self.out, w, h, r).0 } else { raea_io_codec::yuv::rotate(&self.out, w, h, r).0 }
    }

    /// Switch between RGBA and NV12 output (the next frame decodes afresh).
    pub(crate) fn mode(&mut self, nv12: bool) {
        if self.nv12 != nv12 {
            self.nv12 = nv12;
            self.last = None;
        }
    }

    pub(crate) fn decode_sample(&mut self, i: usize) -> R<bool> {
        let (off, size, _, _) = self.track.samples[i];
        let nal = annexb(&self.track.bytes[off..off + size]);
        let dec = self.dec.as_mut().unwrap();
        match dec.decode(&nal).map_err(|e| format!("openh264: {e}"))? {
            Some(yuv) => {
                let (w, h) = yuv.dimensions();
                let (ys, us, vs) = yuv.strides();
                if self.nv12 {
                    raea_io_codec::yuv::i420_to_nv12(yuv.y(), yuv.u(), yuv.v(), ys, us, vs, w, h, &mut self.out);
                } else {
                    // our BT.709 conversion (openh264's write_rgba8 is BT.601, which shifts colour against every
                    // other decoder on HD sources)
                    raea_io_codec::yuv::i420_to_rgba(yuv.y(), yuv.u(), yuv.v(), ys, us, vs, w, h, &mut self.out);
                }
                Ok(true)
            }
            None => Ok(false),
        }
    }
}

#[cfg(feature = "h264")]
impl VideoDecoder for Decoder {
    fn info(&self) -> &Info {
        &self.info
    }
    fn index_at(&mut self, t: f64) -> Option<usize> {
        (!self.pts.is_empty()).then(|| raea_io_codec::as_of(&self.pts, t))
    }
    fn keyframes(&self) -> Option<Vec<usize>> {
        // samples are in presentation order here (openh264 streams without B-frames)
        Some(self.track.samples.iter().enumerate().filter(|(_, s)| s.2).map(|(i, _)| i).collect())
    }
    fn frame_nv12_at(&mut self, t: f64) -> R<Frame> {
        self.mode(true);
        self.frame_inner(t)
    }
    fn frame_at(&mut self, t: f64) -> R<Frame> {
        self.mode(false);
        self.frame_inner(t)
    }
}

#[cfg(feature = "h264")]
impl Decoder {
    pub(crate) fn frame_inner(&mut self, t: f64) -> R<Frame> {
        let want = as_of(&self.pts, t);
        // forward within the GOP: keep decoding; otherwise restart from the keyframe at or before `want`
        let from = match self.last {
            Some(l) if l <= want && !self.track.samples[l + 1..=want].iter().any(|s| s.2) => l + 1,
            Some(l) if l == want => {
                return Ok(Frame {
                    pts: self.pts[want],
                    width: self.info.width,
                    height: self.info.height,
                    rgba: self.display(),
                });
            }
            _ => {
                let key = (0..=want)
                    .rev()
                    .find(|&i| self.track.samples[i].2)
                    .unwrap_or(0);
                self.dec = Some(H264Decoder::new().map_err(|e| format!("openh264: {e}"))?);
                let params = self.params.clone();
                self.dec
                    .as_mut()
                    .unwrap()
                    .decode(&params)
                    .map_err(|e| format!("openh264: {e}"))?;
                key
            }
        };
        for i in from..=want {
            self.decode_sample(i)?;
            self.last = Some(i);
        }
        Ok(Frame {
            pts: self.pts[want],
            width: self.info.width,
            height: self.info.height,
            rgba: self.display(),
        })
    }
}


#[cfg(feature = "h264")]
pub(crate) struct Encoder {
    pub(crate) enc: H264Encoder,
    pub(crate) opts: EncodeOpts,
    pub(crate) out: PathBuf,
    pub(crate) frames: Vec<(Vec<u8>, bool)>,
}

#[cfg(feature = "h264")]
impl VideoEncoder for Encoder {
    fn push(&mut self, rgba: &[u8]) -> R<()> {
        let (w, h) = (self.opts.width as usize, self.opts.height as usize);
        let (y, u, v) = raea_io_codec::yuv::rgba_to_i420(rgba, w, h);
        self.push_i420(&y, &u, &v)
    }
    fn push_nv12(&mut self, nv12: &[u8], w: usize, h: usize) -> R<()> {
        let (y, u, v) = raea_io_codec::yuv::nv12_to_i420(nv12, w, h);
        self.push_i420(&y, &u, &v)
    }
    fn finish(self: Box<Self>, audio: Option<&Audio>) -> R<EncodeStats> {
        mux_h264(&self.out, &self.opts, &self.frames, audio)
    }
}

#[cfg(feature = "h264")]
impl Encoder {
    pub(crate) fn push_i420(&mut self, y: &[u8], u: &[u8], v: &[u8]) -> R<()> {
        let (w, h) = (self.opts.width as usize, self.opts.height as usize);
        let src = YUVSlices::new((y, u, v), (w, h), (w, w / 2, w / 2));
        let bs = self
            .enc
            .encode(&src)
            .map_err(|e| format!("openh264: {e}"))?;
        let key = matches!(
            bs.frame_type(),
            openh264::encoder::FrameType::IDR | openh264::encoder::FrameType::I
        );
        self.frames.push((bs.to_vec(), key));
        Ok(())
    }
}

/// H.264 samples (Annex B, one per frame, display order, with key flags) and the audio columns (as Opus) into an MP4.
pub fn mux_h264(
    out: &Path,
    opts: &EncodeOpts,
    frames: &[(Vec<u8>, bool)],
    audio: Option<&Audio>,
) -> R<EncodeStats> {
    let file = std::fs::File::create(&out).map_err(|e| e.to_string())?;
    let mut b = muxide::api::MuxerBuilder::new(std::io::BufWriter::new(file))
        .video(
            muxide::api::VideoCodec::H264,
            opts.width,
            opts.height,
            opts.fps,
        )
        .with_fast_start(true);
    let packets = match audio {
        Some(a) => Some(opus_packets(a)?),
        None => None,
    };
    if let Some(a) = audio {
        b = b.audio(
            muxide::api::AudioCodec::Opus,
            48000,
            a.channels.len().clamp(1, 2) as u16,
        );
    }
    let mut m = b.build().map_err(|e| format!("muxide: {e}"))?;
    // interleave by time
    let mut ai = 0;
    for (k, (data, key)) in frames.iter().enumerate() {
        let pts = k as f64 / opts.fps;
        if let Some(p) = &packets {
            while ai < p.len() && p[ai].0 < pts {
                m.write_audio(p[ai].0, &p[ai].1)
                    .map_err(|e| format!("muxide audio: {e}"))?;
                ai += 1;
            }
        }
        m.write_video(pts, data, *key)
            .map_err(|e| format!("muxide video: {e}"))?;
    }
    let end = frames.len() as f64 / opts.fps;
    if let Some(p) = &packets {
        for (pts, d) in p[ai..].iter().filter(|x| x.0 < end) {
            m.write_audio(*pts, d)
                .map_err(|e| format!("muxide audio: {e}"))?;
        }
    }
    m.finish().map_err(|e| format!("muxide: {e}"))?;
    Ok(EncodeStats {
        frames: frames.len(),
        bytes: std::fs::metadata(&out).map(|m| m.len()).unwrap_or(0),
        video_codec: "h264 (openh264)".into(),
        audio_codec: if audio.is_some() {
            "opus (libopus)".into()
        } else {
            String::new()
        },
    })
}

/// An encoder for one GOP that depends on nothing before it: fixed quantiser (fixed_qp: the rate control can't move
/// it, so no state is carried between frames), the first frame an IDR with its SPS/PPS, no other keyframes. Two GOPs
/// made from the same frames are the same bytes, so an export can be stitched from GOPs made at different times.
#[cfg(feature = "h264")]
pub struct GopEncoder {
    pub(crate) enc: H264Encoder,
    pub(crate) w: usize,
    pub(crate) h: usize,
}

#[cfg(feature = "h264")]
impl GopEncoder {
    pub fn new(w: u32, h: u32, fps: f64, qp: u8) -> R<GopEncoder> {
        let config = EncoderConfig::new()
            .max_frame_rate(FrameRate::from_hz(fps as f32))
            .skip_frames(false)
            .scene_change_detect(false)
            .adaptive_quantization(false)
            .background_detection(false)
            .intra_frame_period(openh264::encoder::IntraFramePeriod::from_num_frames(
                100_000,
            ))
            .num_threads(1)
            // BT.709 limited range, as h264_encoder tags its stream
            .vui(openh264::encoder::VuiConfig::bt709());
        let enc = H264Encoder::with_api_config(OpenH264API::from_source(), crate::fixed_qp(config, qp))
            .map_err(|e| format!("openh264: {e}"))?;
        Ok(GopEncoder {
            enc,
            w: w as usize,
            h: h as usize,
        })
    }
    pub fn push(&mut self, rgba: &[u8]) -> R<(Vec<u8>, bool)> {
        let (y, u, v) = raea_io_codec::yuv::rgba_to_i420(rgba, self.w, self.h);
        let src = YUVSlices::new(
            (&y, &u, &v),
            (self.w, self.h),
            (self.w, self.w / 2, self.w / 2),
        );
        let bs = self
            .enc
            .encode(&src)
            .map_err(|e| format!("openh264: {e}"))?;
        let key = matches!(
            bs.frame_type(),
            openh264::encoder::FrameType::IDR | openh264::encoder::FrameType::I
        );
        Ok((bs.to_vec(), key))
    }
}
