//! A brand's kit (docs/videoform.feature VF9b): the brand's own stills and graphic clips (its logo, product photos,
//! customers' reviews, screenshots), kept in its fingerprint (brand-SLUG/kit/) by content hash so they reach a team as
//! the fingerprint does. Each is read once for what it is, its words and colours; a card or backdrop in a cut made to a
//! videoform shows the one that fits what's said over it, never a still of the footage when the kit has one.

use serde::{Deserialize, Serialize};
use std::path::Path;

/// Where a fingerprint keeps its kit's index; the assets are beside it, HASH.ext
pub const KIT: &str = "kit/kit.json";

/// What an asset can be, in the order they're said
pub const KINDS: [&str; 7] = ["logo", "product", "person", "testimonial", "screenshot", "graphic", "clip"];

/// What raea-perceive kit read of one asset: its size, how much is see-through, the faces and person in it, its words
/// and where, whether Vision takes it for a screenshot, its commonest colours (#rrggbb, share), and a clip's length
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Seen {
    pub w: u32,
    pub h: u32,
    /// the share of its pixels that are see-through
    pub clear: f64,
    pub faces: Vec<[f64; 4]>,
    /// the share of it that's a person
    pub person: f64,
    pub text: Vec<(String, [f64; 4])>,
    pub screen: bool,
    pub colours: Vec<(String, f64)>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub seconds: Option<f64>,
}

/// One asset of the kit
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Asset {
    /// its content hash, and its file in the kit's folder (HASH.ext)
    pub hash: String,
    pub file: String,
    /// its name and where it was added from
    pub name: String,
    pub from: String,
    /// one of KINDS
    pub kind: String,
    /// the words in it, as read
    pub words: String,
    pub colours: Vec<String>,
    pub w: u32,
    pub h: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub seconds: Option<f64>,
    pub added: String,
    /// what was read of it, so a later raea's tag can be put on it again without reading it again
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub seen: Option<Seen>,
}

/// The kit: its assets, in the order they were added
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Kit {
    pub assets: Vec<Asset>,
}

/// Words that say a picture or a line is someone's word on the product
const REVIEW: [&str; 16] = ["★", "⭐", "*****", "actually works", "review", "verified", "stars", "5/5", "recommend", "obsessed", "results", "customer", "testimonial", "\u{201c}", "rated", "love it"];
/// See-through over this share: a logo or a graphic cut to its shape
const CLEAR: f64 = 0.15;
/// Its three commonest colours over this share: flat artwork, not a photo
const FLAT: f64 = 0.9;
/// A line of words this share of its width or wider: the words are what it shows (a review, a page), not words printed
/// on the product (a label's lines are narrow)
const WIDE: f64 = 0.5;
/// A logo is no wider or taller than this many times the other way (a button or a banner is)
const SQUARE: f64 = 3.0;
/// Someone's word on the product has a line of this many words or more (a rating's "9625 Reviews" doesn't)
const SENTENCE: usize = 5;
/// More lines than this: a whole page, even with reviews on it
const PAGE: usize = 12;
/// A person over this share of it, or a face: a picture of someone
const PERSON: f64 = 0.12;

fn reviewed(s: &str) -> bool {
    let s = s.to_lowercase();
    REVIEW.iter().any(|w| s.contains(w))
}

/// What an asset is, from what was seen in it and its file's name (a name that says "logo" wins; one that says
/// "review" or "testimonial" when its words don't say otherwise)
pub fn tag(s: &Seen, name: &str) -> &'static str {
    let n = name.to_lowercase();
    let words: String = s.text.iter().map(|t| t.0.as_str()).collect::<Vec<_>>().join(" ");
    if s.seconds.is_some() {
        return "clip";
    }
    if n.contains("logo") {
        return "logo";
    }
    let wordy = s.text.iter().any(|t| t.1[2] >= WIDE);
    let flat = s.colours.iter().take(3).map(|c| c.1).sum::<f64>() >= FLAT;
    let someone = !s.faces.is_empty() || s.person > PERSON;
    // a sentence: enough words, some of them lower case (a product's title is capitalised)
    let said = s.text.iter().any(|t| t.0.split_whitespace().count() >= SENTENCE && t.0.split_whitespace().filter(|w| w.chars().next().is_some_and(char::is_lowercase)).count() >= 2);
    // a name that says it's a review is one, unless its words say otherwise (a rating, a title)
    if (n.contains("review") || n.contains("testimonial")) && (said || s.text.is_empty()) {
        return "testimonial";
    }
    if wordy && reviewed(&words) && said && s.text.len() <= PAGE {
        return "testimonial";
    }
    let square = (s.w.max(1) as f64 / s.h.max(1) as f64).max(s.h.max(1) as f64 / s.w.max(1) as f64) <= SQUARE;
    if (s.clear > CLEAR || flat) && !someone && !wordy {
        return if words.split_whitespace().count() <= 4 && square { "logo" } else { "graphic" };
    }
    if s.text.len() > PAGE {
        return "screenshot";
    }
    if wordy {
        return if s.screen || n.contains("screenshot") || n.contains("screen shot") || !flat { "screenshot" } else { "graphic" };
    }
    if someone { "person" } else { "product" }
}

