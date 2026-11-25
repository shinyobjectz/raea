//! A cut's laid-over pictures from the brand's kit (docs/videoform.feature VF9b): each card, backdrop and inset shows
//! the kit's asset that fits what's said over it (a review over someone's word on the product, the product over its
//! name), the least shown first. A card the kit can't fill keeps its still of the footage, and the plan says what the
//! kit lacks, as the footage's shot list does.

use crate::Plan;
use raea_sys_videoform_kit::{Kit, wants};
use std::path::Path;

/// The forms the kit fills
const FILLS: [&str; 3] = ["card", "backdrop", "inset"];

/// The words said while `t` plays in the cut
fn said_at(p: &Plan, t: f64) -> String {
    p.voice.iter().filter(|u| u.at <= t + 1e-6 && t < u.at + (u.to - u.from)).map(|u| p.pieces[u.piece].words.trim()).collect::<Vec<_>>().join(" ")
}

/// Each laid-over picture's asset from `kit` (in `dir`, the brand's fingerprint folder), named `brand`; with no kit, the
/// plan says so once
pub fn dress(p: &mut Plan, kit: Option<(&Kit, &Path, &str)>) {
    let mut shown = vec![0usize; kit.map_or(0, |k| k.0.assets.len())];
    let mut short: Vec<String> = Vec::new();
    for n in 0..p.picture.len() {
        let form = p.picture[n].layer.clone();
        if !FILLS.contains(&form.as_str()) {
            continue;
        }
        let said = said_at(p, p.picture[n].at);
        let Some((kit, dir, brand)) = kit else {
            if form != "inset" && short.is_empty() {
                short.push("pictures for the cards: no brand kit given (--brand B), so each card shows a still of the footage".into());
            }
            continue;
        };
        match kit.choose(&shown, &form, &said, &p.picture[n].shows) {
            Some(i) => {
                shown[i] += 1;
                let a = &kit.assets[i];
                let u = &mut p.picture[n];
                u.asset = dir.join("kit").join(&a.file).to_string_lossy().into_owned();
                u.why = format!("{}, showing {brand}'s {} {}", u.why, a.kind, a.name);
            }
            // an inset with no clip in the kit is the footage's own moving picture, as planned
            None if form == "inset" => {}
            None => {
                let mut want: Vec<&str> = p.picture[n].shows.iter().map(String::as_str).collect();
                want.extend(wants(&form, &said).iter().filter(|k| !want.contains(k)).collect::<Vec<_>>());
                let line = format!("a {form}'s picture: {brand}'s kit has no {} (raea videoform kit {brand} FILE... adds them)", want.join(" or "));
                if !short.contains(&line) {
                    short.push(line);
                }
            }
        }
    }
    p.lacks.extend(short);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Piece, Use};
    use raea_sys_videoform_kit::Asset;

    #[test]
    fn a_card_over_a_review_shows_the_brands_review() {
        let line = |words: &str| Piece { words: words.into(), ..Default::default() };
        let mut p = Plan {
            pieces: vec![line("everyone says it actually works, five stars"), line("")],
            voice: vec![Use { piece: 0, from: 0.0, to: 4.0, at: 0.0, ..Default::default() }],
            picture: vec![Use { piece: 1, at: 1.0, layer: "card".into(), why: "a card".into(), ..Default::default() }, Use { piece: 1, at: 2.0, layer: "card".into(), ..Default::default() }],
            ..Default::default()
        };
        let a = |kind: &str, file: &str| Asset { kind: kind.into(), file: file.into(), name: file.into(), ..Default::default() };
        let kit = Kit { assets: vec![a("product", "oil.webp"), a("testimonial", "cody.png")] };
        let mut bare = p.clone();
        dress(&mut p, Some((&kit, Path::new("/fp"), "Thicc")));
        assert_eq!(p.picture[0].asset, "/fp/kit/cody.png");
        assert_eq!(p.picture[0].why, "a card, showing Thicc's testimonial cody.png");
        assert_eq!(p.picture[1].asset, "/fp/kit/oil.webp", "the review shown once, then the next best");
        dress(&mut bare, None);
        assert!(bare.picture.iter().all(|u| u.asset.is_empty()) && bare.lacks.len() == 1);
    }
}
