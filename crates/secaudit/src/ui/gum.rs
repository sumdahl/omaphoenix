//! The parts of `gum` that `secaudit-ui` used (`choose`, `confirm`,
//! `style`), drawn inline the way gum 2 draws them: same layout, keys,
//! paging, help line and theme colours (`GUM_*` from the Omarchy theme),
//! with lipgloss's colour-profile downsampling.

#![allow(
    clippy::similar_names,
    reason = "fg/bg pairs per gum style: p_fg, s_fg, u_fg"
)]

use std::collections::HashMap;
use std::fmt::Write as _;
use std::io::{self, Write};

use ratatui::crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ratatui::crossterm::terminal;

/// What the terminal can show, as lipgloss detects it.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Profile {
    TrueColor,
    Ansi256,
    Plain,
}

/// Theme values: the process environment, overridden by the current
/// Omarchy theme's `gum_env.lua` (what `omarchy-restart-gum` exports).
pub struct Env {
    vars: HashMap<String, String>,
    pub profile: Profile,
}

impl Env {
    pub fn load() -> Self {
        let mut vars: HashMap<String, String> = std::env::vars().collect();
        if let Some(home) = std::env::var_os("HOME") {
            let path =
                std::path::Path::new(&home).join(".local/state/omarchy/current/theme/gum_env.lua");
            for line in std::fs::read_to_string(path).unwrap_or_default().lines() {
                // hl.env("KEY", "value")
                let parts: Vec<&str> = line.split('"').collect();
                if line.trim_start().starts_with("hl.env(")
                    && let [_, k, _, v, ..] = parts.as_slice()
                    && !k.is_empty()
                    && !v.is_empty()
                {
                    vars.insert((*k).to_owned(), (*v).to_owned());
                }
            }
        }
        let profile = detect(&vars);
        Self { vars, profile }
    }

    pub fn get(&self, k: &str) -> Option<&str> {
        self.vars
            .get(k)
            .map(String::as_str)
            .filter(|v| !v.is_empty())
    }

    /// Foreground escape for a `#rrggbb` (or palette index) colour.
    pub fn fg(&self, c: &str) -> String {
        self.colour(c, 38)
    }

    pub fn bg(&self, c: &str) -> String {
        self.colour(c, 48)
    }

    fn colour(&self, c: &str, layer: u8) -> String {
        if self.profile == Profile::Plain {
            return String::new();
        }
        if let Ok(n) = c.parse::<u8>() {
            return if n < 8 && layer == 38 {
                format!("\x1b[3{n}m")
            } else if n < 8 {
                format!("\x1b[4{n}m")
            } else {
                format!("\x1b[{layer};5;{n}m")
            };
        }
        let Some((r, g, b)) = hex(c) else {
            return String::new();
        };
        match self.profile {
            Profile::TrueColor => format!("\x1b[{layer};2;{r};{g};{b}m"),
            _ => format!("\x1b[{layer};5;{}m", ansi256(r, g, b)),
        }
    }

    /// A gum style: `<prefix>_FOREGROUND` / `_BACKGROUND`.
    fn style(&self, prefix: &str, fg_default: &str) -> (String, String) {
        let fg = self
            .get(&format!("{prefix}_FOREGROUND"))
            .unwrap_or(fg_default)
            .to_owned();
        let bg = self
            .get(&format!("{prefix}_BACKGROUND"))
            .map(|b| self.bg(b))
            .unwrap_or_default();
        (self.fg(&fg), bg)
    }
}

/// charmbracelet/colorprofile's detection, as gum 2 runs it.
/// ponytail: the 16-colour profile is treated as 256; Omarchy terminals
/// all advertise more.
fn detect(env: &HashMap<String, String>) -> Profile {
    let get = |k: &str| env.get(k).map_or("", String::as_str);
    let term = get("TERM");
    if term.is_empty() || term == "dumb" || !get("NO_COLOR").is_empty() {
        return Profile::Plain;
    }
    let muxed = term.starts_with("tmux") || term.starts_with("screen");
    let truecolor = [
        "alacritty",
        "contour",
        "foot",
        "ghostty",
        "kitty",
        "rio",
        "st",
        "wezterm",
    ]
    .iter()
    .any(|t| term.contains(t))
        || (matches!(
            get("COLORTERM").to_ascii_lowercase().as_str(),
            "truecolor" | "24bit" | "yes" | "true"
        ) && !muxed)
        || term.ends_with("direct");
    if truecolor {
        return Profile::TrueColor;
    }
    // Inside tmux: truecolor only when tmux says it has Tc or RGB.
    if !get("TMUX").is_empty()
        && std::process::Command::new("tmux")
            .arg("info")
            .output()
            .is_ok_and(|o| {
                String::from_utf8_lossy(&o.stdout)
                    .lines()
                    .any(|l| (l.contains("Tc") || l.contains("RGB")) && l.contains("true"))
            })
    {
        return Profile::TrueColor;
    }
    Profile::Ansi256
}