/// The kinds a layer wants, best first: a card over a line that's someone's word on the product wants their review; a
/// card otherwise the product; a plain backdrop the product on it, else the logo; an inset a moving clip
pub fn wants(form: &str, said: &str) -> &'static [&'static str] {
    match form {
        "card" if reviewed(said) => &["testimonial", "screenshot", "person", "product"],
        "card" => &["product", "testimonial", "person", "screenshot", "graphic"],
        "backdrop" => &["product", "logo"],
        "inset" => &["clip"],
        _ => &[],
    }
}

/// The words two texts share, four letters or more
fn shared(a: &str, b: &str) -> usize {
    let w = |s: &str| s.to_lowercase().split(|c: char| !c.is_alphanumeric()).filter(|w| w.len() >= 4).map(str::to_string).collect::<std::collections::BTreeSet<_>>();
    w(a).intersection(&w(b)).count()
}

impl Asset {
    /// An asset as it was read: `file` its name in the kit's folder, `from` where it was added from
    pub fn read(s: Seen, hash: String, file: String, from: &str, added: String) -> Asset {
        let name = from.rsplit(['/', '\\']).next().unwrap_or(from).to_string();
        Asset {
            kind: tag(&s, &name).into(),
            words: s.text.iter().map(|t| t.0.as_str()).collect::<Vec<_>>().join(" "),
            colours: s.colours.iter().map(|c| c.0.clone()).collect(),
            w: s.w,
            h: s.h,
            seconds: s.seconds,
            hash,
            file,
            from: from.into(),
            name,
            added,
            seen: Some(s),
        }
    }
}

