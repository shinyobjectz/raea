//! The cli terminal (apps/void/shell.feature: The cli terminal): the person's login shell in the workspace folder over a pty,
//! Alacritty's terminal core (alacritty_terminal: the grid, the escape codes, scrollback, full-screen programs) reading
//! it on a thread of its own. The dock's cli control (apps/void/controls/cli/control.tsx) draws its screen: the panel thread
//! hands it each change as `void:pty JSON` (rows of styled runs and the cursor), keys go from the chrome to the shell as
//! the bytes a terminal sends, and the panel's box is the pty's size in cells.

use std::borrow::Cow;
use std::collections::HashMap;
use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use alacritty_terminal::event::{Event, EventListener, WindowSize};
use alacritty_terminal::event_loop::{EventLoop, EventLoopSender, Msg};
use alacritty_terminal::grid::Scroll;
use alacritty_terminal::sync::FairMutex;
use alacritty_terminal::term::cell::Flags;
use alacritty_terminal::term::test::TermSize;
use alacritty_terminal::term::{Config, Term, TermMode};
use alacritty_terminal::tty;
use alacritty_terminal::vte::ansi::{Color, NamedColor};

/// A cell in points, at the cli's font (Menlo 12, as control.tsx draws it): the panel's box over these is the pty's size.
pub const CELL_W: f64 = 7.23;
pub const CELL_H: f64 = 16.0;
/// The padding around the grid inside the panel, in points.
pub const PAD: f64 = 8.0;

/// What the pty's reader tells: new output (the screen is sent again), what the shell asks written back, its exit.
#[derive(Clone)]
struct Listener {
    drawn: Arc<AtomicU64>,
    exited: Arc<AtomicBool>,
    tx: Arc<Mutex<Option<EventLoopSender>>>,
    title: Arc<Mutex<String>>,
}

impl EventListener for Listener {
    fn send_event(&self, e: Event) {
        match e {
            Event::Wakeup => {
                self.drawn.fetch_add(1, Ordering::Relaxed);
            }
            Event::PtyWrite(s) => {
                if let Some(tx) = self.tx.lock().unwrap().as_ref() {
                    let _ = tx.send(Msg::Input(Cow::Owned(s.into_bytes())));
                }
            }
            Event::Title(t) => *self.title.lock().unwrap() = t,
            Event::ResetTitle => self.title.lock().unwrap().clear(),
            Event::ChildExit(_) | Event::Exit => {
                self.exited.store(true, Ordering::Relaxed);
                self.drawn.fetch_add(1, Ordering::Relaxed);
            }
            _ => {}
        }
    }
}

struct Pty {
    term: Arc<FairMutex<Term<Listener>>>,
    tx: EventLoopSender,
    size: (u16, u16),
    l: Listener,
}

static PTYS: Mutex<Option<HashMap<u64, Pty>>> = Mutex::new(None);

fn with<R>(id: u64, f: impl FnOnce(&mut Pty) -> R) -> Option<R> {
    PTYS.lock().unwrap().as_mut()?.get_mut(&id).map(f)
}

/// The cells a box of `w` by `h` points holds: (columns, rows).
pub fn cells(w: f64, h: f64) -> (u16, u16) {
    (((w - 2.0 * PAD) / CELL_W).floor().clamp(2.0, 1000.0) as u16, ((h - 2.0 * PAD) / CELL_H).floor().clamp(1.0, 500.0) as u16)
}

fn window(cols: u16, rows: u16) -> WindowSize {
    WindowSize { num_lines: rows, num_cols: cols, cell_width: CELL_W.round() as u16, cell_height: CELL_H as u16 }
}