pub fn hex(c: &str) -> Option<(u8, u8, u8)> {
    let h = c.trim().strip_prefix('#')?;
    if h.len() != 6 {
        return None;
    }
    let byte = |i: usize| h.get(i..i + 2).and_then(|b| u8::from_str_radix(b, 16).ok());
    Some((byte(0)?, byte(2)?, byte(4)?))
}

/// `termenv`'s truecolor → 256 mapping: the nearer of the 6×6×6 cube and the
/// grey ramp (distance in RGB; `termenv` uses `HSLuv`, same picks for themes).
pub fn ansi256(r: u8, g: u8, b: u8) -> u8 {
    let v2ci = |v: u8| match v {
        0..48 => 0u8,
        48..115 => 1,
        _ => (v - 35) / 40,
    };
    let (ri, gi, bi) = (v2ci(r), v2ci(g), v2ci(b));
    let i2cv = [0u8, 0x5f, 0x87, 0xaf, 0xd7, 0xff];
    let cv = |i: u8| i2cv.get(usize::from(i)).copied().unwrap_or(0);
    let avg = (u16::from(r) + u16::from(g) + u16::from(b)) / 3;
    let gray_idx = if avg > 238 {
        23
    } else {
        avg.saturating_sub(3) / 10
    };
    let gv = 8 + 10 * gray_idx;
    let dist = |a: (u16, u16, u16)| {
        let d = |x: u16, y: u8| (i32::from(x) - i32::from(y)).pow(2);
        d(a.0, r) + d(a.1, g) + d(a.2, b)
    };
    let cube = dist((u16::from(cv(ri)), u16::from(cv(gi)), u16::from(cv(bi))));
    let grey = dist((gv, gv, gv));
    if cube <= grey {
        16 + 36 * ri + 6 * gi + bi
    } else {
        232 + u8::try_from(gray_idx).unwrap_or(23)
    }
}

const N: &str = "\x1b[39m\x1b[49m";

/// Removes ANSI escapes, as gum does with item labels.
pub fn strip(s: &str) -> String {
    let mut out = String::new();
    let mut it = s.chars();
    while let Some(c) = it.next() {
        if c == '\x1b' {
            for c in it.by_ref() {
                if c.is_ascii_alphabetic() {
                    break;
                }
            }
        } else {
            out.push(c);
        }
    }
    out
}

/// Inline frame: redraws over its own previous lines, erases on exit.
struct Frame {
    lines: usize,
}

