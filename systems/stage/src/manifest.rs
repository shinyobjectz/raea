//! The render manifest and the stage checks: each frame's page read from it (what SG11 and SG14 hold the parts to),
//! and what the checks write into it of the parts (what SG9 names and samples).

use std::collections::HashMap;

use raea_core_scene::{Item, base_ident, spawn_sig};
use serde_json::{Value, json};

use crate::{Page, Words};

/// A render of at most this many frames is stills (`--frame`, `--at`): its manifest names each frame's parts on screen
const STILLS: usize = 16;
/// More than this many parts mounting or unmounting between two frames is a remount
const REMOUNT: usize = 20;

/// What each frame (`nums`) of the manifest's frames draws in its page: its words (what they say, their ink's box,
/// whether they may sit on a part), its videos' boxes and its pictures', the nearly invisible left out, and a CSS
/// background (a colour or a gradient) over half the w x h frame or more: the page's ground, under the 3D or alone, not
/// something shown (an <img> or an svg is). The film's blank cream frame at 6:04 read as full while its background
/// counted (SG14).
pub fn pages(frames: &[Value], nums: &[i64], w: u32, h: u32) -> Vec<Page> {
    let by: HashMap<i64, &Value> = frames.iter().filter_map(|e| Some((e["frame"].as_i64()?, e))).collect();
    // an entry's box as [x0, y0, x1, y1], when it shows
    let shown = |e: &Value, key: &str| {
        let b: Vec<f32> = e.get(key)?.as_array()?.iter().filter_map(|v| Some(v.as_f64()? as f32)).collect();
        let [x, y, bw, bh]: [f32; 4] = b.try_into().ok()?;
        (bw > 0.0 && bh > 0.0 && e["opacity"].as_f64().unwrap_or(1.0) >= 0.1).then_some([x, y, x + bw, y + bh])
    };
    fn list<'a>(e: Option<&&'a Value>, key: &str) -> impl Iterator<Item = &'a Value> {
        e.and_then(|e| e[key].as_array()).into_iter().flatten()
    }
    let words = |t: &Value| {
        let text = Some(t["text"].as_str()?.trim()).filter(|s| !s.is_empty())?.into();
        let points = serde_json::from_value::<[f32; 3]>(t["points"].clone()).ok();
        Some(Words { text, bx: shown(t, "ink").or_else(|| shown(t, "box"))?, allow_overlap: t["allowOverlap"] == true, points })
    };
    nums.iter()
        .map(|f| Page {
            words: list(by.get(f), "text").filter_map(words).collect(),
            videos: list(by.get(f), "videos").filter_map(|v| shown(v, "box")).collect(),
            pictures: list(by.get(f), "pictures").filter_map(|v| shown(v, "box").filter(|b| !matches!(v["kind"].as_str(), Some("fill" | "background")) || (b[2] - b[0]) * (b[3] - b[1]) < 0.5 * (w * h) as f32)).collect(),
        })
        .collect()
}

/// What the manifest says of a render's parts: for stills, each frame's parts on screen and their boxes (`parts`); for
/// a longer render, the frames where more than REMOUNT parts mount or unmount (`remounts`). SG9 re-renders the frame
/// after each remount alone and names the parts under a patch that differs.
pub fn seen(all: &[Vec<Item>], nums: &[i64], w: u32, h: u32) -> Value {
    let mut m = Mounts::new(w, h);
    for (its, f) in all.iter().zip(nums) {
        m.push(*f, its);
    }
    m.value()
}

/// seen as a render's frames come: each frame's count against the one before, one count held at a time (a film's
/// 13,175 at once were a GB), and each frame's parts on screen while it may still be stills.
pub struct Mounts {
    w: u32,
    h: u32,
    n: usize,
    stills: Vec<Value>,
    before: Option<Counted>,
    remounts: Vec<i64>,
}

impl Mounts {
    pub fn new(w: u32, h: u32) -> Mounts {
        Mounts { w, h, n: 0, stills: Vec::new(), before: None, remounts: Vec::new() }
    }

    /// The next frame (its number, `frame`) and its parts.
    pub fn push(&mut self, frame: i64, its: &[Item]) {
        if self.n < STILLS {
            self.stills.push(json!({"frame": frame, "parts": crate::on_screen(its, self.w, self.h)}));
        } else {
            self.stills.clear();
        }
        let now = counted(its);
        if self.before.as_ref().is_some_and(|b| changed(&now, b) > REMOUNT) {
            self.remounts.push(frame);
        }
        self.before = Some(now);
        self.n += 1;
    }

    /// What the manifest says: stills' parts on screen, a longer render's remounts.
    pub fn value(self) -> Value {
        if self.n <= STILLS { json!({"parts": self.stills}) } else { json!({"remounts": self.remounts}) }
    }
}

type Counted = HashMap<(&'static str, String, u64), i64>;

/// A frame's parts as a count by what they are: kind, name, what they draw.
fn counted(its: &[Item]) -> Counted {
    let mut m = Counted::new();
    for it in its {
        *m.entry((it.kind, base_ident(it).to_string(), spawn_sig(it))).or_default() += 1;
    }
    m
}

/// How many parts mount or unmount from one frame's count to the next's.
fn changed(a: &Counted, b: &Counted) -> usize {
    let off = |x: &Counted, y: &Counted| x.iter().map(|(k, n)| (n - y.get(k).copied().unwrap_or(0)).max(0) as usize).sum::<usize>();
    off(a, b) + off(b, a)
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    #[test]
    fn a_frames_page_is_its_words_videos_and_pictures() {
        let frames = vec![json!({"frame": 7, "text": [
            {"text": "Hello", "box": [0, 0, 400, 40], "ink": [10, 0, 100, 40], "opacity": 1.0},
            {"text": "faded", "box": [0, 50, 400, 40], "opacity": 0.05},
            {"text": "on it", "box": [0, 100, 400, 40], "allowOverlap": true}
        ], "videos": [{"box": [0, 0, 1920, 1080], "opacity": 1.0}], "pictures": [{"box": [5, 5, 10, 10]}]})];
        let p = super::pages(&frames, &[7, 8], 1920, 1080);
        assert_eq!(p.len(), 2);
        assert_eq!(p[0].words.len(), 2, "{:?}", p[0].words);
        assert_eq!((p[0].words[0].bx, p[0].words[1].allow_overlap), ([10.0, 0.0, 110.0, 40.0], true));
        assert_eq!((p[0].videos.len(), p[0].pictures[0]), (1, [5.0, 5.0, 15.0, 15.0]));
        assert!(p[1].words.is_empty() && p[1].videos.is_empty());
    }

    #[test]
    fn the_pages_background_fill_is_ground_not_shown() {
        // the film's 6:04: its cream background, twice, full frame, and a gradient; a lower third's bar and a photo stay
        let frames = vec![json!({"frame": 1, "pictures": [
            {"kind": "fill", "box": [0, 0, 1920, 1080], "opacity": 1.0},
            {"kind": "fill", "box": [0, 0, 1920, 1080], "opacity": 1.0},
            {"kind": "fill", "box": [52, 862, 645, 44], "opacity": 1.0},
            {"kind": "background", "box": [0, 0, 1920, 1080], "opacity": 1.0},
            {"kind": "img", "box": [0, 0, 1920, 1080], "opacity": 1.0}
        ]})];
        let p = super::pages(&frames, &[1], 1920, 1080);
        assert_eq!(p[0].pictures, vec![[52.0, 862.0, 697.0, 906.0], [0.0, 0.0, 1920.0, 1080.0]]);
    }
}