/// Shell `id` at `cols` by `rows`: started (the login shell, in `cwd`) the first time, resized after.
pub fn size(id: u64, cols: u16, rows: u16, cwd: Option<&Path>) -> Result<(), String> {
    if let Some(r) = with(id, |p| {
        if p.size != (cols, rows) {
            p.size = (cols, rows);
            p.term.lock().resize(TermSize::new(cols as usize, rows as usize));
            let _ = p.tx.send(Msg::Resize(window(cols, rows)));
            p.l.drawn.fetch_add(1, Ordering::Relaxed);
        }
    }) {
        return Ok(r);
    }
    let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/zsh".into());
    let mut env = HashMap::new();
    env.insert("TERM".to_string(), "xterm-256color".to_string());
    env.insert("COLORTERM".to_string(), "truecolor".to_string());
    env.insert("TERM_PROGRAM".to_string(), "raea".to_string());
    let opts = tty::Options { shell: Some(tty::Shell::new(shell.clone(), vec!["-l".into()])), working_directory: cwd.map(Path::to_path_buf), drain_on_exit: true, env };
    let pty = tty::new(&opts, window(cols, rows), id).map_err(|e| format!("{shell}: {e}"))?;
    let l = Listener { drawn: Arc::new(AtomicU64::new(1)), exited: Arc::new(AtomicBool::new(false)), tx: Arc::new(Mutex::new(None)), title: Arc::new(Mutex::new(String::new())) };
    let config = Config { scrolling_history: 5000, ..Config::default() };
    let term = Arc::new(FairMutex::new(Term::new(config, &TermSize::new(cols as usize, rows as usize), l.clone())));
    let ev = EventLoop::new(term.clone(), l.clone(), pty, true, false).map_err(|e| e.to_string())?;
    let tx = ev.channel();
    *l.tx.lock().unwrap() = Some(tx.clone());
    ev.spawn();
    crate::mcp::log(format!("cli {id}: {shell} -l in {} at {cols}x{rows}", cwd.map(|c| c.display().to_string()).unwrap_or("~".into())));
    PTYS.lock().unwrap().get_or_insert_with(HashMap::new).insert(id, Pty { term, tx, size: (cols, rows), l });
    Ok(())
}

/// Bytes to shell `id`, as typed: the screen goes back to the bottom.
pub fn input(id: u64, bytes: Vec<u8>) {
    with(id, |p| {
        let mut t = p.term.lock();
        if t.grid().display_offset() > 0 {
            t.scroll_display(Scroll::Bottom);
            p.l.drawn.fetch_add(1, Ordering::Relaxed);
        }
        drop(t);
        let _ = p.tx.send(Msg::Input(Cow::Owned(bytes)));
    });
}

/// Text pasted into shell `id`: bracketed when the shell asked for it, so a pasted line doesn't run.
pub fn paste(id: u64, text: &str) {
    let bracketed = with(id, |p| p.term.lock().mode().contains(TermMode::BRACKETED_PASTE)).unwrap_or(false);
    let text = text.replace("\r\n", "\r").replace('\n', "\r");
    input(id, if bracketed { format!("\x1b[200~{text}\x1b[201~") } else { text }.into_bytes());
}

/// The screen of shell `id` scrolled back by `lines` (negative: towards the bottom).
pub fn scroll(id: u64, lines: i32) {
    with(id, |p| {
        p.term.lock().scroll_display(Scroll::Delta(lines));
        p.l.drawn.fetch_add(1, Ordering::Relaxed);
    });
}

/// Shell `id` closed (its tab closed): hung up, its process told to go.
pub fn close(id: u64) {
    if let Some(p) = PTYS.lock().unwrap().as_mut().and_then(|m| m.remove(&id)) {
        let _ = p.tx.send(Msg::Shutdown);
    }
}

/// The ids of the shells running.
pub fn open() -> Vec<u64> {
    PTYS.lock().unwrap().as_ref().map(|m| m.keys().copied().collect()).unwrap_or_default()
}

/// Shell `id`'s size in cells: (columns, rows).
pub fn cells_of(id: u64) -> Option<(u16, u16)> {
    with(id, |p| p.size)
}

/// Shell `id`'s screen as plain text, a line a row (the MCP's state: what the agent and the checks read).
pub fn text(id: u64) -> String {
    with(id, |p| {
        let t = p.term.lock();
        let rows = p.size.1 as usize;
        let mut lines = vec![String::new(); rows];
        let offset = t.grid().display_offset() as i32;
        for cell in t.renderable_content().display_iter {
            let line = cell.point.line.0 + offset;
            if line >= 0 && (line as usize) < rows && !cell.cell.flags.contains(Flags::WIDE_CHAR_SPACER) {
                lines[line as usize].push(cell.cell.c);
            }
        }
        lines.iter().map(|l| l.trim_end()).collect::<Vec<_>>().join("\n").trim_end().to_string()
    })
    .unwrap_or_default()
}

