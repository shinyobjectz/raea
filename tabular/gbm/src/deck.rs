//! K3 on decks and dashboards (the roadmap K3): the slides an agent builds for explainers and ads, in four shapes,
//! each a grid of fixed-size components holding up to three kinds of text (a: the big line, b: the small line or body,
//! c: a tag or pill): KPI tiles (a number, its label, a change pill), pills (tags in fixed-width pills), dashboard
//! cards (a title over a two-line note, fixed height), and an agenda (numbered steps with an owner, like the founder's
//! 2026-10-15 screenshot). Each content is laid out VARIANTS ways (columns, component size, the three text sizes,
//! letter spacing, padding and one of four fonts), labelled by `raea lint --layout`, and ranked by rank.rs's ListNet
//! the same way the card rows were: the best layout is the clean one with the largest type.

use crate::rank::{lint, score_ranking};
use std::path::PathBuf;

struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        self.0 >> 33
    }
    fn pick<'a, T>(&mut self, v: &'a [T]) -> &'a T {
        &v[(self.next() as usize) % v.len()]
    }
}

const SHAPES: [&str; 4] = ["kpi", "pills", "cards", "agenda"];
/// (CSS family, average glyph width over the font size: the plain estimate's width per character)
const FONTS: [(&str, f64); 4] = [("Inter,Helvetica,sans-serif", 0.55), ("Avenir Next,sans-serif", 0.58), ("Georgia,serif", 0.52), ("Menlo,monospace", 0.6)];

const KPI_A: [&str; 10] = ["$48.2k", "12,480", "3.4x", "92%", "$1.27M", "4.8", "18 min", "640k", "$0.42", "2,104,377"];
const KPI_B: [&str; 10] = ["MONTHLY RECURRING REVENUE", "VIEWS", "RETURN ON AD SPEND", "COMPLETION RATE", "PIPELINE", "AVG RATING",
    "WATCH TIME", "IMPRESSIONS", "COST PER CLICK", "TOTAL REACH THIS QUARTER"];
const KPI_C: [&str; 6] = ["+12.4% vs last month", "-3%", "+0.8 pts", "flat", "+128% YoY", "best week yet"];
const PILLS: [&str; 12] = ["Brand safe", "Creator approved", "Needs legal review", "Hook A", "UGC", "Paid social", "Organic only",
    "Whitelisted", "Spark Ads ready", "Under 30 s", "Captions burned in", "Music licensed"];
const CARD_A: [&str; 8] = ["Hook retention", "Spend", "Top creative", "What changed", "Audience", "Next test", "Blockers", "Wins this week"];
const CARD_B: [&str; 8] = ["Average watch time rose after the hook moved to the first second",
    "On pace", "The talking-head cut beat the b-roll cut on every placement we ran it in",
    "Captions now sit above the platform's UI band on all three networks", "25-34, mostly iPhone",
    "Try the problem-first hook against the product-first one", "Waiting on the client's approval of the second script",
    "Two creators renewed"];
const AGENDA_A: [&str; 10] = ["Write the brief", "Find a creator", "Film", "Check the footage", "Edit the cuts", "Client review",
    "Approve", "Post", "Measure the results", "Review the rough cut"];
const AGENDA_C: [&str; 8] = ["STRATEGY", "CREATOR", "PRODUCTION", "QA", "EDITORS", "CLIENT", "SOCIAL", "FULFILLMENT"];

struct Layout {
    cols: usize,
    w: f64,
    h: f64,
    a: f64,
    b: f64,
    c: f64,
    spacing: f64,
    pad: f64,
    font: usize,
}

