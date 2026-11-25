//! Following a videoform (docs/videoform.feature VF9): new footage, read as references are, matched piece by piece to
//! the videoform's shot kinds; what the footage lacks, said before any cut; and the cut planned from the structure, to
//! its lengths. A kind is its shots' mean picture vector with the scale and people most of them have (not talk: a
//! finished video's B-roll plays under a voice its raw footage never had); a piece is its nearest kind's when it's as
//! near as that kind's own shots mostly are. Nothing here opens a reference or the footage: it runs on the kept rows.
//!
//! The cut is planned as it's edited. Where the footage talks, the voice comes first: its lines in the order they were
//! said, each line's last take, the ask at the close and the first lines at the open, trimmed from the middle to the
//! length; then the picture over it, section by section, at the kinds' own lengths in the order the references use
//! them: the speaker in sync, or B-roll of the kind with its sound under the voice. Footage with no talk is cut shot by
//! shot to the kinds.

pub mod kit;
mod layered;
pub mod script;
pub use script::lines;

use raea_sys_videoform::kinds::{Kinds, layered, people};
use raea_sys_videoform::rows::{Rows, Shot};
use raea_sys_videoform::structure::Structure;
use raea_sys_videoform::vocab::dot;
use serde::{Deserialize, Serialize};

/// A kind's own shots mostly sit this near its centre or nearer: the shortest tenth of their nearness sets its floor
const FLOOR_AT: f64 = 0.1;
/// A little under the floor still counts: a kind whose shots are all alike would otherwise take nothing new
const SLACK: f32 = 0.05;
/// How much a measure that differs (scale, people) takes from a piece's nearness
const MEASURE: f32 = 0.06;
/// A kind most references have: one the footage must have, or it's said to lack it
const COMMON: f64 = 0.5;
/// A line is a piece with this many words or more; two lines this alike are two takes of one
pub(crate) const LINE: usize = 3;
const TAKE: f64 = 0.6;
/// The voice may run this far past the length before lines are taken from the middle
const OVER: f64 = 1.15;
/// A cut in the picture lasts at least this long, and at most this long
const SHORTEST: f64 = 0.4;
const LONGEST: f64 = 4.0;

/// A shot kind as the footage is held to it
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Proto {
    pub kind: usize,
    pub name: String,
    pub centre: Vec<f32>,
    /// the scale and people most of its shots have
    pub measures: [String; 2],
    /// a piece this near or nearer is the kind's
    pub floor: f32,
    /// the references it's in, of how many
    pub refs: usize,
    pub of: usize,
    /// how its shots are layered most often (VF8e: cutout, inset, card, backdrop, split; "" never), the share of its
    /// shots that are, and where that layer sits, [x, y, w, h] of the frame, the median of its shots'
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub layer: String,
    #[serde(default)]
    pub layered: f64,
    #[serde(default)]
    pub place: [f64; 4],
    /// what that layer showed in the references, most first (VF9d): a kit's asset kinds
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub shows: Vec<String>,
}

impl Proto {
    /// a kind most of whose shots are layered: a composite, which no one piece of raw footage is
    fn composite(&self) -> bool {
        !self.layer.is_empty() && self.layered >= 0.5
    }
}

impl Proto {
    /// a kind of one person on camera: where the voice is, the speaker in sync
    fn speaker(&self) -> bool {
        self.measures[1] == "one person" && self.measures[0] != "wide"
    }
}

/// A piece of footage, its kind and how near it is
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Piece {
    /// the footage file, as given, and the piece's seconds in it
    pub file: usize,
    pub start: f64,
    pub end: f64,
    pub words: String,
    pub faces: usize,
    pub kind: Option<usize>,
    /// its nearness to its nearest kind less that kind's floor: 0 or above, it's the kind's
    pub fit: f32,
    pub nearest: usize,
}

impl Piece {
    pub(crate) fn said(&self) -> usize {
        self.words.split_whitespace().count()
    }
}