/// What a key pressed sends to shell `id`: the bytes a terminal sends for it (the arrows as the shell's mode asks,
/// Control-letter as its control code, Option as an escape before it). None for a key that sends nothing.
pub fn key_bytes(id: u64, key: &str, ctrl: bool, alt: bool) -> Option<Vec<u8>> {
    let app = with(id, |p| p.term.lock().mode().contains(TermMode::APP_CURSOR)).unwrap_or(false);
    let arrow = |c: char| if app { format!("\x1bO{c}") } else { format!("\x1b[{c}") };
    let s: String = match key {
        "Enter" => "\r".into(),
        "Backspace" => if alt { "\x1b\x7f".into() } else { "\x7f".into() },
        "Tab" => "\t".into(),
        "Escape" => "\x1b".into(),
        "ArrowUp" => arrow('A'),
        "ArrowDown" => arrow('B'),
        "ArrowRight" => if alt { "\x1bf".into() } else { arrow('C') },
        "ArrowLeft" => if alt { "\x1bb".into() } else { arrow('D') },
        "Home" => arrow('H'),
        "End" => arrow('F'),
        "Delete" => "\x1b[3~".into(),
        "PageUp" => "\x1b[5~".into(),
        "PageDown" => "\x1b[6~".into(),
        k if k.chars().count() == 1 => {
            let c = k.chars().next()?;
            // Control-letter already made a control code (as macOS sends it)
            if (c as u32) < 0x20 {
                return Some(if alt { vec![0x1b, c as u8] } else { vec![c as u8] });
            }
            if ctrl {
                let b = match c.to_ascii_lowercase() {
                    l @ 'a'..='z' => l as u8 - b'a' + 1,
                    '@' | ' ' | '2' => 0,
                    '[' | '3' => 0x1b,
                    '\\' | '4' => 0x1c,
                    ']' | '5' => 0x1d,
                    '^' | '6' => 0x1e,
                    '_' | '-' | '7' => 0x1f,
                    _ => return None,
                };
                return Some(if alt { vec![0x1b, b] } else { vec![b] });
            }
            if alt { format!("\x1b{k}") } else { k.to_string() }
        }
        _ => return None,
    };
    Some(s.into_bytes())
}

/// Shell `id`'s screen when it changed since `seen` (its generation, to pass next time): rows of runs (text and
/// style, each from a column), the cursor, how far it's scrolled back, the title, whether the shell's gone.
pub fn screen_since(id: u64, seen: u64) -> Option<(u64, String)> {
    with(id, |p| {
        let g = p.l.drawn.load(Ordering::Relaxed);
        if g == seen {
            return None;
        }
        let t = p.term.lock();
        let (cols, rows) = (p.size.0 as usize, p.size.1 as usize);
        let offset = t.grid().display_offset() as i32;
        let content = t.renderable_content();
        let cursor = content.cursor.point;
        let show_cursor = offset == 0 && t.mode().contains(TermMode::SHOW_CURSOR);
        // each row as runs: [col, text, fg, bg | null, flags (1 bold, 2 dim, 4 italic, 8 underline)]
        let mut lines: Vec<Vec<(usize, String, (u8, u8, u8), Option<(u8, u8, u8)>, u8)>> = vec![Vec::new(); rows];
        for cell in content.display_iter {
            let line = cell.point.line.0 + offset;
            let col = cell.point.column.0;
            if line < 0 || line as usize >= rows || col >= cols || cell.cell.flags.contains(Flags::WIDE_CHAR_SPACER) {
                continue;
            }
            let f = cell.cell.flags;
            let (mut fg, mut bg) = (color(cell.cell.fg, true).unwrap_or(crate::term::ink::FG), color(cell.cell.bg, false));
            if f.contains(Flags::INVERSE) {
                let swapped = bg.unwrap_or(BG);
                bg = Some(fg);
                fg = swapped;
            }
            let flags = f.contains(Flags::BOLD) as u8 | (f.contains(Flags::DIM) as u8) << 1 | (f.contains(Flags::ITALIC) as u8) << 2 | (f.contains(Flags::UNDERLINE) as u8) << 3;
            let row = &mut lines[line as usize];
            match row.last_mut() {
                // the same style, and the next column along (a wide character takes two)
                Some(last) if (last.2, last.3, last.4) == (fg, bg, flags) => last.1.push(cell.cell.c),
                _ => row.push((col, cell.cell.c.to_string(), fg, bg, flags)),
            }
        }
        let hex = |c: (u8, u8, u8)| format!("#{:02x}{:02x}{:02x}", c.0, c.1, c.2);
        let rows_json: Vec<serde_json::Value> = lines
            .into_iter()
            .map(|mut row| {
                // blanks with no background at a row's end are nothing
                while row.last().is_some_and(|r| r.3.is_none() && r.1.trim().is_empty()) {
                    row.pop();
                }
                serde_json::Value::from(row.into_iter().map(|(c, text, fg, bg, fl)| serde_json::json!([c, text, hex(fg), bg.map(hex), fl])).collect::<Vec<_>>())
            })
            .collect();
        let v = serde_json::json!({
            "id": id, "cols": cols, "rows": rows, "lines": rows_json,
            "cursor": if show_cursor { serde_json::json!([cursor.column.0, cursor.line.0]) } else { serde_json::Value::Null },
            "back": offset, "title": p.l.title.lock().unwrap().clone(), "exited": p.l.exited.load(Ordering::Relaxed),
        });
        Some((g, v.to_string()))
    })
    .flatten()
}

