//! The host page's `video` and `audio` (raea-runtime-js's media_html.rs, on the Blitz page): a source is decoded by raea-media
//! on a thread of its own (raea-runtime-js's stream), what it tells goes to the element's JS as raea-runtime-js's `media` host event
//! (`loadedmetadata`, `loadeddata`, `seeked`, `error`: sdk/ui's dom layer fires each at the element), and a video's
//! frame is its picture, which its natural size is. Its poster shows until the first seek; with none, the frame does.
//! A seek is the prop `__seek` ([time, n], set by the dom layer's `currentTime`); playing is `__play` ([on, time, n,
//! loop], set by `play()` and `pause()`), `__muted` the element's muted and `__volume` its volume. Playing, the decoder's frames come on the
//! source's clock (its sound's: raea-runtime-js media_play.rs), each a `time` news for the element's currentTime, with
//! `timeupdate` four times a second as a browser's, `playing` once it's going, and `ended` at its end. A playing
//! video's frames come ahead of their time, each with the moment it is due: at each sync (the page's own frame) the
//! one due then is shown, so the picture keeps the page's rhythm whatever the decoder's. raea-runtime-dom's
//! page host keeps one `Medias` and syncs it after each commit.

use std::collections::{HashMap, VecDeque};
use std::sync::Arc;
use std::time::{Duration, Instant};

use blitz_dom::BaseDocument;
use blitz_dom::node::{ImageData, RasterImageData, SpecialElementData};
use raea_runtime_js::media_html::stream::{Cmd, Halvings, Msg, Stream};
use serde_json::{Value, json};

use raea_sys_page::ops::Nodes;

/// One media element's source and what its decoder has told.
pub struct Media {
    src: String,
    /// `__load` as last seen (the dom layer's load() count)
    load: u64,
    poster: Option<String>,
    seek: Option<Value>,
    stream: Option<Stream>,
    video: bool,
    loaded: bool,
    /// a seek asked and not landed yet
    seeking: Option<f64>,
    /// a seek has landed (the poster's gone)
    seeked: bool,
    duration: f64,
    /// the latest frame: width, height, pixels
    frame: Option<(u32, u32, Arc<Vec<u8>>)>,
    /// what it is drawn from when it came as its size alone (its halving), until that goes to the painter with it
    halves: Halvings,
    /// how many halvings its decoder is asked to send with each frame
    halvings: u8,
    /// a playing video's frames not due yet: each its time, its picture and when to show it
    ahead: VecDeque<(f64, (u32, u32, Vec<u8>), Instant, Halvings)>,
    /// when this was last synced, and how far apart syncs have been lately (the page's frame, s)
    synced: (Option<Instant>, f64),
    /// `__play` as last seen, and whether it asks to play
    play: Option<Value>,
    playing: bool,
    muted: Option<bool>,
    volume: Option<f64>,
    /// when the last `timeupdate` went
    told: Option<std::time::Instant>,
    /// RAEA_MEDIA_STATS=1: since when, frames shown, frames a newer one took the place of before the page drew, and
    /// the time setting the picture took (sum and max, ms)
    stats: Option<(std::time::Instant, u32, u32, f64, f64)>,
    /// RAEA_MEDIA_STATS=1: when a frame was last shown, and the longest wait between two this second (ms)
    shown_gap: (Option<std::time::Instant>, f64),
    /// RAEA_MEDIA_STATS=1, this second: the page's frames, the longest (ms), and how late frames were shown (sum, max; ms)
    paced: (u32, f64, f64, f64),
}

impl Media {
    /// Whether `t` is where the seek asked for landed (it's then asked no more).
    fn landed(&mut self, t: f64) -> bool {
        self.seeking.take_if(|s| (*s - t).abs() < 1e-9).is_some()
    }
}

/// Each media element by its wire id.
pub type Medias = HashMap<u64, Media>;