impl Frame {
    fn draw(&mut self, text: &[String]) {
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

    fn erase(&self) {
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
fn key() -> Option<KeyEvent> {
    loop {
        match event::read() {
            Ok(Event::Key(k)) if k.kind == KeyEventKind::Press => return Some(k),
            Ok(_) => {}
            Err(_) => return None,
        }
    }
}

fn help(env: &Env, pairs: &[(&str, &str)]) -> String {
    let (k, d, sep) = (env.fg("#626262"), env.fg("#4A4A4A"), env.fg("#3C3C3C"));
    pairs
        .iter()
        .map(|(key, desc)| format!("{k}{key}\x1b[39m {d}{desc}"))
        .collect::<Vec<_>>()
        .join(&format!("{sep} • "))
        + "\x1b[39m"
}

fn aborted(k: &KeyEvent) -> bool {
    k.code == KeyCode::Esc
        || (k.modifiers.contains(KeyModifiers::CONTROL) && k.code == KeyCode::Char('c'))
}

/// `gum choose --label-delimiter=SEP --header H --height N --cursor "  ▸ "`:
/// `items` are (label, value); returns the chosen value, None on escape.
pub fn choose(
    env: &Env,
    header: &str,
    height: usize,
    items: &[(String, String)],
) -> Option<String> {
    if items.is_empty() {
        return None;
    }
    let height = height.max(1);
    let (head_fg, head_bg) = env.style("GUM_CHOOSE_HEADER", "99");
    let (cur_fg, cur_bg) = env.style("GUM_CHOOSE_CURSOR", "212");
    let (item_fg, item_bg) = env.style("GUM_CHOOSE_ITEM", "255");
    let item_hex = env
        .get("GUM_CHOOSE_ITEM_FOREGROUND")
        .unwrap_or("255")
        .to_owned();
    let pages = items.len().div_ceil(height);
    let mut sel = 0usize;
    let _ = terminal::enable_raw_mode();
    let _ = io::stdout().write_all(b"\x1b[?25l");
    let mut frame = Frame { lines: 0 };
    let result = loop {
        let page = sel / height;
        let mut text = Vec::new();
        if !header.is_empty() {
            text.push(format!("{head_fg}{head_bg}{}{N}", strip(header)));
        }
        for (i, (label, _)) in items.iter().enumerate().skip(page * height).take(height) {
            let label = strip(label);
            if i == sel {
                text.push(format!("{cur_fg}{cur_bg}  ▸ {label}{N}"));
            } else {
                text.push(format!("    {item_fg}{item_bg}{label}{N}"));
            }
        }
        text.push(String::new());
        if pages > 1 {
            let dots: String = (0..pages)
                .map(|p| {
                    if p == page {
                        format!("{}•", env.fg("#847A85"))
                    } else {
                        format!("{}•", env.fg(&item_hex))
                    }
                })
                .collect();
            text.push(format!("  {dots}\x1b[39m"));
            text.push(String::new());
        } else {
            text.push(String::new());
        }
        text.push(help(env, &[("←↓↑→", "navigate"), ("enter", "submit")]));
        frame.draw(&text);
        let Some(k) = key() else { break None };
        let n = items.len();
        match k.code {
            _ if aborted(&k) => break None,
            KeyCode::Enter => break items.get(sel).map(|i| i.1.clone()),
            KeyCode::Up | KeyCode::Char('k') => sel = (sel + n - 1) % n,
            KeyCode::Down | KeyCode::Char('j') => sel = (sel + 1) % n,
            KeyCode::Char('p') if k.modifiers.contains(KeyModifiers::CONTROL) => {
                sel = (sel + n - 1) % n;
            }
            KeyCode::Char('n') if k.modifiers.contains(KeyModifiers::CONTROL) => {
                sel = (sel + 1) % n;
            }
            KeyCode::Left | KeyCode::Char('h') | KeyCode::PageUp => {
                sel = sel.saturating_sub(height);
            }
            KeyCode::Right | KeyCode::Char('l') | KeyCode::PageDown => {
                sel = (sel + height).min(n - 1);
            }
            KeyCode::Home | KeyCode::Char('g') => sel = 0,
            KeyCode::End | KeyCode::Char('G') => sel = n - 1,
            _ => {}
        }
    };
    frame.erase();
    let _ = io::stdout().write_all(b"\x1b[?25h");
    let _ = terminal::disable_raw_mode();
    result
}

/// `gum confirm PROMPT --affirmative A --negative B --default=false`.
pub fn confirm(env: &Env, prompt: &str, yes: &str, no: &str) -> bool {
    let (p_fg, p_bg) = env.style("GUM_CONFIRM_PROMPT", "#7571F9");
    let (s_fg, s_bg) = env.style("GUM_CONFIRM_SELECTED", "230");
    let (u_fg, u_bg) = env.style("GUM_CONFIRM_UNSELECTED", "254");
    let mut on_yes = false;
    let _ = terminal::enable_raw_mode();
    let _ = io::stdout().write_all(b"\x1b[?25l");
    let mut frame = Frame { lines: 0 };
    let result = loop {
        let button = |label: &str, selected: bool| {
            let (fg, bg) = if selected {
                (&s_fg, &s_bg)
            } else {
                (&u_fg, &u_bg)
            };
            format!("{bg}   {fg}{label}\x1b[39m   \x1b[49m")
        };
        let text = vec![
            format!(" \x1b[1m{p_fg}{p_bg}{prompt}\x1b[0m"),
            String::new(),
            format!(" {}  {}", button(yes, on_yes), button(no, !on_yes)),
            String::new(),
            help(
                env,
                &[("←→", "toggle"), ("enter", "submit"), ("y", yes), ("n", no)],
            ),
        ];
        frame.draw(&text);
        let Some(k) = key() else { break false };
        match k.code {
            _ if aborted(&k) => break false,
            KeyCode::Enter => break on_yes,
            KeyCode::Char('y' | 'Y') => break true,
            KeyCode::Char('n' | 'N') => break false,
            KeyCode::Left
            | KeyCode::Right
            | KeyCode::Char('h' | 'l')
            | KeyCode::Tab
            | KeyCode::BackTab => on_yes = !on_yes,
            _ => {}
        }
    };
    frame.erase();
    let _ = io::stdout().write_all(b"\x1b[?25h");
    let _ = terminal::disable_raw_mode();
    result
}

#[cfg(test)]
mod tests {
    #[test]
    fn downsamples_like_termenv() {
        assert_eq!(super::ansi256(0xf3, 0x8d, 0x70), 209, "accent");
        assert_eq!(super::ansi256(0x2c, 0x25, 0x25), 235, "background");
        assert_eq!(super::ansi256(0xe6, 0xd9, 0xdb), 253, "foreground");
        assert_eq!(super::ansi256(0x40, 0x3e, 0x41), 238, "selected");
        assert_eq!(super::ansi256(0x62, 0x62, 0x62), 241, "help key");
        assert_eq!(super::ansi256(0x4a, 0x4a, 0x4a), 239, "help text");
        assert_eq!(super::ansi256(0x3c, 0x3c, 0x3c), 237, "help separator");
    }
}