/// The panel's background, under an inverted cell with none of its own.
const BG: (u8, u8, u8) = (14, 14, 16);

/// A cell's colour: the named ones through void's palette (term.rs), the 256 indexed as xterm's (the first 16 named).
fn color(c: Color, foreground: bool) -> Option<(u8, u8, u8)> {
    let Color::Indexed(i) = c else { return crate::term::rgb(c, foreground) };
    const NAMED: [NamedColor; 16] = [
        NamedColor::Black, NamedColor::Red, NamedColor::Green, NamedColor::Yellow, NamedColor::Blue, NamedColor::Magenta, NamedColor::Cyan, NamedColor::White,
        NamedColor::BrightBlack, NamedColor::BrightRed, NamedColor::BrightGreen, NamedColor::BrightYellow, NamedColor::BrightBlue, NamedColor::BrightMagenta, NamedColor::BrightCyan, NamedColor::BrightWhite,
    ];
    Some(match i {
        0..=15 => return crate::term::rgb(Color::Named(NAMED[i as usize]), true),
        16..=231 => {
            let n = i - 16;
            let v = |x: u8| if x == 0 { 0 } else { 55 + 40 * x };
            (v(n / 36), v(n / 6 % 6), v(n % 6))
        }
        _ => {
            let g = 8 + 10 * (i - 232);
            (g, g, g)
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_are_a_terminals_bytes() {
        assert_eq!(key_bytes(0, "c", true, false), Some(vec![3]));
        assert_eq!(key_bytes(0, "a", false, false), Some(b"a".to_vec()));
        assert_eq!(key_bytes(0, "Enter", false, false), Some(b"\r".to_vec()));
        assert_eq!(key_bytes(0, "ArrowUp", false, false), Some(b"\x1b[A".to_vec()));
        assert_eq!(key_bytes(0, "b", false, true), Some(b"\x1bb".to_vec()));
        assert_eq!(key_bytes(0, "Shift", false, false), None);
        assert_eq!(cells(7.23 * 80.0 + 16.0, 16.0 * 24.0 + 16.0), (80, 24));
    }

    #[test]
    fn a_login_shell_runs_and_draws() {
        let id = 9_000_000 + std::process::id() as u64;
        size(id, 60, 10, Some(Path::new("/tmp"))).unwrap();
        input(id, b"echo raea-$((6*7))\r".to_vec());
        let mut seen = 0;
        let mut text = String::new();
        let t0 = std::time::Instant::now();
        while !text.contains("raea-42") {
            assert!(t0.elapsed() < std::time::Duration::from_secs(20), "no echo: {text}");
            if let Some((g, s)) = screen_since(id, seen) {
                seen = g;
                let v: serde_json::Value = serde_json::from_str(&s).unwrap();
                text = v["lines"].as_array().unwrap().iter().flat_map(|r| r.as_array().unwrap().iter().map(|run| run[1].as_str().unwrap().to_string())).collect::<Vec<_>>().join("\n");
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        size(id, 40, 8, None).unwrap();
        assert_eq!(screen_since(id, seen).map(|(_, s)| serde_json::from_str::<serde_json::Value>(&s).unwrap()["cols"].as_u64()), Some(Some(40)));
        close(id);
        assert!(!open().contains(&id));
    }
}