/// A stretch of a piece in the cut: the file's seconds used, where it falls in the cut, the section it's in, and
/// whether it's B-roll over the voice (its sound under it) or heard as it is
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Use {
    pub piece: usize,
    pub file: usize,
    pub from: f64,
    pub to: f64,
    pub at: f64,
    pub slot: usize,
    pub kind: Option<usize>,
    pub over: bool,
    pub why: String,
    /// drawn over the picture before it in the plan, at the same time (VF8e): its form and place; "" for a picture of
    /// its own
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub layer: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub place: Option<[f64; 4]>,
    /// the brand kit's asset it shows instead (VF9b): a file in the brand's fingerprint; "" for the footage's own
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub asset: String,
    /// what the references showed in such a layer, most first (VF9d): the kit's asset is chosen by it
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub shows: Vec<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Slot {
    pub name: String,
    /// where it starts in the cut, and how long it runs there; the references' median, scaled to the cut's length
    pub at: f64,
    pub seconds: f64,
    pub wanted: f64,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Plan {
    pub seconds: f64,
    pub pieces: Vec<Piece>,
    /// what the footage lacks, a line each: the person's shot list
    pub lacks: Vec<String>,
    pub slots: Vec<Slot>,
    /// the lines heard, in order (empty when the footage doesn't talk), and the picture, in order
    pub voice: Vec<Use>,
    pub picture: Vec<Use>,
}

fn measures(s: &Shot) -> [String; 2] {
    [s.framing.clone(), people(s).into()]
}

fn vector(rows: &Rows, s: usize) -> &[f32] {
    let w = rows.sig.len() / rows.shots.len().max(1);
    &rows.sig[s * w..(s + 1) * w]
}

fn unit(mut v: Vec<f32>) -> Vec<f32> {
    let n = dot(&v, &v).sqrt().max(1e-6);
    v.iter_mut().for_each(|x| *x /= n);
    v
}

fn most(v: impl Iterator<Item = String>) -> String {
    let mut all: Vec<String> = v.collect();
    all.sort();
    all.chunk_by(|a, b| a == b).max_by_key(|c| c.len()).and_then(|c| c.first().cloned()).unwrap_or_default()
}

/// A piece's nearness to a kind: the cosine of their pictures, less a little for each measure that differs
fn near(p: &Proto, v: &[f32], m: &[String; 2]) -> f32 {
    dot(&p.centre, v) - MEASURE * p.measures.iter().zip(m).filter(|(a, b)| a != b).count() as f32
}

/// Each kind as footage is held to it, from the references' rows
pub fn protos(refs: &[Rows], kinds: &Kinds) -> Vec<Proto> {
    kinds.kinds.iter().enumerate().map(|(k, kind)| {
        let w = refs.iter().find(|r| !r.shots.is_empty()).map_or(0, |r| r.sig.len() / r.shots.len());
        let mut c = vec![0f32; w];
        for &(r, s) in &kind.shots {
            c.iter_mut().zip(vector(&refs[r], s)).for_each(|(a, b)| *a += b);
        }
        let shots = || kind.shots.iter().map(|&(r, s)| &refs[r].shots[s]);
        let ms = [most(shots().map(|s| s.framing.clone())), most(shots().map(|s| people(s).to_string()))];
        // a screen is shown, never made from footage
        let layer = most(shots().map(layered).filter(|l| !["", "screen"].contains(l)).map(String::from));
        let layered = shots().filter(|s| s.has_layer(&layer)).count() as f64 / kind.shots.len().max(1) as f64;
        // its layer's place: the references' most typical box (the one nearest all the others), a real one
        let boxes: Vec<[f64; 4]> = shots().flat_map(|s| s.layers.iter().flatten()).filter(|l| l.form == layer).map(|l| l.at).collect();
        let place = layered::typical(&boxes);
        // what its layer showed, most first: the kind's own count (VF9d)
        let ranked: std::collections::BTreeSet<_> = kind.shows.iter().map(|(k, v)| (std::cmp::Reverse((v * 1e3) as u32), k.clone())).collect();
        let shows = ranked.into_iter().map(|x| x.1).collect();
        let mut p = Proto { kind: k, name: kind.name.clone(), centre: unit(c), measures: ms, floor: 0.0, refs: kind.refs.len(), of: refs.len(), layer, layered, place, shows };
        let mut own: Vec<f32> = kind.shots.iter().map(|&(r, s)| near(&p, vector(&refs[r], s), &measures(&refs[r].shots[s]))).collect();
        own.sort_by(f32::total_cmp);
        p.floor = own.get(((own.len().max(1) - 1) as f64 * FLOOR_AT).round() as usize).copied().unwrap_or(1.0) - SLACK;
        p
    }).collect()
}

/// Every piece of the footage, with its nearest kind, and that kind when it's near enough
pub fn pieces(footage: &[Rows], protos: &[Proto]) -> Vec<Piece> {
    let mut out = Vec::new();
    for (f, rows) in footage.iter().enumerate() {
        for (s, shot) in rows.shots.iter().enumerate() {
            let (v, m) = (vector(rows, s), measures(shot));
            // raw footage is one picture: a layered kind is made from two pieces, never matched by one
            let best = protos.iter().filter(|p| !p.composite()).map(|p| (p.kind, near(p, v, &m), p.floor)).max_by(|a, b| a.1.total_cmp(&b.1));
            let (nearest, fit) = best.map_or((0, f32::NEG_INFINITY), |(k, n, floor)| (k, n - floor));
            out.push(Piece { file: f, start: shot.start, end: shot.end, words: shot.words.clone(), faces: shot.faces, kind: (fit >= 0.0).then_some(nearest), fit, nearest });
        }
    }
    out
}

/// What the footage lacks: each kind most references have that no piece is (a composite is made from pieces, so
/// it's never lacked)
pub fn lacks(protos: &[Proto], pieces: &[Piece]) -> Vec<String> {
    let common = |p: &&Proto| p.refs as f64 >= p.of as f64 * COMMON;
    let unlayered = protos.iter().filter(common).filter(|p| !p.composite() && !pieces.iter().any(|x| x.kind == Some(p.kind)));
    unlayered.map(|p| {
        let close = pieces.iter().filter(|x| x.nearest == p.kind).max_by(|a, b| a.fit.total_cmp(&b.fit));
        let hint = match close {
            Some(c) => format!(" (nearest: footage {} at {:.1} s)", c.file, c.start),
            None => String::new(),
        };
        format!("no {}, which {} of the {} references have{hint}", p.name, p.refs, p.of)
    }).collect()
}

/// The sections the references have (a slot no reference has is left out)
fn found(st: &Structure) -> Vec<usize> {
    st.slots.iter().enumerate().filter_map(|(s, x)| (!x.refs.is_empty()).then_some(s)).collect()
}

/// A section's length in a cut of `target`: its share of the references' median lengths
fn share(st: &Structure, live: &[usize], s: usize, target: f64) -> f64 {
    st.slots[s].seconds[1] / live.iter().map(|&x| st.slots[x].seconds[1]).sum::<f64>().max(1e-6) * target
}

/// The open, the close and the middle among the sections the references have (the middle is the open or the close
/// when there's nothing between)
fn ends(st: &Structure, live: &[usize]) -> (Option<usize>, Option<usize>, usize) {
    let place = |s: usize, at: &str| st.slots[s].at.as_deref() == Some(at);
    let open = live.iter().copied().find(|&s| place(s, "open"));
    let close = live.iter().copied().rfind(|&s| place(s, "close"));
    let middle = live.iter().copied().find(|&s| Some(s) != open && Some(s) != close).or(open).or(close).unwrap_or(0);
    (open, close, middle)
}

/// How alike two lines' words are: the words they share over the words either has
fn alike(a: &str, b: &str) -> f64 {
    let w = |s: &str| s.to_lowercase().split(|c: char| !c.is_alphanumeric()).filter(|x| !x.is_empty()).map(str::to_string).collect::<std::collections::BTreeSet<_>>();
    let (a, b) = (w(a), w(b));
    a.intersection(&b).count() as f64 / a.union(&b).count().max(1) as f64
}

/// The voice: each line's last take, in the order said, sorted into the open, the middle and the close (`hits[piece]`
/// says which sections' words a piece says), each section held to its share of `target`
fn voice(pieces: &[Piece], lens: &[f64], hits: &[Vec<bool>], live: &[usize], st: &Structure, target: f64) -> Vec<(usize, usize)> {
    let mut lines: Vec<usize> = (0..pieces.len()).filter(|&i| pieces[i].said() >= LINE).collect();
    lines.sort_by(|&a, &b| (pieces[a].file, pieces[a].start).partial_cmp(&(pieces[b].file, pieces[b].start)).unwrap());
    let later = |i: usize, n: usize| lines[n + 1..].iter().any(|&j| alike(&pieces[i].words, &pieces[j].words) >= TAKE);
    let kept: Vec<usize> = lines.iter().enumerate().filter(|&(n, &i)| !later(i, n)).map(|(_, &i)| i).collect();
    let (open, close, middle) = ends(st, live);
    let open_len = open.map_or(0.0, |s| share(st, live, s, target));
    let (mut out, mut said) = (Vec::new(), 0.0);
    for &i in &kept {
        let slot = match (close, open) {
            (Some(c), _) if hits.get(i).is_some_and(|h| h[c]) => c,
            (_, Some(o)) if said < open_len => o,
            _ => middle,
        };
        if slot != close.unwrap_or(usize::MAX) {
            said += lens[i];
        }
        out.push((i, slot));
    }
    // each section keeps its lines up to its share of the length: the open its first, the close its last, the middle
    // its strongest, then all of them in the order they were said
    let len = |i: usize| lens[i];
    let mut kept = Vec::new();
    for &s in live {
        let mut mine: Vec<usize> = out.iter().filter(|x| x.1 == s).map(|x| x.0).collect();
        if Some(s) == close {
            mine.reverse();
        } else if Some(s) != open {
            mine.sort_by(|&a, &b| pieces[b].fit.total_cmp(&pieces[a].fit));
        }
        let budget = share(st, live, s, target) * OVER;
        let mut took = 0.0;
        for i in mine {
            if took > 0.0 && took + len(i) > budget {
                continue;
            }
            took += len(i);
            kept.push((i, s));
        }
    }
    kept.sort_by_key(|&(i, s)| (live.iter().position(|&x| x == s), pieces[i].file, (pieces[i].start * 1000.0) as i64));
    kept
}

/// The agent's lines (`say`: pieces, in the order it wants them heard), each in the section its place in the cut falls
/// in: the open's share first, the close's share last, the middle between
fn chosen(pieces: &[Piece], lens: &[f64], say: &[usize], live: &[usize], st: &Structure, target: f64) -> Vec<(usize, usize)> {
    let (open, close, middle) = ends(st, live);
    let len = |i: usize| lens[i];
    let whole: f64 = say.iter().map(|&i| len(i)).sum();
    let part = |s: Option<usize>| s.map_or(0.0, |s| share(st, live, s, whole.max(target)));
    let (open_len, close_len) = (part(open), part(close));
    let mut at = 0.0;
    say.iter().filter(|&&i| i < pieces.len()).map(|&i| {
        let slot = match (open, close) {
            (Some(o), _) if at < open_len * 0.8 => o,
            (_, Some(c)) if at + len(i) > whole - close_len * 0.8 => c,
            _ => middle,
        };
        at += len(i);
        (i, slot)
    }).collect()
}

/// The kinds a section is cut through, in the order the references use them, each at its own length
fn rhythm(st: &Structure, s: usize, kinds: &Kinds) -> Vec<(usize, f64)> {
    let seq: Vec<(usize, f64)> = st.slots[s].kinds.iter().flat_map(|k| std::iter::repeat_n((k.kind, k.seconds.clamp(SHORTEST, LONGEST)), (k.count.round() as usize).clamp(1, 4))).collect();
    if seq.is_empty() { kinds.kinds.iter().enumerate().take(1).map(|(k, x)| (k, x.seconds[1].clamp(SHORTEST, LONGEST))).collect() } else { seq }
}

/// The best unused B-roll of a kind (a piece that isn't a line), and its stretch of `len` from its middle
fn broll(pieces: &[Piece], used: &[bool], k: usize, len: f64) -> Option<(usize, f64, f64)> {
    layered::pick(pieces, used, &|p| p.kind == Some(k), len)
}

/// Lines found covering under this share of the length: the footage doesn't carry the voice
const QUIET: f64 = 0.4;

/// The cut planned to the structure. `hits[piece][slot]` says where a piece says a section's words (the vocabulary's
/// patterns, matched by the caller); `seconds`, the cut's length (the references' median when None); `say`, the lines
/// the agent chose, in its order (raea's own pick by the structure when None)
pub fn plan(st: &Structure, kinds: &Kinds, protos: &[Proto], pieces: Vec<Piece>, hits: &[Vec<bool>], seconds: Option<f64>, say: Option<&[usize]>) -> Plan {
    let target = seconds.unwrap_or(st.seconds[1]);
    let live = found(st);
    let lens: Vec<f64> = pieces.iter().map(|p| p.end - p.start).collect();
    let mut lines = match say {
        Some(say) => chosen(&pieces, &lens, say, &live, st, target),
        None => voice(&pieces, &lens, hits, &live, st, target),
    };
    // footage that barely talks is B-roll for a voice still to be written, cut to the sections' lengths
    if say.is_none() && lines.iter().map(|x| lens[x.0]).sum::<f64>() < QUIET * target {
        lines.clear();
    }
    let mut used = vec![false; pieces.len()];
    let mut owed = vec![0.0; protos.len().max(kinds.kinds.len())];
    let (mut vo, mut pic, mut slots) = (Vec::new(), Vec::new(), Vec::new());
    let mut at = 0.0;
    for &s in &live {
        let start = at;
        let wanted = share(st, &live, s, target);
        let mine: Vec<usize> = lines.iter().filter(|x| x.1 == s).map(|x| x.0).collect();
        // the section's span: its lines' length where the footage talks, else its share of the length
        let mut spans: Vec<(usize, f64, f64)> = Vec::new();
        for &i in &mine {
            // a line heard straight after the one before it in the take starts where that one ended, not twice over
            // and runs on from it when it starts where that one ends (within a frame or two)
            let before = vo.last().filter(|u: &&Use| u.file == pieces[i].file && u.from < pieces[i].start).map(|u| u.to);
            let from = before.filter(|&to| to > pieces[i].start).unwrap_or(pieces[i].start);
            let runs_on = before.is_some_and(|to| to >= pieces[i].start - 0.05);
            let len = pieces[i].end - from;
            vo.push(Use { piece: i, file: pieces[i].file, from, to: pieces[i].end, at, slot: s, kind: pieces[i].kind, over: false, why: "a line".into(), ..Default::default() });
            used[i] = true;
            // a line that runs straight on from the one before in the take is one span of the speaker with it
            match spans.last_mut() {
                Some(last) if runs_on && (last.2 - at).abs() < 1e-6 => last.2 = at + len,
                _ => spans.push((i, at - (from - pieces[i].start), at + len)),
            }
            at += len;
        }
        let end = if lines.is_empty() { start + wanted } else { at };
        let mut t = start;
        let seq = rhythm(st, s, kinds);
        let mut n = 0;
        while t < end - 0.05 && !seq.is_empty() {
            let (k, len) = seq[n % seq.len()];
            n += 1;
            let speaking = spans.iter().copied().find(|&(_, a, b)| (a..b).contains(&t));
            let len = len.min(end - t);
            // a layered kind: two pieces at once, the one under and the one over it
            // a kind's shots are layered at its own rate: every so many of its cuts, the next is laid over
            let due = protos.get(k).filter(|p| !p.layer.is_empty()).is_some_and(|p| {
                owed[k] += p.layered;
                owed[k] >= 1.0
            });
            if let Some(p) = protos.get(k).filter(|_| due) {
                owed[k] -= 1.0;
                let plain: Vec<usize> = seq.iter().map(|x| x.0).filter(|&x| protos.get(x).is_some_and(|q| !q.composite())).collect();
                let both = layered::compose(p, &pieces, &mut used, speaking, t, len, s, &plain);
                if let Some(u) = both.first() {
                    t += u.to - u.from;
                    pic.extend(both);
                    continue;
                }
            }
            let speaker = protos.get(k).is_some_and(Proto::speaker);
            let take = if speaker && speaking.is_some() { None } else { broll(&pieces, &used, k, len) };
            // the speaker is shown in sync, so a cut to them ends with their line
            let len = len.min(speaking.map_or(f64::INFINITY, |x| x.2 - t));
            match (take, speaking) {
                (Some((i, from, to)), _) => {
                    used[i] = true;
                    pic.push(Use { piece: i, file: pieces[i].file, from, to, at: t, slot: s, kind: Some(k), over: speaking.is_some(), why: protos.get(k).map_or(String::new(), |p| format!("{} ({:+.2})", p.name, pieces[i].fit)), ..Default::default() });
                    t += to - from;
                }
                (None, Some((i, a, _))) => {
                    let from = pieces[i].start + (t - a);
                    pic.push(Use { piece: i, file: pieces[i].file, from, to: from + len, at: t, slot: s, kind: pieces[i].kind, over: false, why: "the speaker".into(), ..Default::default() });
                    t += len;
                }
                // no talk and none of this kind left: the section's next kind, unless none of them has any
                (None, None) if n > seq.len() * 2 && pic.last().is_none_or(|u: &Use| u.at < start) => break,
                (None, None) => {}
            }
            if n > 400 {
                break;
            }
        }
        if lines.is_empty() {
            at = t;
        }
        slots.push(Slot { name: st.slots[s].name.clone(), at: start, seconds: at - start, wanted });
    }
    Plan { seconds: target, lacks: lacks(protos, &pieces), pieces, slots, voice: vo, picture: pic }
}

#[cfg(test)]
mod tests {
    use super::*;
    use raea_sys_videoform::kinds::Kind;
    use raea_sys_videoform::structure::{KindIn, Slot as StSlot};

    fn shot(start: f64, end: f64, framing: &str, faces: usize, words: &str) -> Shot {
        Shot { start, end, framing: framing.into(), faces, words: words.into(), ..Default::default() }
    }

    #[test]
    fn the_voice_is_cut_first_and_the_picture_over_it() {
        // references: talk to camera (x) then a product with no one (y), twice
        let r = Rows { shots: vec![shot(0.0, 3.0, "medium", 1, "so"), shot(3.0, 5.0, "no people", 0, "")], sig: vec![1.0, 0.0, 0.0, 1.0], ..Default::default() };
        let refs = vec![r.clone(), r];
        let kinds = Kinds {
            kinds: vec![
                Kind { name: "talk".into(), shots: vec![(0, 0), (1, 0)], refs: vec![0, 1], seconds: [3.0; 3], ..Default::default() },
                Kind { name: "product".into(), shots: vec![(0, 1), (1, 1)], refs: vec![0, 1], seconds: [2.0; 3], ..Default::default() },
            ],
            of: vec![vec![Some(0), Some(1)]; 2],
        };
        let p = protos(&refs, &kinds);
        // footage: a line said twice (the second take kept), another line, and a product shot with no one
        let f = Rows {
            shots: vec![shot(0.0, 2.0, "medium", 1, "this oil is great"), shot(2.0, 4.0, "medium", 1, "this oil is so great"), shot(4.0, 6.0, "medium", 1, "my hair loves it"), shot(6.0, 9.0, "no people", 0, "")],
            sig: vec![1.0, 0.0, 1.0, 0.0, 0.99, 0.1, 0.0, 1.0],
            ..Default::default()
        };
        let ki = |kind: usize, name: &str, seconds: f64| KindIn { kind, name: name.into(), refs: 2, count: 1.0, seconds, at: 0.0 };
        let st = Structure { seconds: [5.0; 3], shots: [2.0; 3], slots: vec![StSlot { name: "the pitch".into(), refs: vec![0, 1], seconds: [5.0; 3], kinds: vec![ki(0, "talk", 1.5), ki(1, "product", 1.0)], ..Default::default() }] };
        let all = pieces(&[f], &p);
        assert_eq!(all.iter().map(|x| x.kind).collect::<Vec<_>>(), vec![Some(0), Some(0), Some(0), Some(1)]);
        let plan = plan(&st, &kinds, &p, all, &vec![vec![false]; 4], None, None);
        assert!(plan.lacks.is_empty());
        // the voice: the second take of the first line, then the second line
        assert_eq!(plan.voice.iter().map(|u| (u.from, u.to, u.at)).collect::<Vec<_>>(), vec![(2.0, 4.0, 0.0), (4.0, 6.0, 2.0)]);
        // the picture: the speaker for 1.5 s, the product for 1 s over the voice, then the speaker again, at the next line
        let pic: Vec<(f64, f64, bool)> = plan.picture.iter().map(|u| (u.at, u.to - u.from, u.over)).collect();
        assert_eq!(pic[..3], [(0.0, 1.5, false), (1.5, 1.0, true), (2.5, 1.5, false)]);
    }
}
