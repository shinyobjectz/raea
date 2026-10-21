//! (d) Royalty-free AV1: rav1d decodes (dav1d ported to Rust, BSD-2: no system library, builds on every target), rav1e encodes (Rust, BSD-2), muxide
//! writes the MP4, Opus carries the audio. AV1 is royalty-free (AOMedia patent licence), so this is the path with no
//! codec patent question at all.

use std::path::{Path, PathBuf};

use crate::{
    Audio, Backend, EncodeOpts, EncodeStats, Frame, Info, R, VideoDecoder, VideoEncoder, as_of,
};

pub struct Av1;

/// rav1d through dav1d's API (it is a Rust port with the same entry points).
struct Rav1d(rav1d::include::dav1d::dav1d::Dav1dContext);

impl Rav1d {
    fn new() -> R<Rav1d> {
        use rav1d::include::dav1d::dav1d::Dav1dSettings;
        use std::ptr::NonNull;
        let mut s = std::mem::MaybeUninit::<Dav1dSettings>::uninit();
        unsafe { rav1d::src::lib::dav1d_default_settings(NonNull::new(s.as_mut_ptr()).unwrap()) };
        let mut s = unsafe { s.assume_init() };
        s.n_threads = 4;
        s.max_frame_delay = 1;
        let mut ctx = None;
        let r =
            unsafe { rav1d::src::lib::dav1d_open(NonNull::new(&mut ctx), NonNull::new(&mut s)) };
        if r.0 != 0 {
            return Err(format!("rav1d: open {}", r.0));
        }
        Ok(Rav1d(ctx.ok_or("rav1d: no context")?))
    }
}

impl Drop for Rav1d {
    fn drop(&mut self) {
        let mut c = Some(self.0);
        unsafe { rav1d::src::lib::dav1d_close(std::ptr::NonNull::new(&mut c)) };
    }
}

struct Decoder {
    track: crate::soft::Track,
    info: Info,
    /// display times in output order (the decoder gives frames in display order)
    pts: Vec<f64>,
    dec: Option<Rav1d>,
    /// a sample the decoder couldn't take yet
    pending: Option<rav1d::include::dav1d::data::Dav1dData>,
    /// samples sent so far (decode order), and frames received (display order)
    sent: usize,
    got: usize,
    /// RGBA, or NV12 when `nv12`
    out: Vec<u8>,
    nv12: bool,
}

const EAGAIN: i32 = -(libc_eagain() as i32);

const fn libc_eagain() -> u8 {
    // EAGAIN is 35 on Apple, 11 on Linux and Android
    if cfg!(target_vendor = "apple") {
        35
    } else {
        11
    }
}

impl Decoder {
    fn restart(&mut self, sample: usize) -> R<()> {
        self.dec = Some(Rav1d::new()?);
        self.pending = None;
        self.sent = sample;
        // frames before the keyframe's display position aren't coming back
        self.got = sample;
        Ok(())
    }