impl Kit {
    /// The kit in a fingerprint's folder (empty when it has none)
    pub fn load(dir: &Path) -> Result<Kit, String> {
        let p = dir.join(KIT);
        match std::fs::read(&p) {
            Ok(b) => serde_json::from_slice(&b).map_err(|e| format!("{}: {e}", p.display())),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Kit::default()),
            Err(e) => Err(format!("{}: {e}", p.display())),
        }
    }

    pub fn save(&self, dir: &Path) -> Result<(), String> {
        let p = dir.join(KIT);
        std::fs::create_dir_all(p.parent().unwrap()).map_err(|e| e.to_string())?;
        std::fs::write(&p, serde_json::to_vec_pretty(self).unwrap()).map_err(|e| format!("{}: {e}", p.display()))
    }

    /// Each asset tagged again from what was read of it (this raea's tags); whether any changed
    pub fn retag(&mut self) -> bool {
        let mut changed = false;
        for a in &mut self.assets {
            if let Some(k) = a.seen.as_ref().map(|s| tag(s, &a.name))
                && a.kind != k
            {
                a.kind = k.into();
                changed = true;
            }
        }
        changed
    }

    /// The asset a layer of `form` shows over `said`: the least shown so far (`shown`, a count each), then the kind it
    /// wants most, then the most words shared with what's said; None when the kit has none of the kinds it wants.
    /// `learned` is what the videoform's references showed in such a layer, most first (VF9d): it comes before the
    /// guess `wants` makes, and a clip counts as what it shows (a screen recording is a screenshot that moves)
    pub fn choose(&self, shown: &[usize], form: &str, said: &str, learned: &[String]) -> Option<usize> {
        let mut want: Vec<&str> = learned.iter().map(String::as_str).collect();
        want.extend(wants(form, said).iter().filter(|k| !learned.iter().any(|l| l == *k)));
        let rank = |a: &Asset| {
            let content = a.seen.as_ref().filter(|_| a.kind == "clip").map(|s| tag(&Seen { seconds: None, ..s.clone() }, &a.name));
            let moving = (form == "inset") == (a.kind == "clip");
            want.iter().position(|k| *k == a.kind || (moving && content == Some(*k)))
        };
        (0..self.assets.len())
            .filter_map(|i| rank(&self.assets[i]).map(|r| (i, r)))
            .min_by_key(|&(i, r)| (shown.get(i).copied().unwrap_or(0), r, std::cmp::Reverse(shared(said, &format!("{} {}", self.assets[i].words, self.assets[i].name)))))
            .map(|x| x.0)
    }

    /// How many of each kind it has, as "3 products, 2 testimonials"
    pub fn said(&self) -> String {
        let parts: Vec<String> = KINDS
            .iter()
            .filter_map(|k| {
                let n = self.assets.iter().filter(|a| a.kind == *k).count();
                (n > 0).then(|| match (n, *k) {
                    (1, k) => format!("1 {k}"),
                    (n, "person") => format!("{n} people"),
                    (n, k) => format!("{n} {k}s"),
                })
            })
            .collect();
        if parts.is_empty() { "nothing yet".into() } else { parts.join(", ") }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn seen(text: &[&str], faces: usize, colours: &[f64]) -> Seen {
        Seen {
            w: 1000,
            h: 1000,
            text: text.iter().map(|t| (t.to_string(), [0.1, 0.1, if t.len() > 30 { 0.8 } else { 0.2 }, 0.05])).collect(),
            faces: vec![[0.3, 0.2, 0.2, 0.2]; faces],
            colours: colours.iter().enumerate().map(|(i, s)| (format!("#00000{i}"), *s)).collect(),
            ..Default::default()
        }
    }

    #[test]
    fn an_asset_is_known_by_what_is_in_it() {
        let photo = [0.3, 0.2, 0.1, 0.1];
        assert_eq!(tag(&seen(&[], 0, &photo), "HairOil5pack_1512x.webp"), "product");
        assert_eq!(tag(&seen(&[], 1, &photo), "customer.jpg"), "person");
        assert_eq!(tag(&seen(&["THICC"], 0, &[0.8, 0.15]), "mark.png"), "logo");
        let button = Seen { w: 1000, h: 190, ..seen(&["Get My Starter Kit"], 0, &[0.8, 0.15]) };
        assert_eq!(tag(&button, "cta.png"), "graphic", "a button is no logo");
        let rating = ["Retail $70 310 Shake Starter Kit", "★★★★★ 9625 Reviews $9"];
        assert_ne!(tag(&seen(&rating, 0, &photo), "kit-reviews.png"), "testimonial", "a rating isn't someone's word, whatever it's called");
        assert_eq!(tag(&seen(&["★★★★★ Verified buyer", "My hair has never been this thick, I'm obsessed with it"], 0, &photo), "IMG_1.png"), "testimonial");
        let label = ["THICC", "HAIR OIL", "Natural & Organic", "Follicle Stimulating Oil", "2 fl oz"];
        assert_eq!(tag(&seen(&label, 0, &photo), "HairOil5pack_1512x.webp"), "product", "words printed on it are narrow");
        let page = ["Bundle & Save! Treat yourself to instant savings", "Add to cart"];
        assert_eq!(tag(&seen(&page, 0, &photo), "Screenshot 2024-09-09.png"), "screenshot");
        assert_eq!(tag(&seen(&[], 1, &photo), "Screenshot 2024-09-10.png"), "person", "what's in it, not what it's called");
        assert_eq!(tag(&Seen { seconds: Some(4.0), ..Default::default() }, "spin.mov"), "clip");
    }

    #[test]
    fn a_card_shows_the_asset_that_fits_what_is_said() {
        let a = |kind: &str, words: &str| Asset { kind: kind.into(), words: words.into(), ..Default::default() };
        let kit = Kit { assets: vec![a("product", "hair oil rosemary"), a("testimonial", "my hair grew"), a("product", "hair kit gift set"), a("logo", "")] };
        assert_eq!(kit.choose(&[0; 4], "card", "people say they're obsessed with the results", &[]), Some(1), "a review over a review");
        assert_eq!(kit.choose(&[0; 4], "card", "the gift set comes with everything", &[]), Some(2), "the product the line names");
        assert_eq!(kit.choose(&[0, 0, 1, 0], "card", "the gift set comes with everything", &[]), Some(0), "one not shown yet first");
        assert_eq!(kit.choose(&[0; 4], "backdrop", "", &[]), Some(0));
        assert_eq!(kit.choose(&[0; 4], "inset", "", &[]), None, "no clip in the kit");
        // what the references showed there comes first (VF9d): a review card in the videoform, over any line
        assert_eq!(kit.choose(&[0; 4], "card", "the gift set comes with everything", &["testimonial".into()]), Some(1));
        assert_eq!(kit.said(), "1 logo, 2 products, 1 testimonial");
    }
}
