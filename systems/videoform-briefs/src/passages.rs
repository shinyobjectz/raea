//! A document's words as passages (VF8f): a page's lines read by Vision (each with its box and height), a Word
//! document's paragraphs, or plain text, into the passages a brief is made of. A bullet is its own passage; lines that
//! wrap are one; a heading (type larger than the body's, or a short line ending in a colon) isn't a passage but heads
//! those after it, so "NYC" under "Location:" reads "Location: NYC". Two columns of a table side by side are one row,
//! its cells joined: "24/7 Virtual Vet Care — The simplest way to say what Dutch is."

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Passage {
    /// the first 12 hex of the sha256 of its heading and text: the same words have the same id in any document
    pub id: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub heading: String,
    pub text: String,
    /// the page it's on, from 1 (0: a document without pages)
    #[serde(default)]
    pub page: usize,
}

impl Passage {
    pub fn new(heading: &str, text: &str, page: usize) -> Passage {
        let (heading, text) = (heading.trim().trim_end_matches(':').trim().to_string(), text.trim().to_string());
        let id = Sha256::digest(format!("{heading}\n{text}").as_bytes()).iter().take(6).map(|b| format!("{b:02x}")).collect();
        Passage { id, heading, text, page }
    }

    /// How it's read: its heading, then its words
    pub fn said(&self) -> String {
        if self.heading.is_empty() { self.text.clone() } else { format!("{}: {}", self.heading, self.text) }
    }
}

/// A line Vision read on a page: its words, its box ([x, y, w, h] of the page, from the top left) and its height in
/// pixels of a page 1700 wide
#[derive(Clone, Debug, Default, Deserialize)]
pub struct Line {
    pub text: String,
    pub at: [f64; 4],
    pub h: f64,
}

/// Type this much taller than the body's is a heading
const LARGER: f64 = 1.25;
/// A gap between lines under this share of a line's height is a line wrapping, not a new paragraph
const WRAP: f64 = 0.7;
/// Cells further apart than this share of the page's width are a table's columns
const CELL: f64 = 0.04;

fn bullet(t: &str) -> bool {
    let t = t.trim_start();
    t.starts_with(['•', '◦', '○', '▪', '■', '–', '-', '*', '·', '●', '✓', '✔', '→']) || {
        let n: String = t.chars().take_while(|c| c.is_ascii_digit()).collect();
        !n.is_empty() && n.len() <= 2 && t[n.len()..].starts_with(['.', ')'])
    }
}

fn unbullet(t: &str) -> &str {
    let t = t.trim_start();
    if !bullet(t) {
        return t;
    }
    if t.starts_with(char::is_numeric) {
        return t.trim_start_matches(|c: char| c.is_ascii_digit()).trim_start_matches(['.', ')']).trim_start();
    }
    t.trim_start_matches(['•', '◦', '○', '▪', '■', '–', '-', '*', '·', '●', '✓', '✔', '→']).trim_start()
}


/// A short line ending in a colon, or of two to six words set in capitals; never a bullet
fn titled(t: &str) -> bool {
    let w = t.split_whitespace().count();
    !bullet(t) && ((t.ends_with(':') && w <= 8) || ((2..=6).contains(&w) && t.chars().any(char::is_alphabetic) && !t.chars().any(char::is_lowercase)))
}

/// A page's lines into rows: lines whose middles share a height are one row, cells joined left to right
fn rows(lines: &[Line]) -> Vec<(String, [f64; 4], f64)> {
    let mut ls: Vec<&Line> = Vec::new();
    for l in lines {
        if !l.text.trim().is_empty() {
            ls.push(l);
        }
    }
    ls.sort_by(|a, b| (a.at[1] + a.at[3] / 2.0).total_cmp(&(b.at[1] + b.at[3] / 2.0)));
    let mut out: Vec<Vec<&Line>> = Vec::new();
    for l in ls {
        let mid = l.at[1] + l.at[3] / 2.0;
        match out.last_mut() {
            Some(r) if (r[0].at[1] + r[0].at[3] / 2.0 - mid).abs() < r[0].at[3].min(l.at[3]) * 0.5 => r.push(l),
            _ => out.push(vec![l]),
        }
    }
    out.into_iter().map(|mut r| {
        r.sort_by(|a, b| a.at[0].total_cmp(&b.at[0]));
        let mut text = r[0].text.trim().to_string();
        for w in r.windows(2) {
            let gap = w[1].at[0] - (w[0].at[0] + w[0].at[2]);
            text += if gap > CELL { " — " } else { " " };
            text += w[1].text.trim();
        }
        let x0 = r.iter().map(|l| l.at[0]).fold(1.0, f64::min);
        let (y0, y1) = (r.iter().map(|l| l.at[1]).fold(1.0, f64::min), r.iter().map(|l| l.at[1] + l.at[3]).fold(0.0, f64::max));
        let h = r.iter().map(|l| l.h).fold(0.0, f64::max);
        (text, [x0, y0, 1.0 - x0, y1 - y0], h)
    }).collect()
}

/// The body's height: the median line's
fn body(pages: &[Vec<Line>]) -> f64 {
    let mut hs: Vec<f64> = pages.iter().flatten().map(|l| l.h).filter(|h| *h > 0.0).collect();
    hs.sort_by(f64::total_cmp);
    hs.get(hs.len() / 2).copied().unwrap_or(0.0)
}

