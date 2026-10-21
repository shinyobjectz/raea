//! The magic terminal, emulated: Alacritty's terminal core (alacritty_terminal: the grid, cursor, colours, wrapping,
//! scrollback and the escape codes a PTY speaks) fed the ANSI void writes, drawn as rows of styled runs. There's no
//! shell behind it; what scrolls up is the narrator's output (narrate.rs), and the prompt is a line editor here.
//!
//! The committed output is kept as ANSI text. Each frame resets the emulator and replays it, then draws the live
//! bottom (the reply as it streams, the running step with its spinner or progress bar, and the prompt), so spinners
//! and bars animate in place and nothing has to track where the cursor went.

use alacritty_terminal::event::VoidListener;
use alacritty_terminal::grid::Scroll;
use alacritty_terminal::term::cell::Flags;
use alacritty_terminal::term::test::TermSize;
use alacritty_terminal::term::{Config, Term};
use alacritty_terminal::vte::ansi::{Color, NamedColor, Processor, Rgb};

/// The palette, as truecolor escapes.
pub mod ink {
    pub const FG: (u8, u8, u8) = (226, 228, 235);
    pub const DIM: (u8, u8, u8) = (122, 126, 140);
    pub const FAINT: (u8, u8, u8) = (78, 82, 96);
    pub const CYAN: (u8, u8, u8) = (110, 220, 255);
    pub const VIOLET: (u8, u8, u8) = (160, 165, 255);
    pub const GREEN: (u8, u8, u8) = (150, 240, 190);
    pub const RED: (u8, u8, u8) = (255, 135, 135);
    pub const AMBER: (u8, u8, u8) = (255, 204, 102);
}

fn fg(c: (u8, u8, u8)) -> String {
    format!("\x1b[38;2;{};{};{}m", c.0, c.1, c.2)
}
const RESET: &str = "\x1b[0m";

/// A run of text with its inline style: plain, **bold**, or `code`.
#[derive(Clone, Copy, PartialEq)]
enum Style {
    Plain,
    Bold,
    Code,
}

/// Light markdown read into words, each a list of styled pieces (a word can be half bold: "**this**,"). Headings
/// ("## x") come out bold, bullets ("- x", "* x") as "•".
fn md_words(line: &str) -> Vec<Vec<(String, Style)>> {
    let t = line.trim_end();
    let (lead, body, all_bold) = if let Some(h) = t.trim_start().strip_prefix('#') {
        ("", h.trim_start_matches('#').trim_start(), true)
    } else if let Some(b) = t.trim_start().strip_prefix("- ").or_else(|| t.trim_start().strip_prefix("* ")) {
        ("• ", b, false)
    } else {
        ("", t, false)
    };
    let mut words: Vec<Vec<(String, Style)>> = Vec::new();
    let mut word: Vec<(String, Style)> = Vec::new();
    let mut cur = String::new();
    let (mut bold, mut code) = (all_bold, false);
    let style = |bold: bool, code: bool| if code { Style::Code } else if bold { Style::Bold } else { Style::Plain };
    let text = format!("{lead}{body}");
    let mut it = text.chars().peekable();
    while let Some(c) = it.next() {
        if c == '`' {
            if !cur.is_empty() { word.push((std::mem::take(&mut cur), style(bold, code))); }
            code = !code;
        } else if c == '*' && it.peek() == Some(&'*') && !code {
            it.next();
            if !cur.is_empty() { word.push((std::mem::take(&mut cur), style(bold, code))); }
            bold = !bold || all_bold;
        } else if c.is_whitespace() {
            if !cur.is_empty() { word.push((std::mem::take(&mut cur), style(bold, code))); }
            if !word.is_empty() { words.push(std::mem::take(&mut word)); }
        } else {
            cur.push(c);
        }
    }
    if !cur.is_empty() { word.push((cur, style(bold, code))); }
    if !word.is_empty() { words.push(word); }
    words
}

