//! A copy of `secaudit`'s TUI theme (that crate must not depend on OmaPhoenix
//! crates, ADR-0013). Colours from the live Omarchy theme (DESIGN.md §5.1, §12): truecolor when
//! the terminal says it has it, else the 16-colour palette Omarchy themes
//! also set. `NO_COLOR` turns colour off; nothing relies on colour alone.

use std::path::PathBuf;

use ratatui::style::{Color, Modifier, Style};

/// Resolved colours for the TUI.
#[derive(Clone, Copy, Debug)]
pub struct Theme {
    pub muted: Color,
    pub accent: Color,
    pub red: Color,
    pub yellow: Color,
    pub green: Color,
    pub selection: Color,
    pub color: bool,
}

impl Theme {
    /// Reads the theme without querying the terminal (no startup stalls).
    pub fn load() -> Self {
        if std::env::var_os("NO_COLOR").is_some_and(|v| !v.is_empty()) {
            return Self::plain();
        }
        let truecolor = std::env::var("COLORTERM").is_ok_and(|v| v == "truecolor" || v == "24bit");
        let table: toml::Table = theme_file()
            .and_then(|p| std::fs::read_to_string(p).ok())
            .and_then(|t| t.parse().ok())
            .unwrap_or_default();
        let pick = |keys: &[&str], ansi: Color| -> Color {
            if !truecolor {
                return ansi;
            }
            keys.iter()
                .find_map(|k| table.get(*k).and_then(toml::Value::as_str))
                .and_then(hex)
                .unwrap_or(ansi)
        };
        Self {
            muted: pick(&["muted", "color8"], Color::DarkGray),
            accent: pick(&["accent", "blue"], Color::Blue),
            red: pick(&["red"], Color::Red),
            yellow: pick(&["yellow"], Color::Yellow),
            green: pick(&["green"], Color::Green),
            selection: pick(&["selection", "lighter_background"], Color::DarkGray),
            color: true,
        }
    }

    fn plain() -> Self {
        Self {
            muted: Color::Reset,
            accent: Color::Reset,
            red: Color::Reset,
            yellow: Color::Reset,
            green: Color::Reset,
            selection: Color::Reset,
            color: false,
        }
    }

    #[allow(
        clippy::unused_self,
        reason = "a theme method, so call sites read t.fg(t.red)"
    )]
    pub fn fg(&self, c: Color) -> Style {
        Style::new().fg(c)
    }

    /// The highlighted row: reversed without colour, so it stays visible.
    pub fn selected(&self) -> Style {
        if self.color {
            Style::new().bg(self.selection).add_modifier(Modifier::BOLD)
        } else {
            Style::new().add_modifier(Modifier::REVERSED)
        }
    }

    #[allow(clippy::unused_self, reason = "a theme method, like fg")]
    pub fn bold(&self) -> Style {
        Style::new().add_modifier(Modifier::BOLD)
    }
}

fn theme_file() -> Option<PathBuf> {
    let state = std::env::var_os("XDG_STATE_HOME")
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/state")))?;
    Some(state.join("omarchy/current/theme/colors.toml"))
}

fn hex(s: &str) -> Option<Color> {
    let h = s.strip_prefix('#')?;
    if h.len() != 6 {
        return None;
    }
    let byte = |i: usize| h.get(i..i + 2).and_then(|b| u8::from_str_radix(b, 16).ok());
    Some(Color::Rgb(byte(0)?, byte(2)?, byte(4)?))
}

#[cfg(test)]
mod tests {
    use ratatui::style::Color;

    #[test]
    fn parses_hex() {
        assert_eq!(super::hex("#f38d70"), Some(Color::Rgb(0xf3, 0x8d, 0x70)));
        assert_eq!(super::hex("f38d70"), None);
        assert_eq!(super::hex("#fff"), None);
    }
}