    /// The next frame in display order.
    fn next(&mut self) -> R<()> {
        use rav1d::include::dav1d::data::Dav1dData;
        use rav1d::include::dav1d::picture::Dav1dPicture;
        use std::ptr::NonNull;
        loop {
            let ctx = self.dec.as_ref().unwrap().0;
            let mut pic = Dav1dPicture::default();
            let r =
                unsafe { rav1d::src::lib::dav1d_get_picture(Some(ctx), NonNull::new(&mut pic)) };
            if r.0 == 0 {
                let (w, h) = (pic.p.w as usize, pic.p.h as usize);
                let (ys, cs) = (pic.stride[0] as usize, pic.stride[1] as usize);
                let plane = |i: usize, len: usize| unsafe {
                    std::slice::from_raw_parts(pic.data[i].unwrap().as_ptr() as *const u8, len)
                };
                let (y, u, v) = (
                    plane(0, ys * h),
                    plane(1, cs * h.div_ceil(2)),
                    plane(2, cs * h.div_ceil(2)),
                );
                if self.nv12 {
                    crate::yuv::i420_to_nv12(y, u, v, ys, cs, cs, w, h, &mut self.out);
                } else {
                    crate::yuv::i420_to_rgba(y, u, v, ys, cs, cs, w, h, &mut self.out);
                }
                unsafe { rav1d::src::lib::dav1d_picture_unref(NonNull::new(&mut pic)) };
                self.got += 1;
                return Ok(());
            }
            if r.0 != EAGAIN {
                return Err(format!("rav1d: get_picture {}", r.0));
            }
            // it wants data: the pending sample first, else the next one, else drain
            let mut data = match self.pending.take() {
                Some(d) => d,
                None => {
                    if self.sent >= self.track.samples.len() {
                        return Err("rav1d: no more frames".into());
                    }
                    let (off, size, _, _) = self.track.samples[self.sent];
                    self.sent += 1;
                    let mut d = Dav1dData::default();
                    let buf =
                        unsafe { rav1d::src::lib::dav1d_data_create(NonNull::new(&mut d), size) };
                    if buf.is_null() {
                        return Err("rav1d: data_create".into());
                    }
                    unsafe {
                        std::ptr::copy_nonoverlapping(
                            self.track.bytes[off..off + size].as_ptr(),
                            buf,
                            size,
                        )
                    };
                    d
                }
            };
            let r = unsafe { rav1d::src::lib::dav1d_send_data(Some(ctx), NonNull::new(&mut data)) };
            if r.0 == EAGAIN || (r.0 == 0 && data.sz > 0) {
                self.pending = Some(data);
            } else if r.0 != 0 {
                return Err(format!("rav1d: send_data {}", r.0));
            }
        }
    }
}