/// Text wrapped to `width` columns by words (a word longer than a line is broken), each line after the first indented
/// to `indent` columns, so a long step or reply stays a block under its own column at any width. `head` is the first
/// line's prefix (already styled) and `indent` its visible width; `color` the text's ink.
fn wrapped(head: &str, indent: usize, color: (u8, u8, u8), text: &str, width: usize) -> Vec<String> {
    let room = width.saturating_sub(indent).max(8);
    let mut out: Vec<String> = Vec::new();
    let paint = |piece: &str, st: Style| match st {
        Style::Plain => format!("{}{piece}", fg(color)),
        Style::Bold => format!("\x1b[1m{}{piece}\x1b[22m", fg(color)),
        Style::Code => format!("{}{piece}", fg(ink::CYAN)),
    };
    for (n, src) in text.split('\n').enumerate() {
        let mut line = String::new();
        let mut used = 0usize;
        let mut lines: Vec<String> = Vec::new();
        for word in md_words(src) {
            let len: usize = word.iter().map(|(p, _)| p.chars().count()).sum();
            if used > 0 && used + 1 + len > room {
                lines.push(std::mem::take(&mut line));
                used = 0;
            }
            if used > 0 {
                line.push(' ');
                used += 1;
            }
            for (piece, st) in word {
                // a word longer than the line is broken where the line ends
                let mut rest: Vec<char> = piece.chars().collect();
                while used + rest.len() > room && room > used {
                    let take = room - used;
                    let head_part: String = rest.drain(..take).collect();
                    line.push_str(&paint(&head_part, st));
                    lines.push(std::mem::take(&mut line));
                    used = 0;
                }
                let s: String = rest.into_iter().collect();
                used += s.chars().count();
                line.push_str(&paint(&s, st));
            }
        }
        if !line.is_empty() || lines.is_empty() {
            lines.push(line);
        }
        for (i, l) in lines.into_iter().enumerate() {
            let prefix = if n == 0 && i == 0 { head.to_string() } else { " ".repeat(indent) };
            out.push(format!("{prefix}{l}{RESET}"));
        }
    }
    out
}
const SPIN: [char; 10] = ['⠋', '⠙', '⠹', '⠸', '⠼', '⠴', '⠦', '⠧', '⠇', '⠏'];

/// A run of cells with one style, as the view draws it.
#[derive(Clone, Debug, PartialEq)]
pub struct Run {
    pub text: String,
    pub fg: (u8, u8, u8),
    pub bg: Option<(u8, u8, u8)>,
    pub bold: bool,
    pub dim: bool,
    pub italic: bool,
    pub underline: bool,
}