/// Where passages are gathered: the heading over them, and the one being written
#[derive(Default)]
struct Gather {
    out: Vec<Passage>,
    heading: String,
    open: Option<String>,
    page: usize,
}

impl Gather {
    fn close(&mut self) {
        if let Some(t) = self.open.take().filter(|t| t.chars().any(char::is_alphanumeric)) {
            self.out.push(Passage::new(&self.heading, &t, self.page));
        }
    }

    fn head(&mut self, t: &str) {
        self.close();
        self.heading = t.trim_end_matches(':').to_string();
    }

    /// A row: a new passage, or more of the open one
    fn row(&mut self, t: &str, more: bool) {
        match (&mut self.open, more) {
            (Some(o), true) => {
                o.push(' ');
                o.push_str(t);
            }
            _ => {
                self.close();
                self.open = Some(unbullet(t).to_string());
            }
        }
    }
}

/// A document's pages, as Vision read them, into passages
pub fn from_pages(pages: &[Vec<Line>]) -> Vec<Passage> {
    let body = body(pages);
    let mut g = Gather::default();
    for (n, lines) in pages.iter().enumerate() {
        g.close();
        g.page = n + 1;
        let mut last: Option<[f64; 4]> = None;
        for (t, at, h) in rows(lines) {
            // a page number, alone on its row
            if t.chars().all(|c| c.is_ascii_digit() || c.is_whitespace()) {
                continue;
            }
            // a heading is set larger, short, and starts as a title does (a box's height swells with quotes and
            // descenders, so a wrapped line starting lowercase is the line before's)
            let starts = t.chars().next().is_some_and(|c| !c.is_lowercase());
            if (h > body * LARGER && t.split_whitespace().count() <= 12 && !bullet(&t) && starts) || titled(&t) {
                g.head(&t);
                last = None;
                continue;
            }
            let near = last.is_some_and(|l| at[1] - (l[1] + l[3]) < at[3] * WRAP);
            g.row(&t, near && !bullet(&t));
            last = Some(at);
        }
    }
    g.close();
    g.out
}

/// A Word document's paragraphs, each with whether its style is a heading's
pub fn from_paragraphs(paras: &[(String, bool)]) -> Vec<Passage> {
    let mut g = Gather::default();
    for (t, heading) in paras {
        let t = t.trim();
        if t.is_empty() {
            continue;
        }
        if *heading || titled(t) {
            g.head(t);
        } else {
            g.row(t, false);
        }
    }
    g.close();
    g.out
}

/// Plain text or Markdown: a line starting `#` or a short line ending in a colon heads those after it; a bullet is its
/// own passage, and a paragraph's lines are one
pub fn from_text(text: &str) -> Vec<Passage> {
    let mut g = Gather::default();
    for l in text.lines().map(str::trim) {
        if l.is_empty() {
            g.close();
        } else if l.starts_with('#') {
            g.head(l.trim_start_matches('#').trim());
        } else if titled(l) {
            g.head(l);
        } else {
            g.row(l, !bullet(l));
        }
    }
    g.close();
    g.out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line(text: &str, x: f64, y: f64, h: f64) -> Line {
        Line { text: text.into(), at: [x, y, 0.3, 0.012], h }
    }

    #[test]
    fn headings_head_bullets_and_wrapped_lines_are_one() {
        let page = vec![
            line("Dutch (Pet) — Street Interview Brief R1", 0.1, 0.05, 60.0),
            line("Location:", 0.1, 0.10, 28.0),
            line("• NYC", 0.1, 0.12, 30.0),
            line("Props Needed:", 0.1, 0.16, 30.0),
            line("• Phone with the Dutch app, logged into", 0.1, 0.18, 30.0),
            line("a host account", 0.1, 0.193, 30.0),
            line("• Yellow QR cards", 0.1, 0.206, 30.0),
            line("24/7 Virtual Vet Care", 0.1, 0.30, 30.0),
            line("The simplest way to say what Dutch is.", 0.5, 0.30, 30.0),
            line("3", 0.5, 0.95, 30.0),
        ];
        let p = from_pages(&[page]);
        let said: Vec<String> = p.iter().map(Passage::said).collect();
        assert_eq!(said, [
            "Location: NYC",
            "Props Needed: Phone with the Dutch app, logged into a host account",
            "Props Needed: Yellow QR cards",
            "Props Needed: 24/7 Virtual Vet Care — The simplest way to say what Dutch is.",
        ]);
        assert_eq!(p[0].id, Passage::new("Location", "NYC", 9).id, "an id is its words, wherever they are");
    }

    #[test]
    fn text_and_paragraphs_read_alike() {
        let t = from_text("# CTA\nEnd on the ask:\nMake your Wishlist\non Elfster.\n\n- It's free\n- No White Elephant");
        let said: Vec<String> = t.iter().map(Passage::said).collect();
        assert_eq!(said, ["End on the ask: Make your Wishlist on Elfster.", "End on the ask: It's free", "End on the ask: No White Elephant"]);
        let w = from_paragraphs(&[("Music".into(), true), ("Upbeat holiday".into(), false)]);
        assert_eq!(w[0].said(), "Music: Upbeat holiday");
    }
}