impl VideoDecoder for Decoder {
    fn info(&self) -> &Info {
        &self.info
    }
    fn index_at(&mut self, t: f64) -> Option<usize> {
        (!self.pts.is_empty()).then(|| crate::as_of(&self.pts, t))
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

impl Decoder {
    /// Switch between RGBA and NV12 output (decoding restarts).
    fn mode(&mut self, nv12: bool) {
        if self.nv12 != nv12 {
            self.nv12 = nv12;
            self.dec = None;
        }
    }

    fn frame_inner(&mut self, t: f64) -> R<Frame> {
        let want = as_of(&self.pts, t);
        // got is how many display frames have come out; the last one out is index got-1
        let behind =
            self.dec.is_none() || self.got == 0 || want + 1 < self.got || want >= self.got + 60;
        if behind {
            // restart at the keyframe whose display position is at or before `want`
            let key = (0..self.track.samples.len())
                .rev()
                .find(|&i| self.track.samples[i].2 && i <= want)
                .unwrap_or(0);
            self.restart(key)?;
        }
        while self.got <= want {
            self.next()?;
        }
        Ok(Frame {
            pts: self.pts[want],
            width: self.info.width,
            height: self.info.height,
            rgba: if self.nv12 {
                crate::yuv::rotate_nv12(&self.out, self.track.width as usize, self.track.height as usize, self.track.rotation).0
            } else {
                crate::yuv::rotate(&self.out, self.track.width as usize, self.track.height as usize, self.track.rotation).0
            },
        })
    }
}

struct Encoder {
    ctx: rav1e::Context<u8>,
    opts: EncodeOpts,
    out: PathBuf,
    packets: Vec<(u64, Vec<u8>, bool)>,
    pushed: usize,
}

impl Encoder {
    fn drain(&mut self) -> R<()> {
        loop {
            match self.ctx.receive_packet() {
                Ok(p) => self.packets.push((
                    p.input_frameno,
                    p.data,
                    p.frame_type == rav1e::prelude::FrameType::KEY,
                )),
                Err(rav1e::EncoderStatus::Encoded) => continue,
                Err(rav1e::EncoderStatus::NeedMoreData)
                | Err(rav1e::EncoderStatus::LimitReached) => return Ok(()),
                Err(e) => return Err(format!("rav1e: {e:?}")),
            }
        }
    }
}

impl VideoEncoder for Encoder {
    fn push(&mut self, rgba: &[u8]) -> R<()> {
        let (w, h) = (self.opts.width as usize, self.opts.height as usize);
        let (y, u, v) = crate::yuv::rgba_to_i420(rgba, w, h);
        self.push_i420(&y, &u, &v)
    }
    fn push_nv12(&mut self, nv12: &[u8], w: usize, h: usize) -> R<()> {
        let (y, u, v) = crate::yuv::nv12_to_i420(nv12, w, h);
        self.push_i420(&y, &u, &v)
    }
    fn finish(mut self: Box<Self>, audio: Option<&Audio>) -> R<EncodeStats> {
        self.ctx.flush();
        self.drain()?;
        self.packets.sort_by_key(|p| p.0);
        let file = std::fs::File::create(&self.out).map_err(|e| e.to_string())?;
        let mut b = muxide::api::MuxerBuilder::new(std::io::BufWriter::new(file))
            .video(
                muxide::api::VideoCodec::Av1,
                self.opts.width,
                self.opts.height,
                self.opts.fps,
            )
            .with_fast_start(true);
        let packets = match audio {
            Some(a) => Some(crate::soft::opus_packets(a)?),
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
        let mut ai = 0;
        for (n, data, key) in &self.packets {
            let pts = *n as f64 / self.opts.fps;
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
        let end = self.pushed as f64 / self.opts.fps;
        if let Some(p) = &packets {
            for (pts, d) in p[ai..].iter().filter(|x| x.0 < end) {
                m.write_audio(*pts, d)
                    .map_err(|e| format!("muxide audio: {e}"))?;
            }
        }
        m.finish().map_err(|e| format!("muxide: {e}"))?;
        Ok(EncodeStats {
            frames: self.pushed,
            bytes: std::fs::metadata(&self.out).map(|m| m.len()).unwrap_or(0),
            video_codec: "av1 (rav1e)".into(),
            audio_codec: if audio.is_some() {
                "opus (libopus)".into()
            } else {
                String::new()
            },
        })
    }
}

impl Encoder {
    fn push_i420(&mut self, y: &[u8], u: &[u8], v: &[u8]) -> R<()> {
        let w = self.opts.width as usize;
        let mut f = self.ctx.new_frame();
        f.planes[0].copy_from_raw_u8(y, w, 1);
        f.planes[1].copy_from_raw_u8(u, w / 2, 1);
        f.planes[2].copy_from_raw_u8(v, w / 2, 1);
        self.ctx.send_frame(f).map_err(|e| format!("rav1e: {e:?}"))?;
        self.pushed += 1;
        self.drain()
    }
}

impl Backend for Av1 {
    fn name(&self) -> &'static str {
        "av1"
    }
    fn open_video(&self, path: &Path) -> R<Box<dyn VideoDecoder>> {
        let track = crate::soft::demux(path, "av01")?;
        let mut pts: Vec<f64> = track.samples.iter().map(|s| s.3).collect();
        pts.sort_by(f64::total_cmp);
        let fps = if pts.len() > 1 {
            (pts.len() - 1) as f64 / (pts[pts.len() - 1] - pts[0]).max(1e-9)
        } else {
            30.0
        };
        // the display size: a quarter turn swaps the sides
        let (dw, dh) = if track.rotation % 180 == 90 { (track.height, track.width) } else { (track.width, track.height) };
        let info = Info {
            codec: track.codec.clone(),
            width: dw,
            height: dh,
            fps,
            duration: track.duration,
            frames: pts.len(),
        };
        Ok(Box::new(Decoder {
            info,
            pts,
            dec: None,
            pending: None,
            sent: 0,
            got: 0,
            out: Vec::new(),
            nv12: false,
            track,
        }))
    }
    fn decode_audio(&self, path: &Path) -> R<Audio> {
        crate::soft::decode_audio_symphonia(path)
    }
    fn encoder(&self, out: &Path, o: &EncodeOpts) -> R<Box<dyn VideoEncoder>> {
        let mut enc = rav1e::EncoderConfig::with_speed_preset(10);
        enc.width = o.width as usize;
        enc.height = o.height as usize;
        enc.time_base = rav1e::data::Rational::new(1, o.fps.round() as u64);
        enc.bitrate = (o.kbps * 1000) as i32;
        enc.min_key_frame_interval = 0;
        enc.max_key_frame_interval = (o.fps.round() as u64) * 2;
        enc.low_latency = true;
        let cfg = rav1e::Config::new()
            .with_encoder_config(enc)
            .with_threads(8);
        let ctx = cfg.new_context::<u8>().map_err(|e| format!("rav1e: {e}"))?;
        Ok(Box::new(Encoder {
            ctx,
            opts: o.clone(),
            out: out.to_path_buf(),
            packets: Vec::new(),
            pushed: 0,
        }))
    }
}