/// One line of the conversation, for `load`.
pub enum Entry<'a> {
    You(&'a str),
    Note(&'a str),
    /// A process: a tool at work (its verb, what it's doing or, done, its summary; the lines kept under it).
    Proc { verb: &'a str, text: &'a str, live: bool, mark: char, detail: &'a [String] },
    /// The voice: the brain's prose (a murmur between steps, or the reply), typed out as it streams.
    Voice { id: u64, text: &'a str, live: bool, murmur: bool },
}

pub struct Shell {
    term: Term<VoidListener>,
    parser: Processor,
    cols: usize,
    rows: usize,
    /// Everything printed so far, as ANSI.
    log: String,
    /// The reply as it streams in.
    reply: String,
    /// The running process: its verb, what it's doing and, when known, how far along (0-1).
    live: Option<(String, String, Option<f32>)>,
    /// How much of each voice line has been typed out, and the frame that was counted last.
    typed: std::collections::HashMap<u64, usize>,
    typed_frame: usize,
    pub input: String,
    /// The input is what the mic hears (dim), not what's typed.
    pub dim_input: bool,
    history: Vec<String>,
    hist_at: Option<usize>,
    frame: usize,
    /// Lines scrolled back from the bottom.
    scroll: i32,
    /// The running step's connector (a tree branch), and the frame it started on (its elapsed time).
    live_branch: &'static str,
    live_since: (String, usize),
    /// The agent is thinking (a model round, no step running): the frame it started on, for a live row of its own under
    /// the last line, where the turn's work goes on.
    thinking: Option<usize>,
    /// The step that was running last, and the frame it finished on: it lands in the log bright and dims a moment later.
    last_live: String,
    settled: (String, usize),
}

impl Shell {
    pub fn new(cols: usize, rows: usize) -> Self {
        let (cols, rows) = (cols.max(20), rows.max(3));
        let config = Config { scrolling_history: 4000, ..Config::default() };
        Self {
            term: Term::new(config, &TermSize::new(cols, rows), VoidListener),
            parser: Processor::new(),
            cols,
            rows,
            log: String::new(),
            reply: String::new(),
            live: None,
            typed: std::collections::HashMap::new(),
            typed_frame: 0,
            input: String::new(),
            dim_input: false,
            history: Vec::new(),
            hist_at: None,
            frame: 0,
            scroll: 0,
            live_branch: "└─",
            live_since: (String::new(), 0),
            thinking: None,
            last_live: String::new(),
            settled: (String::new(), 0),
        }
    }

    pub fn resize(&mut self, cols: usize, rows: usize) {
        let (cols, rows) = (cols.max(20), rows.max(3));
        if (cols, rows) != (self.cols, self.rows) {
            self.cols = cols;
            self.rows = rows;
            self.term.resize(TermSize::new(cols, rows));
        }
    }

    fn commit(&mut self, line: &str) {
        self.log.push_str(line);
        self.log.push_str("\r\n");
        // keep the replay small: the last ~3000 lines
        if self.log.len() > 400_000 {
            let cut = self.log.len() - 300_000;
            let at = self.log[cut..].find("\r\n").map(|i| cut + i + 2).unwrap_or(cut);
            self.log.drain(..at);
        }
        self.scroll = 0;
    }

    /// What the person typed or said.
    pub fn you(&mut self, text: &str) {
        for l in wrapped(&format!("{}❯{RESET} ", fg(ink::CYAN)), 2, ink::FG, text, self.cols) {
            self.commit(&l);
        }
    }

    pub fn note(&mut self, text: &str) {
        for l in wrapped(&format!("  {}· ", fg(ink::DIM)), 4, ink::DIM, text, self.cols) {
            self.commit(&l);
        }
    }

    /// A step's columns: its branch of the turn's tree, its mark, its verb (left out when the terminal is narrow), and
    /// where its text starts.
    fn step_head(&self, branch: &str, mark: &str, mark_color: (u8, u8, u8), verb: &str, verb_color: (u8, u8, u8)) -> (String, usize) {
        let b = format!("  {}{branch}{RESET} ", fg(ink::FAINT));
        if self.cols < 56 {
            (format!("{b}{}{mark}{RESET} ", fg(mark_color)), 7)
        } else {
            (format!("{b}{}{mark}{RESET} {}{verb:<7}{RESET}  ", fg(mark_color), fg(verb_color)), 16)
        }
    }

    /// One line, cut to the width with an ellipsis (steps never wrap: the log stays one line a step).
    fn cut(&self, text: &str, indent: usize) -> String {
        let room = self.cols.saturating_sub(indent + 1).max(8);
        let t: String = text.split_whitespace().collect::<Vec<_>>().join(" ");
        if t.chars().count() > room { format!("{}…", t.chars().take(room.saturating_sub(1)).collect::<String>()) } else { t }
    }

    /// A run of steps the log folds: how many, and of what ("… 6 more · read 3 · edit 2 · run 1").
    fn folded(&mut self, verbs: &[&str]) {
        let mut counts: Vec<(&str, usize)> = Vec::new();
        for v in verbs {
            match counts.iter_mut().find(|(k, _)| k == v) {
                Some(c) => c.1 += 1,
                None => counts.push((v, 1)),
            }
        }
        counts.sort_by(|a, b| b.1.cmp(&a.1));
        let what: String = counts.iter().map(|(v, n)| format!(" · {v} {n}")).collect();
        let what = self.cut(&format!("… {} more{what}", verbs.len()), 5);
        let line = format!("  {}│  {what}{RESET}", fg(ink::FAINT));
        self.commit(&line);
    }

    /// A process starts: it spins at the bottom until done.
    pub fn step(&mut self, verb: &str, text: &str) {
        self.flush_reply();
        let key = format!("{verb}|{text}");
        if self.live_since.0 != key {
            self.live_since = (key, self.frame);
        }
        self.live = Some((verb.to_string(), text.to_string(), None));
    }

    /// The running process's progress (0-1), shown as a bar. (Preview renders and downloads report through it next.)
    #[allow(dead_code)]
    pub fn progress(&mut self, p: f32) {
        if let Some(l) = self.live.as_mut() {
            l.2 = Some(p.clamp(0.0, 1.0));
        }
    }

    /// The process ended: its mark, verb and summary go into the log, with the lines it keeps under it.
    pub fn done(&mut self, mark: char, verb: &str, text: &str, detail: &[String]) {
        self.done_on("└─", mark, verb, text, detail);
    }

    fn done_on(&mut self, branch: &str, mark: char, verb: &str, text: &str, detail: &[String]) {
        self.live = None;
        let c = match mark {
            '✓' => ink::GREEN,
            '✗' => ink::RED,
            '⚠' => ink::AMBER,
            '◉' => ink::VIOLET,
            _ => ink::DIM,
        };
        // the step that just finished stays bright for a moment (~0.6 s), then settles into the log's grey
        let key = format!("{verb}|{text}");
        if key == self.last_live {
            self.settled = (key.clone(), self.frame);
            self.last_live.clear();
        }
        let fresh = self.settled.0 == key && self.frame.saturating_sub(self.settled.1) < 7;
        let body = if mark == '✗' { ink::RED } else if fresh { ink::FG } else { ink::DIM };
        let (head, indent) = self.step_head(branch, &mark.to_string(), c, verb, if fresh { ink::CYAN } else { ink::FAINT });
        let line = format!("{head}{}{}{RESET}", fg(body), self.cut(text, indent));
        self.commit(&line);
        // what a step keeps under it (a check's faults): a line each, under its branch
        let stem = if branch == "├─" { "│" } else { " " };
        for d in detail {
            let (sign, rest) = d.split_at(d.char_indices().nth(1).map(|(i, _)| i).unwrap_or(d.len()));
            let sc = match sign { "+" => ink::GREEN, "~" => ink::AMBER, "-" | "✗" => ink::RED, _ => ink::DIM };
            let pad = indent.saturating_sub(5);
            let line = format!("  {}{stem}{RESET}  {}{}{sign}{RESET} {}{}{RESET}", fg(ink::FAINT), " ".repeat(pad), fg(sc), fg(ink::DIM), self.cut(rest.trim_start(), indent + 2));
            self.commit(&line);
        }
    }

    /// The reply is complete: into the log, in the voice's colour.
    pub fn flush_reply(&mut self) {
        let r = std::mem::take(&mut self.reply);
        let r = r.trim();
        if !r.is_empty() {
            // the answer stands apart from the work above it: a blank line, a mark, full-brightness text
            self.commit("");
            for line in wrapped(&format!("{}◆{RESET} ", fg(ink::VIOLET)), 2, ink::FG, r, self.cols) {
                self.commit(&line);
            }
        }
    }

    /// A murmur: what the voice said between steps, one dim line.
    fn murmur(&mut self, text: &str) {
        let t: String = text.split_whitespace().collect::<Vec<_>>().join(" ");
        // what's left after the narrating opener ("I'll get oriented first" -> "get oriented first")
        let lower = t.to_lowercase();
        let t = ["i'll ", "i will ", "let me ", "i'm going to ", "i am going to ", "first, i'll ", "ok, ", "okay, "]
            .iter()
            .find(|p| lower.starts_with(*p))
            .map(|p| t[p.len()..].to_string())
            .unwrap_or(t);
        let t = t.trim().trim_end_matches(['.', ':']);
        if t.is_empty() {
            return;
        }
        let mut t = t.to_string();
        if let Some(c) = t.chars().next() {
            t = c.to_lowercase().collect::<String>() + &t[c.len_utf8()..];
        }
        // a heading for the steps under it: what the agent set out to do
        for l in wrapped(&format!("{}▸{RESET} ", fg(ink::VIOLET)), 2, ink::DIM, &t, self.cols) {
            self.commit(&l);
        }
    }

    /// The spinner's frame (the app's clock, ~11 a second while something runs).
    pub fn set_frame(&mut self, n: usize) {
        self.frame = n;
    }

    /// Whether the agent is at work: while it is and nothing else on screen moves (no step running, no reply typing),
    /// a thinking row spins under the last line.
    pub fn set_thinking(&mut self, on: bool) {
        match (on, self.thinking) {
            (true, None) => self.thinking = Some(self.frame),
            (false, Some(_)) => self.thinking = None,
            _ => {}
        }
    }

    /// How far back the last render is scrolled (clamped by the history there is).
    pub fn offset(&self) -> i32 {
        self.term.grid().display_offset() as i32
    }

    pub fn set_scroll(&mut self, lines: i32) {
        self.scroll = lines.max(0);
    }

    // the line editor

    pub fn set_input(&mut self, s: &str) {
        self.input = s.to_string();
        self.hist_at = None;
    }

    /// Enter: the line, remembered for ↑.
    pub fn take_input(&mut self) -> Option<String> {
        let line = std::mem::take(&mut self.input).trim().to_string();
        self.hist_at = None;
        if line.is_empty() {
            return None;
        }
        if self.history.last() != Some(&line) {
            self.history.push(line.clone());
        }
        Some(line)
    }

    /// ↑ / ↓ through what was typed before.
    pub fn history(&mut self, up: bool) -> &str {
        if self.history.is_empty() {
            return &self.input;
        }
        let n = self.history.len();
        self.hist_at = match (self.hist_at, up) {
            (None, true) => Some(n - 1),
            (Some(i), true) => Some(i.saturating_sub(1)),
            (Some(i), false) if i + 1 < n => Some(i + 1),
            _ => None,
        };
        self.input = self.hist_at.map(|i| self.history[i].clone()).unwrap_or_default();
        &self.input
    }

    /// The whole conversation as the terminal shows it, in order (the screen is a projection of it): the log rebuilt,
    /// the live step and the streaming reply at the bottom. The person's scroll position survives.
    pub fn load<'a>(&mut self, entries: impl IntoIterator<Item = Entry<'a>>) {
        let scroll = self.scroll;
        if let Some((v, t, _)) = &self.live {
            self.last_live = format!("{v}|{t}");
        }
        self.log.clear();
        self.live = None;
        self.reply.clear();
        // typing: each frame since the last reveals ~6 more characters of a live voice line (~70 a second)
        let step = self.frame.saturating_sub(self.typed_frame) * 6;
        self.typed_frame = self.frame;
        let entries: Vec<Entry<'a>> = entries.into_iter().collect();
        let mut i = 0;
        while i < entries.len() {
            // a run of steps: one tree under the heading before it; a long run keeps its first and last three, the rest
            // folded into a count (failures always shown)
            if matches!(entries[i], Entry::Proc { .. }) {
                let mut j = i;
                while j < entries.len() && matches!(entries[j], Entry::Proc { .. }) {
                    j += 1;
                }
                let run = &entries[i..j];
                let n = run.len();
                let keep = |k: usize, e: &Entry| n <= 5 || k == 0 || k + 3 >= n || matches!(e, Entry::Proc { mark: '✗', .. });
                let mut folded: Vec<&str> = Vec::new();
                for (k, e) in run.iter().enumerate() {
                    let Entry::Proc { verb, text, live, mark, detail } = e else { continue };
                    if !keep(k, e) {
                        folded.push(verb);
                        continue;
                    }
                    if !folded.is_empty() {
                        self.folded(&folded);
                        folded.clear();
                    }
                    let last = k + 1 == n;
                    if *live {
                        self.live_branch = if last { "└─" } else { "├─" };
                        self.step(verb, text);
                    } else {
                        self.done_on(if last { "└─" } else { "├─" }, *mark, verb, text, detail);
                    }
                }
                i = j;
                continue;
            }
            let e = &entries[i];
            i += 1;
            match *e {
                Entry::You(t) => {
                    self.flush_reply();
                    // each request opens a new stretch of the log
                    if !self.log.is_empty() {
                        self.commit("");
                    }
                    self.you(t)
                }
                Entry::Note(t) => self.note(t),
                Entry::Proc { .. } => {}
                Entry::Voice { text, murmur: true, live: false, .. } => {
                    self.flush_reply();
                    self.murmur(text);
                }
                Entry::Voice { id, text, live, .. } => {
                    self.flush_reply();
                    if live {
                        let n = self.typed.entry(id).or_insert(0);
                        // a backlog over 120 characters catches up within a second
                        let behind = text.chars().count().saturating_sub(*n);
                        *n += step.max(if behind > 120 { behind / 11 } else { 0 });
                        self.reply = text.chars().take(*n).collect();
                    } else {
                        self.typed.remove(&id);
                        self.reply = text.to_string();
                        self.flush_reply();
                    }
                }
            }
        }
        self.scroll = scroll;
    }

    /// The live bottom, as ANSI: the streaming reply, the running step, the prompt with its cursor.
    fn bottom(&self, prompt: &str) -> String {
        let mut b = String::new();
        if !self.reply.is_empty() {
            let lines = wrapped(&format!("{}◆{RESET} ", fg(ink::VIOLET)), 2, ink::FG, &self.reply, self.cols.saturating_sub(1));
            let n = lines.len();
            b += "\r\n";
            for (i, line) in lines.into_iter().enumerate() {
                // the caret rides the end of what's typed so far
                let caret = if i + 1 == n { format!("{}▍{RESET}", fg(ink::VIOLET)) } else { String::new() };
                b += &format!("{line}{caret}\r\n");
            }
        }
        if let Some((verb, text, p)) = &self.live {
            let s = SPIN[self.frame % SPIN.len()];
            // the running step stays one line: cut to the width, not wrapped
            let (head, indent) = self.step_head(self.live_branch, &s.to_string(), ink::CYAN, verb, ink::CYAN);
            // room for the bar and the time when the terminal is wide enough; the text cut to what's left
            let wide = self.cols >= 40 && p.is_none();
            let w = if self.cols >= 64 { 10usize } else { 6 };
            let tail = if wide { w + 8 } else { 0 };
            let room = self.cols.saturating_sub(indent + 1 + tail);
            let text: String = if text.chars().count() > room { format!("{}…", text.chars().take(room.saturating_sub(1)).collect::<String>()) } else { text.clone() };
            b += &format!("{head}{}{text}{RESET}", fg(ink::DIM));
            if wide {
                // an indeterminate bar, a bright stretch sweeping along it, and how long the step has run (~11 frames a second)
                b += &format!("  {}", self.sweep(w, self.live_since.1));
            }
            if let Some(p) = p {
                let w = 24usize;
                let done = (p * w as f32).round() as usize;
                b += &format!("  {}{}{}{}{RESET} {}{:>3}%{RESET}", fg(ink::CYAN), "█".repeat(done), fg(ink::FAINT), "░".repeat(w - done), fg(ink::DIM), (p * 100.0) as u32);
            }
            b += "\r\n";
        } else if let (Some(since), true) = (self.thinking, self.reply.is_empty()) {
            // the agent thinking, under the last line (the person's message, or the steps so far): a spinner, the word,
            // the sweep and the seconds
            let s = SPIN[self.frame % SPIN.len()];
            b += &format!("  {}{s}{RESET} {}thinking{RESET}", fg(ink::VIOLET), fg(ink::DIM));
            if self.cols >= 40 {
                b += &format!("  {}", self.sweep(if self.cols >= 64 { 10 } else { 6 }, since));
            }
            b += "\r\n";
        }
        let ink_in = if self.dim_input { ink::DIM } else { ink::FG };
        b += &format!("{}{prompt}{RESET} {}{}{RESET}\x1b[7m \x1b[0m", fg(ink::CYAN), fg(ink_in), self.input);
        b
    }

    /// An indeterminate bar `w` cells wide, a bright stretch sweeping along it, and the seconds since frame `since`.
    fn sweep(&self, w: usize, since: usize) -> String {
        let at = self.frame % (w + 4);
        let bar: String = (0..w).map(|c| if c + 3 >= at && c <= at { format!("{}━", fg(ink::CYAN)) } else { format!("{}─", fg(ink::FAINT)) }).collect();
        let secs = self.frame.saturating_sub(since) / 11;
        format!("{bar}{RESET} {}{secs}s{RESET}", fg(ink::FAINT))
    }

    /// The screen: the log replayed, the bottom drawn, scrolled back as asked; rows of styled runs.
    pub fn render(&mut self, prompt: &str) -> Vec<Vec<Run>> {
        let bytes = format!("\x1bc{}{}", self.log, self.bottom(prompt));
        self.parser.advance(&mut self.term, bytes.as_bytes());
        if self.scroll > 0 {
            self.term.scroll_display(Scroll::Delta(self.scroll));
        }
        let offset = self.term.grid().display_offset() as i32;
        let mut rows: Vec<Vec<Run>> = vec![Vec::new(); self.rows];
        let content = self.term.renderable_content();
        for cell in content.display_iter {
            let line = cell.point.line.0 + offset;
            if line < 0 || line as usize >= self.rows || cell.cell.flags.contains(Flags::WIDE_CHAR_SPACER) {
                continue;
            }
            let f = cell.cell.flags;
            let (mut fgc, mut bgc) = (rgb(cell.cell.fg, true).unwrap_or(ink::FG), rgb(cell.cell.bg, false));
            if f.contains(Flags::INVERSE) {
                let swapped = bgc.unwrap_or((10, 10, 13));
                bgc = Some(fgc);
                fgc = swapped;
            }
            let run = Run {
                text: cell.cell.c.to_string(),
                fg: fgc,
                bg: bgc,
                bold: f.contains(Flags::BOLD),
                dim: f.contains(Flags::DIM),
                italic: f.contains(Flags::ITALIC),
                underline: f.contains(Flags::UNDERLINE),
            };
            let row = &mut rows[line as usize];
            match row.last_mut() {
                Some(last) if (last.fg, last.bg, last.bold, last.dim, last.italic, last.underline) == (run.fg, run.bg, run.bold, run.dim, run.italic, run.underline) => {
                    last.text.push(cell.cell.c)
                }
                _ => row.push(run),
            }
        }
        // trailing blanks drawn with no style are nothing; the view anchors what's left to the bottom, so the rows
        // after the prompt (an unfilled screen) go too
        for row in rows.iter_mut() {
            while row.last().is_some_and(|r| r.bg.is_none() && r.text.trim().is_empty()) {
                row.pop();
            }
            if let Some(last) = row.last_mut().filter(|r| r.bg.is_none()) {
                let t = last.text.trim_end().len();
                last.text.truncate(t);
            }
        }
        while rows.last().is_some_and(|r| r.is_empty()) {
            rows.pop();
        }
        rows
    }
}

/// A cell colour as RGB: truecolor as is, the named and indexed ones through void's palette; the default background
/// is none (the panel shows through).
pub(crate) fn rgb(c: Color, foreground: bool) -> Option<(u8, u8, u8)> {
    match c {
        Color::Spec(Rgb { r, g, b }) => Some((r, g, b)),
        Color::Named(n) => match n {
            NamedColor::Foreground | NamedColor::BrightForeground => Some(ink::FG),
            NamedColor::Background => None,
            NamedColor::DimForeground => Some(ink::DIM),
            NamedColor::Red | NamedColor::BrightRed => Some(ink::RED),
            NamedColor::Green | NamedColor::BrightGreen => Some(ink::GREEN),
            NamedColor::Yellow | NamedColor::BrightYellow => Some(ink::AMBER),
            NamedColor::Blue | NamedColor::BrightBlue | NamedColor::Magenta | NamedColor::BrightMagenta => Some(ink::VIOLET),
            NamedColor::Cyan | NamedColor::BrightCyan => Some(ink::CYAN),
            NamedColor::Black | NamedColor::BrightBlack => Some(ink::FAINT),
            _ => Some(ink::FG),
        },
        Color::Indexed(_) => Some(ink::FG),
    }
    .or(if foreground { Some(ink::FG) } else { None })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text(rows: &[Vec<Run>]) -> Vec<String> {
        rows.iter().map(|r| r.iter().map(|x| x.text.as_str()).collect::<String>()).collect()
    }

    #[test]
    fn narrow_steps_stay_one_line_on_their_branch() {
        let mut s = Shell::new(40, 12);
        s.load([Entry::Proc { verb: "study", text: "studied modes/game, motion/games, motion/sound, stages/direct", live: false, mark: '✓', detail: &[] },
            Entry::Voice { id: 1, text: "The **match** plays out 3-2 with the `match point` as the longest rally.", live: false, murmur: false }]);
        let rows = text(&s.render("❯"));
        let shown: Vec<&String> = rows.iter().filter(|r| !r.trim().is_empty()).collect();
        // no verb column at 40 columns, the step one line on its branch (cut with an ellipsis), the answer marked and
        // wrapped under itself, nothing past the edge, no markdown left
        assert!(shown[0].starts_with("  └─ ✓ studied") && shown[0].ends_with('…'), "{shown:?}");
        assert!(shown[1].starts_with("◆ The match"), "{shown:?}");
        assert!(shown[2].starts_with("  ") && !shown[2].starts_with("   "), "{shown:?}");
        assert!(shown.iter().all(|r| r.chars().count() <= 40), "{shown:?}");
        assert!(!rows.join(" ").contains("**") && !rows.join(" ").contains('`'), "{rows:?}");
        // wide: the verb column is back
        let mut w = Shell::new(100, 6);
        w.load([Entry::Proc { verb: "study", text: "studied modes/game", live: false, mark: '✓', detail: &[] }]);
        assert!(text(&w.render("❯"))[0].starts_with("  └─ ✓ study    studied"));
    }

    #[test]
    fn the_work_shows_under_the_message_that_asked_for_it() {
        let mut s = Shell::new(60, 10);
        s.load([Entry::You("make pong")]);
        s.set_thinking(true);
        let t = text(&s.render("❯"));
        let you = t.iter().position(|l| l.contains("❯ make pong")).expect("the message");
        let think = t.iter().position(|l| l.contains("⠋ thinking")).expect("the thinking row");
        assert!(think == you + 1, "{t:?}");
        // the row spins, its bar sweeps and its seconds count
        s.set_frame(22);
        let t = text(&s.render("❯"));
        assert!(t.iter().any(|l| l.contains("thinking") && l.contains("2s") && l.contains("━")), "{t:?}");
        // a step running takes the live row; done, it lands under the message bright, then settles
        s.load([Entry::You("make pong"), Entry::Proc { verb: "write", text: "edit.tsx", live: true, mark: ' ', detail: &[] }]);
        let t = text(&s.render("❯"));
        assert!(!t.iter().any(|l| l.contains("thinking")), "{t:?}");
        assert!(t.iter().position(|l| l.contains("write")) > t.iter().position(|l| l.contains("❯ make pong")), "{t:?}");
        s.load([Entry::You("make pong"), Entry::Proc { verb: "write", text: "edit.tsx", live: false, mark: '✓', detail: &[] }]);
        let rows = s.render("❯");
        let bright = |rows: &[Vec<Run>]| rows.iter().flatten().any(|r| r.text.contains("edit.tsx") && r.fg == ink::FG);
        assert!(bright(&rows), "just finished: bright");
        assert!(text(&rows).iter().any(|l| l.contains("thinking")), "still thinking after the step");
        s.set_frame(40);
        s.load([Entry::You("make pong"), Entry::Proc { verb: "write", text: "edit.tsx", live: false, mark: '✓', detail: &[] }]);
        assert!(!bright(&s.render("❯")), "settled: grey");
        s.set_thinking(false);
        assert!(!text(&s.render("❯")).iter().any(|l| l.contains("thinking")));
    }

    #[test]
    fn steps_spin_in_place_and_land_in_the_log() {
        let mut s = Shell::new(60, 8);
        s.you("make the title red");
        s.step("write", "changing the edit");
        let t = text(&s.render("❯"));
        assert!(t.iter().any(|l| l.contains("❯ make the title red")), "{t:?}");
        assert!(t.iter().any(|l| l.contains("⠋ write    changing the edit")), "{t:?}");
        s.set_frame(1);
        assert!(text(&s.render("❯")).iter().any(|l| l.contains("⠙ write")));
        s.done('✓', "write", "changed 1 thing", &["~ hook".to_string()]);
        s.reply = "Done, the title is red.".into();
        s.flush_reply();
        let t = text(&s.render("❯"));
        assert!(t.iter().any(|l| l.contains("✓ write    changed 1 thing")), "{t:?}");
        assert!(t.iter().any(|l| l.trim() == "~ hook"), "{t:?}");
        assert!(!t.iter().any(|l| l.contains("changing the edit")), "{t:?}");
        assert!(t.last().is_some_and(|l| l.starts_with("❯")), "{t:?}");
    }

    #[test]
    fn progress_draws_a_bar_and_colours_survive() {
        let mut s = Shell::new(70, 6);
        s.step("render", "rendering preview");
        s.progress(0.5);
        let rows = s.render("❯");
        let t = text(&rows);
        assert!(t.iter().any(|l| l.contains("████████████░░░░░░░░░░░░  50%")), "{t:?}");
        assert!(rows.iter().flatten().any(|r| r.fg == ink::CYAN && r.text.contains('█')));
    }

    #[test]
    fn long_output_scrolls_and_scrolls_back() {
        let mut s = Shell::new(40, 5);
        for i in 0..30 {
            s.note(&format!("line {i}"));
        }
        assert!(text(&s.render("❯")).iter().any(|l| l.contains("line 29")));
        s.set_scroll(10);
        let t = text(&s.render("❯"));
        assert!(t.iter().any(|l| l.contains("line 2")) && !t.iter().any(|l| l.contains("line 29")), "{t:?}");
    }

    #[test]
    fn history() {
        let mut s = Shell::new(40, 5);
        s.set_input("one");
        assert_eq!(s.take_input().as_deref(), Some("one"));
        s.set_input("two");
        s.take_input();
        assert_eq!(s.history(true), "two");
        assert_eq!(s.history(true), "one");
        assert_eq!(s.history(false), "two");
    }
}

#[cfg(test)]
mod reset_tests {
    use super::*;

    #[test]
    fn a_frame_leaves_nothing_behind() {
        let mut s = Shell::new(40, 6);
        s.load([Entry::You("make it red")]);
        let _ = s.render("❯");
        s.load([Entry::You("make it red"), Entry::Proc { verb: "write", text: "changed 1 thing", live: false, mark: '✓', detail: &[] }]);
        let rows = s.render("❯");
        let stray: Vec<_> = rows.iter().enumerate().flat_map(|(i, r)| r.iter().filter(|x| x.bg.is_some()).map(move |x| (i, x.text.clone()))).collect();
        // only the prompt's cursor has a background
        assert_eq!(stray.len(), 1, "{stray:?} in {:?}", rows.iter().map(|r| r.iter().map(|x| x.text.as_str()).collect::<String>()).collect::<Vec<_>>());
    }
}

#[cfg(test)]
mod mark_tests {
    use super::*;

    #[test]
    fn marks_leave_no_stray_cells() {
        let mut s = Shell::new(60, 6);
        s.load([Entry::Proc { verb: "spaces", text: "listing spaces", live: false, mark: '·', detail: &[] }, Entry::Proc { verb: "look", text: "looked", live: false, mark: '◉', detail: &[] }]);
        let rows = s.render("❯");
        for r in &rows {
            println!("{:?}", r.iter().map(|x| (x.text.clone(), x.bg)).collect::<Vec<_>>());
        }
        let stray = rows.iter().take(2).flatten().filter(|x| x.bg.is_some()).count();
        assert_eq!(stray, 0);
    }

    #[test]
    fn a_long_run_of_steps_folds_into_a_count() {
        let mut s = Shell::new(80, 30);
        let verbs = ["read", "read", "edit", "read", "edit", "run", "read", "check", "write"];
        let entries: Vec<Entry> = verbs.iter().map(|v| Entry::Proc { verb: v, text: "a file", live: false, mark: '✓', detail: &[] }).collect();
        s.load(entries);
        let rows: Vec<String> = super::tests_text(&s.render("❯")).into_iter().filter(|r| !r.trim().is_empty() && !r.contains('❯')).collect();
        // the first, a fold, the last three
        assert_eq!(rows.len(), 5, "{rows:?}");
        assert!(rows[0].starts_with("  ├─ ✓ read"), "{rows:?}");
        assert!(rows[1].contains("│  … 5 more · read 2 · edit 2 · run 1"), "{rows:?}");
        assert!(rows[4].starts_with("  └─ ✓ write"), "{rows:?}");
    }
}

#[cfg(test)]
fn tests_text(rows: &[Vec<Run>]) -> Vec<String> {
    rows.iter().map(|r| r.iter().map(|x| x.text.as_str()).collect::<String>()).collect()
}
