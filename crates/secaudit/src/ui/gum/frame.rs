//! The inline frame, key reading and help line shared by `choose` and `confirm`.

use std::fmt::Write as _;
use std::io::{self, Write};

use ratatui::crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ratatui::crossterm::terminal;

use super::Env;

pub(super) const N: &str = "\x1b[39m\x1b[49m";

/// Inline frame: redraws over its own previous lines, erases on exit.
pub(super) struct Frame {
    pub(super) lines: usize,
}

impl Frame {
    pub(super) fn draw(&mut self, text: &[String]) {
        let mut out = io::stdout();
        let cols = terminal::size().map_or(80, |(c, _)| usize::from(c));
        let mut s = String::from("\r");
        if self.lines > 1 {
            let _ = write!(s, "\x1b[{}A", self.lines - 1);
        }
        s.push_str("\x1b[J");
        for (i, l) in text.iter().enumerate() {
            if i > 0 {
                s.push_str("\r\n");
            }
            s.push_str(&clip_visible(l, cols));
        }
        self.lines = text.len().max(1);
        let _ = out.write_all(s.as_bytes());
        let _ = out.flush();
    }

    pub(super) fn erase(&self) {
        let mut s = String::from("\r");
        if self.lines > 1 {
            let _ = write!(s, "\x1b[{}A", self.lines - 1);
        }
        s.push_str("\x1b[J");
        let mut out = io::stdout();
        let _ = out.write_all(s.as_bytes());
        let _ = out.flush();
    }
}

/// Cuts a styled line to `cols` visible characters.
fn clip_visible(l: &str, cols: usize) -> String {
    let mut out = String::new();
    let mut seen = 0;
    let mut it = l.chars().peekable();
    while let Some(c) = it.next() {
        if c == '\x1b' {
            out.push(c);
            for c in it.by_ref() {
                out.push(c);
                if c.is_ascii_alphabetic() {
                    break;
                }
            }
        } else if seen < cols {
            out.push(c);
            seen += 1;
        }
    }
    out
}

/// Waits for a key press; ctrl+c comes back as a key in raw mode.
pub(super) fn key() -> Option<KeyEvent> {
    loop {
        match event::read() {
            Ok(Event::Key(k)) if k.kind == KeyEventKind::Press => return Some(k),
            Ok(_) => {}
            Err(_) => return None,
        }
    }
}

pub(super) fn help(env: &Env, pairs: &[(&str, &str)]) -> String {
    let (k, d, sep) = (env.fg("#626262"), env.fg("#4A4A4A"), env.fg("#3C3C3C"));
    pairs
        .iter()
        .map(|(key, desc)| format!("{k}{key}\x1b[39m {d}{desc}"))
        .collect::<Vec<_>>()
        .join(&format!("{sep} • "))
        + "\x1b[39m"
}

pub(super) fn aborted(k: &KeyEvent) -> bool {
    k.code == KeyCode::Esc
        || (k.modifiers.contains(KeyModifiers::CONTROL) && k.code == KeyCode::Char('c'))
}