/// The elements' sources and seeks brought up to their props, and what their decoders told since: the `media` events
/// for the element's JS in `out`, and each video's picture set.
/// `draw`: a video's frames are its picture; false when the page's caller places them itself (html.rs, each at the
/// time a seek asks, not when a stream gets there), and only the events go to the JS.
pub fn sync(doc: &mut BaseDocument, nodes: &Nodes, medias: &mut Medias, out: &mut Vec<Value>, draw: bool) {
    medias.retain(|id, _| nodes.by_id.contains_key(id));
    for id in nodes.media_ids() {
        let video = nodes.kind(id) == Some("video");
        let props = nodes.props.get(&id);
        let s = |k: &str| props.and_then(|p| p.get(k)).and_then(Value::as_str).filter(|v| !v.is_empty()).map(String::from);
        let (src, poster, seek) = (s("src").unwrap_or_default(), s("poster"), props.and_then(|p| p.get("__seek")).cloned());
        // load(): its source opened again, even the same one
        let load = props.and_then(|p| p.get("__load")).and_then(Value::as_u64).unwrap_or(0);
        let m = match medias.get_mut(&id) {
            Some(m) if m.src == src && m.load == load => m,
            _ => {
                let stream = (!src.is_empty()).then(|| raea_runtime_js::media_html::stream::open(&raea_sys_page::net::page_path(&src), video)).flatten();
                medias.insert(id, Media { src, load, poster: None, seek: None, stream, video, loaded: false, seeking: None, seeked: false, duration: f64::NAN, frame: None, halves: Vec::new(), halvings: 0, ahead: VecDeque::new(), synced: (None, 1.0 / 60.0), paced: (0, 0.0, 0.0, 0.0), play: None, playing: false, muted: None, volume: None, told: None, shown_gap: (None, 0.0), stats: (std::env::var("RAEA_MEDIA_STATS").as_deref() == Ok("1")).then(|| (std::time::Instant::now(), 0, 0, 0.0, 0.0)) });
                medias.get_mut(&id).unwrap()
            }
        };
        let mut redraw = false;
        let mut came = 0u32;
        if m.poster != poster {
            m.poster = poster;
            redraw = true;
        }
        let muted = props.and_then(|p| p.get("__muted")).and_then(Value::as_bool);
        if m.muted != muted {
            if let (Some(v), Some(st)) = (muted, m.stream.as_mut()) {
                st.ask(Cmd::Muted(v));
            }
            m.muted = muted;
        }
        let volume = props.and_then(|p| p.get("__volume")).and_then(Value::as_f64);
        if m.volume != volume {
            if let (Some(v), Some(st)) = (volume, m.stream.as_mut()) {
                st.ask(Cmd::Volume(v as f32));
            }
            m.volume = volume;
        }
        if m.seek != seek {
            if let Some(t) = seek.as_ref().and_then(|v| v.get(0)).and_then(Value::as_f64)
                && let Some(st) = m.stream.as_mut()
            {
                m.seeking = Some(t);
                st.ask(Cmd::Seek(t));
            }
            m.seek = seek;
        }
        let play = props.and_then(|p| p.get("__play")).cloned();
        if m.play != play {
            if let (Some(p), Some(st)) = (play.as_ref(), m.stream.as_mut()) {
                m.playing = p.get(0).and_then(Value::as_bool).unwrap_or(false);
                if m.playing {
                    st.ask(Cmd::Play { at: p.get(1).and_then(Value::as_f64).unwrap_or(0.0), looping: p.get(3).and_then(Value::as_bool).unwrap_or(false) });
                } else {
                    st.ask(Cmd::Pause);
                    m.ahead.clear();
                }
            }
            m.play = play;
        }
        while let Some(msg) = m.stream.as_ref().and_then(Stream::next) {
            match msg {
                Msg::Meta { w, h, duration } => {
                    m.duration = duration;
                    out.push(if m.video { json!({"id": id, "type": "loadedmetadata", "w": w, "h": h, "duration": duration}) } else { json!({"id": id, "type": "loadedmetadata", "duration": duration}) });
                }
                Msg::Fail(err) => {
                    bevy::log::warn!("raea-page: media: {err}");
                    out.push(json!({"id": id, "type": "error", "message": err}));
                }
                Msg::Frame(t, f, halves) => {
                    // the first frame or a seek's: shown now, and what was on its way for before the seek is not
                    m.ahead.clear();
                    let (w, h) = (f.width, f.height);
                    m.frame = Some((w, h, Arc::new(f.rgba)));
                    m.halves = halves;
                    redraw = true;
                    came += 1;
                    if !m.loaded {
                        m.loaded = true;
                        out.push(json!({"id": id, "type": "loadeddata", "w": w, "h": h, "duration": m.duration}));
                    }
                    if m.landed(t) {
                        m.seeked = true;
                        out.push(json!({"id": id, "type": "seeked", "time": t}));
                    }
                    if m.playing {
                        m.seeked = true;
                        told(m, id, t, out);
                    }
                }
                // a playing video's frame, for later; one that comes after a pause is for nothing
                Msg::Due(t, f, due, halves) => {
                    if m.playing {
                        m.ahead.push_back((t, (f.width, f.height, f.rgba), due, halves));
                    }
                }
                Msg::Time(t) => {
                    if m.landed(t) {
                        out.push(json!({"id": id, "type": "seeked", "time": t}));
                    }
                    if m.playing {
                        told(m, id, t, out);
                    } else {
                        out.push(json!({"id": id, "type": "time", "time": t}));
                    }
                }
                Msg::Playing(t) => {
                    m.ahead.clear();
                    out.push(json!({"id": id, "type": "playing", "time": t}));
                }
                Msg::Ended(t) => {
                    m.playing = false;
                    out.push(json!({"id": id, "type": "ended", "time": t}));
                }
            }
        }
        // the frame due at this sync: the latest whose moment has come, or comes within half a page frame (shown a
        // touch early rather than a whole page frame late); the ones before it were never seen
        let now = Instant::now();
        if let Some(last) = m.synced.0 {
            let dt = now.duration_since(last).as_secs_f64();
            m.synced.1 += 0.1 * (dt.min(0.1) - m.synced.1);
            if m.stats.is_some() && !m.ahead.is_empty() {
                (m.paced.0, m.paced.1) = (m.paced.0 + 1, m.paced.1.max(dt * 1e3));
            }
        }
        m.synced.0 = Some(now);
        let reach = now + Duration::from_secs_f64((0.5 * m.synced.1).min(0.008));
        let (due, passed) = due_by(&mut m.ahead, reach);
        came += passed;
        if let Some((t, (w, h, rgba), at, halves)) = due {
            m.halves = halves;
            let late = now.saturating_duration_since(at).as_secs_f64() * 1e3;
            (m.paced.2, m.paced.3) = (m.paced.2 + late, m.paced.3.max(late));
            m.frame = Some((w, h, Arc::new(rgba)));
            (redraw, m.seeked) = (true, true);
            came += 1;
            told(m, id, t, out);
        }
        // how many halvings its frames should come with: as many as bring one within twice the size it is drawn at
        // (the painter's rule, blitz-paint mip_level), counted from its box as last laid out
        if m.video && draw && let (Some((w, h, _)), Some(n), Some(st)) = (m.frame.as_ref(), nodes.by_id.get(&id), m.stream.as_mut()) {
            let want = doc.get_node(*n).map_or(0, |node| {
                let (l, scale) = (node.final_layout(), doc.viewport().scale_f64());
                let drawn = (l.content_box_width() as f64 * scale / *w as f64).min(l.content_box_height() as f64 * scale / *h as f64);
                let mut n = 0u8;
                while drawn > 0.0 && drawn * f64::from(1u32 << (n + 1)) <= 1.0 && n < 6 {
                    n += 1;
                }
                n
            });
            // RAEA_MEDIA_HALVINGS=0: frames drawn as they come, as before (to compare with)
            let want = if std::env::var("RAEA_MEDIA_HALVINGS").as_deref() == Ok("0") { 0 } else { want };
            if want != m.halvings {
                m.halvings = want;
                st.ask(Cmd::Halvings(want));
            }
        }
        if redraw && m.video && draw {
            let pic = match (&m.poster, m.seeked) {
                (Some(p), false) => poster_image(p).map(|(w, h, px)| (w, h, Arc::new(px))),
                _ => None,
            }
            // the frame's pixels shared, not copied again (a 4K frame is 33 MB)
            .or_else(|| m.frame.as_ref().map(|(w, h, px)| (*w, *h, px.clone())));
            if let (Some((w, h, px)), Some(n)) = (pic, nodes.by_id.get(&id)) {
                let began = std::time::Instant::now();
                set_image(doc, *n, w, h, px, std::mem::take(&mut m.halves));
                if let Some(s) = m.stats.as_mut().filter(|_| came > 0) {
                    let ms = began.elapsed().as_secs_f64() * 1e3;
                    (s.1, s.2, s.3, s.4) = (s.1 + 1, s.2 + came - 1, s.3 + ms, s.4.max(ms));
                    if let Some(l) = m.shown_gap.0 {
                        m.shown_gap.1 = m.shown_gap.1.max(began.duration_since(l).as_secs_f64() * 1e3);
                    }
                    m.shown_gap.0 = Some(began);
                    let took = s.0.elapsed().as_secs_f64();
                    if took >= 1.0 {
                        eprintln!("raea-page: video {took:.2} s: {} frames shown ({w}x{h}), {} passed over before the page drew, picture set avg {:.2} ms max {:.2} ms, longest gap {:.0} ms; the page synced {} times, longest apart {:.0} ms; shown late avg {:.1} ms max {:.1} ms", s.1, s.2, s.3 / s.1.max(1) as f64, s.4, m.shown_gap.1, m.paced.0, m.paced.1, m.paced.2 / s.1.max(1) as f64, m.paced.3);
                        *s = (std::time::Instant::now(), 0, 0, 0.0, 0.0);
                        m.shown_gap.1 = 0.0;
                        m.paced = (0, 0.0, 0.0, 0.0);
                    }
                }
            }
        }
    }
}

