//! A layered shot in the cut (docs/videoform.feature VF8e, VF9), at the rate the references layer that kind of shot.
//! Raw footage is one picture at a time, so a kind the references layer (the speaker cut out over B-roll, a picture inset over the speaker, a card laid over, a plain
//! backdrop) is made from two of the footage's pieces at once: the one under, and the one over it in its form and place.
//! The plan says which and where; the edit draws them.

use crate::{LINE, Piece, Proto, Use};

/// The box nearest all the others (the sum of its corners' distances to theirs least), a real one; zeros for none
pub(crate) fn typical(boxes: &[[f64; 4]]) -> [f64; 4] {
    let mut best = ([0.0; 4], f64::INFINITY);
    for a in boxes {
        let mut far = 0.0;
        for b in boxes {
            for i in 0..4 {
                far += (a[i] - b[i]).abs();
            }
        }
        if far < best.1 {
            best = (*a, far);
        }
    }
    best.0
}

/// The best unused B-roll (a piece that isn't a line) that `fits`, and its stretch of `len` from its middle
pub(crate) fn pick(pieces: &[Piece], used: &[bool], fits: &dyn Fn(&Piece) -> bool, len: f64) -> Option<(usize, f64, f64)> {
    let i = (0..pieces.len()).filter(|&i| !used[i] && pieces[i].said() < LINE && fits(&pieces[i])).max_by(|&a, &b| pieces[a].fit.total_cmp(&pieces[b].fit))?;
    let (s, e) = (pieces[i].start, pieces[i].end);
    let l = len.min(e - s);
    Some((i, (s + e - l) / 2.0, (s + e + l) / 2.0))
}

/// B-roll for a layer: of the section's own kinds first, then of any kind the footage matched, then any
fn broll(pieces: &[Piece], used: &[bool], prefer: &[usize], len: f64) -> Option<(usize, f64, f64)> {
    pick(pieces, used, &|p| p.kind.is_some_and(|k| prefer.contains(&k)), len).or_else(|| pick(pieces, used, &|p| p.kind.is_some(), len)).or_else(|| pick(pieces, used, &|_| true, len))
}

/// One stretch of a layered kind at `t`, for up to `len`: the picture under first, then the one over it. `speaking` is
/// the line heard then (its piece, and the span it plays in the cut); `prefer`, the section's unlayered kinds. Empty
/// when the footage can't make it (no B-roll left, or a cut-out with no one speaking).
#[allow(clippy::too_many_arguments)]
pub(crate) fn compose(p: &Proto, pieces: &[Piece], used: &mut [bool], speaking: Option<(usize, f64, f64)>, t: f64, len: f64, slot: usize, prefer: &[usize]) -> Vec<Use> {
    let len = len.min(speaking.map_or(f64::INFINITY, |x| x.2 - t));
    let speaker = |len: f64| speaking.map(|(i, a, _)| {
        let from = pieces[i].start + (t - a);
        (i, from, from + len)
    });
    let at = |(i, from, to): (usize, f64, f64), over: bool, layer: &str| Use {
        piece: i,
        file: pieces[i].file,
        from,
        to,
        at: t,
        slot,
        kind: pieces[i].kind,
        over,
        why: if layer.is_empty() { "under the layer".into() } else { format!("{} ({layer})", p.name) },
        layer: layer.into(),
        place: (!layer.is_empty()).then_some(p.place),
        asset: String::new(),
        shows: if layer.is_empty() { Vec::new() } else { p.shows.clone() },
    };
    let talking = speaking.is_some();
    let (under, top) = match p.layer.as_str() {
        // the speaker, cut out, over B-roll
        "cutout" => {
            let Some(b) = broll(pieces, used, prefer, len) else { return Vec::new() };
            let Some(s) = speaker(b.2 - b.1) else { return Vec::new() };
            (at(b, talking, ""), at(s, false, "cutout"))
        }
        // B-roll laid over the speaker (or over other B-roll), in its box
        _ => {
            let Some(b) = broll(pieces, used, prefer, len) else { return Vec::new() };
            used[b.0] = true;
            let under = match speaker(b.2 - b.1) {
                Some(s) => at(s, false, ""),
                None => match broll(pieces, used, prefer, b.2 - b.1) {
                    Some(c) => at(c, false, ""),
                    None => {
                        used[b.0] = false;
                        return Vec::new();
                    }
                },
            };
            (under, at(b, talking, &p.layer))
        }
    };
    for u in [&under, &top] {
        if pieces[u.piece].said() < LINE {
            used[u.piece] = true;
        }
    }
    vec![under, top]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_cut_out_kind_is_the_speaker_over_b_roll_and_an_inset_is_b_roll_over_the_speaker() {
        let piece = |file: usize, start: f64, end: f64, words: &str, kind: Option<usize>| Piece { file, start, end, words: words.into(), kind, ..Default::default() };
        let mut pieces = vec![piece(0, 0.0, 4.0, "this oil saved my hair", Some(0)), piece(1, 0.0, 3.0, "", Some(1)), piece(2, 0.0, 3.0, "", Some(1))];
        pieces[1].fit = 0.2;
        let cut = Proto { kind: 2, name: "hair, cut out".into(), layer: "cutout".into(), place: [0.0, 0.5, 0.4, 0.3], ..Default::default() };
        let mut used = vec![true, false, false];
        let both = compose(&cut, &pieces, &mut used, Some((0, 0.0, 4.0)), 1.0, 2.0, 0, &[1]);
        assert_eq!(both.iter().map(|u| (u.piece, u.layer.as_str())).collect::<Vec<_>>(), [(1, ""), (0, "cutout")]);
        assert_eq!((both[1].from, both[1].to, both[1].place), (1.0, 3.0, Some([0.0, 0.5, 0.4, 0.3])), "the speaker in sync, where the references put them");
        assert!(both[0].over, "the B-roll under plays under the voice");
        let inset = Proto { layer: "inset".into(), ..cut };
        let both = compose(&inset, &pieces, &mut used, Some((0, 0.0, 4.0)), 1.0, 1.0, 0, &[1]);
        assert_eq!(both.iter().map(|u| (u.piece, u.layer.as_str())).collect::<Vec<_>>(), [(0, ""), (2, "inset")]);
        assert!(compose(&inset, &pieces, &mut used, None, 3.0, 1.0, 0, &[1]).is_empty(), "no B-roll left");
    }
}