/// One component's texts (a, b, c; empty when the shape has none).
type Item = (&'static str, &'static str, &'static str);

fn content(rng: &mut Rng, shape: &str) -> Vec<Item> {
    let n = match shape { "pills" => 4 + (rng.next() % 6) as usize, "kpi" => 2 + (rng.next() % 5) as usize, _ => 3 + (rng.next() % 4) as usize };
    (0..n).map(|_| match shape {
        "kpi" => (*rng.pick(&KPI_A), *rng.pick(&KPI_B), *rng.pick(&KPI_C)),
        "pills" => (*rng.pick(&PILLS), "", ""),
        "cards" => (*rng.pick(&CARD_A), *rng.pick(&CARD_B), ""),
        _ => (*rng.pick(&AGENDA_A), "", *rng.pick(&AGENDA_C)),
    }).collect()
}

fn layout(rng: &mut Rng, shape: &str, n: usize) -> Layout {
    let cols = (*rng.pick(&[2usize, 3, 4, 6])).min(n);
    match shape {
        "kpi" => Layout { cols, w: *rng.pick(&[180.0, 220.0, 260.0, 320.0, 400.0]), h: *rng.pick(&[150.0, 180.0, 220.0]), a: *rng.pick(&[40.0, 52.0, 64.0, 80.0]),
            b: *rng.pick(&[12.0, 14.0, 16.0]), c: *rng.pick(&[12.0, 14.0, 16.0]), spacing: *rng.pick(&[0.0, 1.0, 2.0]), pad: *rng.pick(&[16.0, 24.0, 32.0]), font: (rng.next() % 4) as usize },
        "pills" => Layout { cols, w: *rng.pick(&[120.0, 150.0, 180.0, 240.0]), h: *rng.pick(&[44.0, 56.0]), a: *rng.pick(&[16.0, 18.0, 22.0, 26.0]),
            b: 0.0, c: 0.0, spacing: *rng.pick(&[0.0, 1.0, 2.0]), pad: *rng.pick(&[12.0, 18.0, 24.0]), font: (rng.next() % 4) as usize },
        "cards" => Layout { cols, w: *rng.pick(&[220.0, 280.0, 340.0, 420.0]), h: *rng.pick(&[130.0, 160.0, 200.0]), a: *rng.pick(&[18.0, 22.0, 28.0]),
            b: *rng.pick(&[14.0, 16.0, 18.0, 20.0]), c: 0.0, spacing: *rng.pick(&[0.0, 0.5, 1.0]), pad: *rng.pick(&[16.0, 20.0, 28.0]), font: (rng.next() % 4) as usize },
        _ => Layout { cols, w: *rng.pick(&[140.0, 180.0, 220.0, 280.0]), h: *rng.pick(&[110.0, 140.0]), a: *rng.pick(&[16.0, 20.0, 24.0]),
            b: 0.0, c: *rng.pick(&[11.0, 13.0, 15.0]), spacing: *rng.pick(&[0.0, 1.0, 2.0, 3.0]), pad: *rng.pick(&[14.0, 20.0, 28.0]), font: (rng.next() % 4) as usize },
    }
}

fn html(shape: &str, items: &[Item], l: &Layout) -> String {
    let comps: String = items.iter().enumerate().map(|(i, (a, b, c))| {
        let num = if shape == "agenda" { format!(r#"<div class="n">{}</div>"#, i + 1) } else { String::new() };
        let b = if b.is_empty() { String::new() } else { format!(r#"<div class="b">{b}</div>"#) };
        let c = if c.is_empty() { String::new() } else { format!(r#"<div class="c">{c}</div>"#) };
        format!(r#"<div class="k" id="k{i}">{num}<div class="a">{a}</div>{b}{c}</div>"#)
    }).collect();
    let (fam, _) = FONTS[l.font];
    let pill = if shape == "pills" { "border-radius:999px;display:flex;align-items:center;justify-content:center;text-align:center;" } else { "border-radius:10px;" };
    let c_style = if shape == "kpi" { "display:inline-block;margin-top:8px;padding:2px 8px;border-radius:999px;background:#e6f6ee;color:#137a43" } else { "font-family:Menlo,monospace;color:#666;margin-top:6px" };
    format!(r#"<!doctype html><html><head><style>
html,body{{margin:0;width:1920px;height:1080px;background:#f6f6f4;font-family:{fam};color:#111}}
#root{{position:relative;width:1920px;height:1080px;overflow:hidden}}
.grid{{position:absolute;left:60px;top:80px;display:grid;grid-template-columns:repeat({cols},{w}px);gap:20px}}
.k{{width:{w}px;height:{h}px;box-sizing:border-box;padding:{pad}px;background:#fff;border:1px solid #ddd;{pill}overflow:visible;letter-spacing:{sp}px}}
.n{{font-size:{c}px;color:#999;margin-bottom:4px}}
.a{{font-size:{a}px;font-weight:700;line-height:1.2}}
.b{{font-size:{b}px;line-height:1.35;color:#444;margin-top:6px}}
.c{{font-size:{c}px;{c_style}}}
</style></head><body><div id="root" data-composition-id="main" data-start="0" data-duration="1" data-width="1920" data-height="1080">
<div class="grid">{comps}</div></div>
<script>window.__timelines = {{ main: {{ duration: function () {{ return 1; }}, seek: function () {{}}, pause: function () {{}}, time: function () {{ return 0; }} }} }};</script>
</body></html>"#, cols = l.cols, w = l.w, h = l.h, pad = l.pad, sp = l.spacing, a = l.a, b = l.b, c = l.c)
}

/// What a plan knows before rendering: its numbers and each text kind's estimated width against the inner width, and
/// the body's estimated lines against the room left under the big line.
fn features(shape: usize, items: &[Item], l: &Layout) -> Vec<f64> {
    let k = FONTS[l.font].1;
    let inner = l.w - 2.0 * l.pad - 2.0;
    let wid = |s: &str, px: f64, mono: bool| s.len() as f64 * (px * if mono { 0.6 } else { k } + l.spacing);
    let wa = items.iter().map(|i| wid(i.0, l.a, false)).fold(0.0, f64::max);
    let wb = items.iter().map(|i| wid(i.1, l.b, false)).fold(0.0, f64::max);
    let wc = items.iter().map(|i| wid(i.2, l.c, shape == 3) + if shape == 0 { 16.0 } else { 0.0 }).fold(0.0, f64::max);
    let word = items.iter().flat_map(|i| i.0.split(' ')).map(|w| wid(w, l.a, false)).fold(0.0, f64::max);
    let lines_a = (wa / inner).ceil().max(1.0);
    let lines_b = (wb / inner).ceil();
    let used = lines_a * l.a * 1.2 + lines_b * l.b * 1.35 + if l.c > 0.0 { l.c * 1.4 + 8.0 } else { 0.0 } + if shape == 3 { l.c * 1.4 } else { 0.0 };
    let room = l.h - 2.0 * l.pad - 2.0;
    vec![shape as f64, items.len() as f64, l.cols as f64, l.w, l.h, inner, l.a, l.b, l.c, l.spacing, l.pad, l.font as f64, k,
         wa / inner, wb / inner, wc / inner, word / inner, used / room]
}

/// The plain estimate: the tightest of the widest unbreakable word and the tag over the inner width, and the text's
/// height over the room (lower is roomier).
fn estimate(f: &[f64]) -> f64 {
    f[15].max(f[16]).max(f[17])
}

pub fn run(contents: usize, variants: usize, budget: f32) -> Result<serde_json::Value, String> {
    let raea = PathBuf::from(std::env::var("HOME").unwrap_or_default()).join("raea/engine/target/release/raea");
    let dir = std::env::temp_dir().join(format!("raea-gbm-deck-{}", std::process::id()));
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let mut rng = Rng(23);
    let mut rows: Vec<(usize, Vec<f64>, f64)> = Vec::new();
    let mut by_shape = vec![[0usize; 3]; SHAPES.len()];
    for c in 0..contents {
        let s = c % SHAPES.len();
        let items = content(&mut rng, SHAPES[s]);
        for v in 0..variants {
            let l = layout(&mut rng, SHAPES[s], items.len());
            let path = dir.join(format!("c{c}-v{v}.html"));
            std::fs::write(&path, html(SHAPES[s], &items, &l)).map_err(|e| e.to_string())?;
            if let Some(fit) = lint(raea.to_str().unwrap_or("raea"), &path) {
                by_shape[s][fit as usize] += 1;
                let (amin, amax) = match SHAPES[s] { "kpi" => (40.0, 80.0), "pills" => (16.0, 26.0), "cards" => (18.0, 28.0), _ => (16.0, 24.0) };
                let small = if l.b > 0.0 { (l.b - 14.0) / 6.0 } else { (l.c - 11.0) / 5.0 };
                let rel = if fit == 2.0 { 2.0 + (l.a - amin) / (amax - amin) * 3.0 + small.clamp(0.0, 1.0) } else { fit };
                rows.push((c, features(s, &items, &l), rel));
            }
        }
    }
    if std::env::var_os("RAEA_DECK_KEEP").is_none() {
        let _ = std::fs::remove_dir_all(&dir);
    }
    let mut out = score_ranking(&rows, budget, estimate, |f| f[6] * 100.0 + f[7] + f[8])?;
    out["contents"] = serde_json::json!(contents);
    out["by_shape"] = serde_json::json!(SHAPES.iter().zip(&by_shape).map(|(s, n)| (s.to_string(), serde_json::json!({"spill": n[0], "crowded_only": n[1], "clean": n[2]}))).collect::<serde_json::Map<_, _>>());
    Ok(out)
}