/// Of the frames waiting, the one to show by `reach`: the latest whose moment has come, taken out with the ones before
/// it, and how many those were (frames nobody saw: the page drew less often than they came).
fn due_by<T>(ahead: &mut VecDeque<(f64, T, Instant, Halvings)>, reach: Instant) -> (Option<(f64, T, Instant, Halvings)>, u32) {
    let (mut due, mut passed) = (None, 0);
    while ahead.front().is_some_and(|f| f.2 <= reach) {
        passed += due.replace(ahead.pop_front().unwrap()).is_some() as u32;
    }
    (due, passed)
}

/// A playing element's time: each as `time` (its currentTime), and a `timeupdate` four times a second.
fn told(m: &mut Media, id: u64, t: f64, out: &mut Vec<Value>) {
    out.push(json!({"id": id, "type": "time", "time": t}));
    if m.told.is_none_or(|w| w.elapsed() >= std::time::Duration::from_millis(250)) {
        m.told = Some(std::time::Instant::now());
        out.push(json!({"id": id, "type": "timeupdate", "time": t}));
    }
}

/// A poster's picture, decoded as an img's source is.
fn poster_image(src: &str) -> Option<(u32, u32, Vec<u8>)> {
    let (mime, bytes) = match raea_io_net::data_url(src) {
        Some(d) => d,
        None => {
            let path = raea_sys_page::net::page_path(src);
            let ext = path.rsplit_once('.')?.1.to_ascii_lowercase();
            (format!("image/{ext}"), std::fs::read(&path).ok()?)
        }
    };
    let img = raea_sys_image::decode::decode(&bytes, &mime)?;
    let size = img.size();
    Some((size.x, size.y, img.data?))
}

