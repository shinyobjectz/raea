//! `yt-dlp -J` read: which of a video's streams raea reads. The best picture at 1080p or under in the codec raea's
//! decoders take best (H.264, then HEVC, then the rest, before height), and the best sound beside it; one stream holding both is taken
//! when it's as tall as the best picture alone.

use serde_json::Value;

/// The streams chosen for one video: yt-dlp format ids, the picture's first
#[derive(Debug, Clone, PartialEq)]
pub struct Pick {
    pub picture: String,
    /// a separate sound stream; None when the picture's stream carries the sound, or there's none
    pub sound: Option<String>,
    pub height: u32,
    pub vcodec: String,
}

/// Streams are read at 1080p at most; reading never needs more
pub const TALLEST: u64 = 1080;

fn has(f: &Value, k: &str) -> bool {
    f[k].as_str().is_some_and(|c| c != "none")
}

/// How well raea decodes a picture codec: VideoToolbox and the portable decoders take these first
fn codec_rank(vcodec: &str) -> u32 {
    let c = vcodec.to_ascii_lowercase();
    if c.starts_with("avc") || c.starts_with("h264") {
        3
    } else if c.starts_with("hvc") || c.starts_with("hev") || c.starts_with("h265") {
        2
    } else if c.starts_with("av01") || c.starts_with("vp9") || c.starts_with("vp09") {
        1
    } else {
        0
    }
}

fn height(f: &Value) -> u64 {
    f.get("height").and_then(Value::as_u64).unwrap_or_default()
}

fn rate(f: &Value) -> f64 {
    f["tbr"].as_f64().or(f["vbr"].as_f64()).or(f["abr"].as_f64()).unwrap_or(0.0)
}

/// The formats a video's info lists, or the info itself when it is one stream (a direct link)
fn formats(info: &Value) -> Vec<Value> {
    match info["formats"].as_array() {
        Some(f) if !f.is_empty() => f.clone(),
        _ => vec![info.clone()],
    }
}

/// The streams to read, or None when the video has no picture raea can read under 1080p
pub fn pick(info: &Value) -> Option<Pick> {
    let all = formats(info);
    // a stream with no stated codec (a direct link's one file) counts as picture and sound
    let unknown = |f: &Value| f["vcodec"].is_null() && f["acodec"].is_null();
    let pictures: Vec<&Value> = all.iter().filter(|f| (has(f, "vcodec") || unknown(f)) && height(f) <= TALLEST).collect();
    // a codec raea decodes before a taller picture: macOS won't decode an Instagram reel's VP9, and the plain H.264 file
    // beside it (no codec or height stated) reads whole
    let key = |f: &&&Value| (codec_rank(f["vcodec"].as_str().unwrap_or("avc")), height(f), (rate(f) * 1000.0) as u64);
    let best = pictures.iter().max_by_key(key)?;
    let both = pictures.iter().filter(|f| has(f, "acodec") || unknown(f)).max_by_key(key);
    let text = |f: &Value, k: &str, or: &str| f[k].as_str().unwrap_or(or).to_string();
    let id = |f: &Value| text(f, "format_id", "0");
    let vcodec = |f: &Value| text(f, "vcodec", "");
    if let Some(b) = both.filter(|b| height(b) >= height(best)) {
        return Some(Pick { picture: id(b), sound: None, height: height(b) as u32, vcodec: vcodec(b) });
    }
    // sound: AAC first (every decoder raea has reads it), then the best rate
    let sound = all.iter().filter(|f| has(f, "acodec") && !has(f, "vcodec")).max_by_key(|f| (u32::from(f["acodec"].as_str().is_some_and(|c| c.starts_with("mp4a"))), (rate(f) * 1000.0) as u64));
    Some(Pick { picture: id(best), sound: sound.map(id), height: height(best) as u32, vcodec: vcodec(best) })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn a_platform_page_splits_picture_and_sound_under_1080p() {
        let info = json!({"formats": [
            {"format_id": "18", "vcodec": "avc1.42001E", "acodec": "mp4a.40.2", "height": 360, "tbr": 500.0},
            {"format_id": "137", "vcodec": "avc1.640028", "acodec": "none", "height": 1080, "tbr": 4000.0},
            {"format_id": "248", "vcodec": "vp9", "acodec": "none", "height": 1080, "tbr": 5000.0},
            {"format_id": "313", "vcodec": "vp9", "acodec": "none", "height": 2160, "tbr": 20000.0},
            {"format_id": "251", "vcodec": "none", "acodec": "opus", "abr": 160.0},
            {"format_id": "140", "vcodec": "none", "acodec": "mp4a.40.2", "abr": 128.0},
        ]});
        assert_eq!(pick(&info), Some(Pick { picture: "137".into(), sound: Some("140".into()), height: 1080, vcodec: "avc1.640028".into() }));
    }

    #[test]
    fn a_direct_link_is_its_one_stream() {
        let info = json!({"format_id": "mp4", "url": "https://x/y.mp4", "ext": "mp4"});
        assert_eq!(pick(&info).map(|p| (p.picture, p.sound)), Some(("mp4".into(), None)));
    }

    #[test]
    fn one_stream_with_both_wins_when_its_as_tall() {
        let info = json!({"formats": [
            {"format_id": "hls-720", "vcodec": "avc1", "acodec": "mp4a", "height": 720, "tbr": 2500.0},
            {"format_id": "dash-v720", "vcodec": "avc1", "acodec": "none", "height": 720, "tbr": 2400.0},
            {"format_id": "dash-a", "vcodec": "none", "acodec": "mp4a", "abr": 128.0},
        ]});
        assert_eq!(pick(&info).map(|p| (p.picture, p.sound)), Some(("hls-720".into(), None)));
    }
}