/// Node `n` drawn as an image: `rgba` as its raster. Its layout is made again only when the picture's size isn't the
/// last one's: a playing video's next frame is the same size, and making its box and its ancestors' layout again for
/// each was a part of every page frame it played in.
fn set_image(doc: &mut BaseDocument, n: blitz_dom::NodeId, w: u32, h: u32, rgba: Arc<Vec<u8>>, halves: Halvings) {
    let Some(node) = doc.get_node_mut(n) else { return };
    let Some(el) = node.element_data_mut() else { return };
    let same = matches!(&el.special_data, SpecialElementData::Image(i) if matches!(&**i, ImageData::Raster(r) if (r.width, r.height) == (w, h)));
    let image = RasterImageData { width: w, height: h, data: vello::peniko::Blob::new(rgba) };
    if !halves.is_empty() {
        raea_sys_page::preload_mips(&image, halves);
    }
    el.special_data = SpecialElementData::Image(Box::new(ImageData::Raster(image)));
    if same {
        return node.insert_damage(raea_sys_page::REPAINT_DAMAGE);
    }
    node.clear_layout_cache();
    node.insert_damage(raea_sys_page::ALL_DAMAGE);
    node.mark_ancestors_dirty();
}

#[cfg(test)]
mod tests {
    use super::*;

    /// At a sync the frame shown is the latest one due by then; earlier ones due are passed over, later ones wait.
    #[test]
    fn the_frame_due_is_the_latest_whose_moment_has_come() {
        let t0 = Instant::now();
        let at = |ms: u64| t0 + Duration::from_millis(ms);
        let mut ahead: VecDeque<(f64, u8, Instant, Halvings)> = (0..4).map(|i| (i as f64 / 60.0, i as u8, at(17 * i as u64), Vec::new())).collect();
        // nothing is due before the first
        assert!(due_by(&mut ahead, t0 - Duration::from_millis(1)).0.is_none());
        // a page drawing as often as the frames come shows each
        assert_eq!(due_by(&mut ahead, at(0)).0.map(|f| f.1), Some(0));
        assert_eq!(due_by(&mut ahead, at(17)).0.map(|f| f.1), Some(1));
        // a page a frame late shows the latest and counts the one it passed
        let (due, passed) = due_by(&mut ahead, at(51));
        assert_eq!((due.map(|f| f.1), passed), (Some(3), 1));
        assert!(ahead.is_empty());
    }
}
