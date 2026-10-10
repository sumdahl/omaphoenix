//! The palette of the Python `secaudit-tui`, read the same way from the
//! active Omarchy theme, plus the shades Textual derives from it (captured
//! from the original's screen and matched as blends).

use ratatui::style::{Color, Modifier, Style};

/// Colours, as `load_palette()` in the Python app picks them.
#[derive(Clone, Copy, Debug)]
pub struct Pal {
    pub accent: Color,
    pub fg: Color,
    pub bg: Color,
    pub panel: Color,
    pub muted: Color,
    pub red: Color,
    pub yellow: Color,
    pub green: Color,
    /// Textual's `$text-muted` (captions).
    pub text_muted: Color,
    /// Inactive tab labels.
    pub tab_dim: Color,
    /// The tab underline's background bar.
    pub bar_dim: Color,
    /// A highlighted list item when the list isn't focused.
    pub highlight: Color,
    /// `DataTable` header background.
    pub header_bg: Color,
    /// `$panel-lighten-2`: compact buttons.
    pub button_bg: Color,
    /// `$boost`: code boxes.
    pub boost: Color,
    /// Disabled footer keys and their labels.
    pub key_dim: Color,
    pub label_dim: Color,
    /// The footer's `▏` before `^p palette`.
    pub divider: Color,
    /// `Input`: background, blurred border, placeholder; focused background.
    pub input_focus: Color,
    pub input_bg: Color,
    pub input_border: Color,
    pub placeholder: Color,
    /// `$surface`: dialogs.
    pub surface: Color,
    /// Scrollbar track and thumb (approximations of Textual's).
    pub track: Color,
    pub thumb: Color,
    pub color: bool,
}

fn rgb(c: Color) -> (f32, f32, f32) {
    match c {
        Color::Rgb(r, g, b) => (f32::from(r), f32::from(g), f32::from(b)),
        _ => (128.0, 128.0, 128.0),
    }
}

/// `a` mixed into `b`: `t` of `a`, `1 - t` of `b`.
pub fn blend(a: Color, b: Color, t: f32) -> Color {
    let ((ar, ag, ab), (br, bg, bb)) = (rgb(a), rgb(b));
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "0..=255 by construction"
    )]
    // Truncated, as Textual's `Color.blend` does.
    let mix = |x: f32, y: f32| (y + (x - y) * t).clamp(0.0, 255.0) as u8;
    Color::Rgb(mix(ar, br), mix(ag, bg), mix(ab, bb))
}

/// Each channel moved by `d`, as Textual's lighten/darken comes out on
/// these near-grey shades.
pub(crate) fn shade(c: Color, d: i16) -> Color {
    let (red, green, blue) = rgb(c);
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "clamped to 0..=255"
    )]
    let f = |x: f32| (x as i16 + d).clamp(0, 255) as u8;
    Color::Rgb(f(red), f(green), f(blue))
}

fn hex(s: &str) -> Option<Color> {
    let h = s.trim().strip_prefix('#')?;
    let byte = |i: usize| h.get(i..i + 2).and_then(|b| u8::from_str_radix(b, 16).ok());
    Some(Color::Rgb(byte(0)?, byte(2)?, byte(4)?))
}

impl Pal {
    /// `~/.local/state/omarchy/current/theme/colors.toml`, like the original
    /// (which reads it from the home directory, not `XDG_STATE_HOME`).
    pub fn load() -> Self {
        let table: toml::Table = std::env::var_os("HOME")
            .map(|h| {
                std::path::Path::new(&h).join(".local/state/omarchy/current/theme/colors.toml")
            })
            .and_then(|p| std::fs::read_to_string(p).ok())
            .and_then(|t| t.parse().ok())
            .unwrap_or_default();
        let get = |k: &str| table.get(k).and_then(toml::Value::as_str).and_then(hex);
        let dark = table.get("mode").and_then(toml::Value::as_str) != Some("light");
        let pick = |k: &str, d: &str| get(k).or_else(|| hex(d)).unwrap_or(Color::Reset);
        let accent = pick("accent", "#4b8ef4");
        let fg = pick("foreground", "#e6e6e6");
        let bg = pick("background", "#101010");
        let panel = pick("lighter_bg", "#1d1d1d");
        let mut p = Self {
            accent,
            fg,
            bg,
            panel,
            muted: pick("muted", "#6b6b6b"),
            // Bright variants read better as status colours on dark themes.
            red: get(if dark { "bright_red" } else { "red" })
                .unwrap_or_else(|| pick("", "#e06c75")),
            yellow: get(if dark { "bright_yellow" } else { "yellow" })
                .unwrap_or_else(|| pick("", "#e5c07b")),
            green: pick("green", "#98c379"),
            text_muted: blend(
                if dark {
                    Color::Rgb(255, 255, 255)
                } else {
                    Color::Rgb(0, 0, 0)
                },
                bg,
                0.6,
            ),
            tab_dim: blend(fg, bg, 0.5),
            bar_dim: blend(fg, bg, 0.1),
            // Fitted to the capture: Textual's 30% comes out a hair under.
            highlight: blend(accent, bg, 0.297),
            header_bg: blend(fg, panel, 0.05),
            button_bg: shade(panel, 32),
            boost: blend(Color::Rgb(255, 255, 255), bg, 0.04),
            key_dim: blend(accent, panel, 0.66),
            label_dim: blend(fg, panel, 0.66),
            divider: blend(fg, panel, 0.2),
            input_bg: shade(panel, 1),
            input_focus: blend(fg, shade(panel, 1), 0.05),
            input_border: shade(panel, -4),
            placeholder: blend(Color::Rgb(255, 255, 255), shade(panel, 1), 0.38),
            surface: shade(panel, 1),
            track: shade(bg, -16),
            thumb: blend(accent, shade(bg, -16), 0.4),
            color: true,
        };
        if std::env::var_os("NO_COLOR").is_some_and(|v| !v.is_empty()) {
            p = p.plain();
        }
        p
    }

    fn plain(mut self) -> Self {
        for c in [
            &mut self.accent,
            &mut self.fg,
            &mut self.bg,
            &mut self.panel,
            &mut self.muted,
            &mut self.red,
            &mut self.yellow,
            &mut self.green,
            &mut self.text_muted,
            &mut self.tab_dim,
            &mut self.bar_dim,
            &mut self.highlight,
            &mut self.header_bg,
            &mut self.button_bg,
            &mut self.boost,
            &mut self.key_dim,
            &mut self.label_dim,
            &mut self.divider,
            &mut self.input_bg,
            &mut self.input_focus,
            &mut self.input_border,
            &mut self.placeholder,
            &mut self.surface,
            &mut self.track,
            &mut self.thumb,
        ] {
            *c = Color::Reset;
        }
        self.color = false;
        self
    }

    #[allow(
        clippy::unused_self,
        reason = "reads as p.fg_(…) beside the palette's colours"
    )]
    pub fn fg_(&self, c: Color) -> Style {
        Style::new().fg(c)
    }

    #[allow(
        clippy::unused_self,
        reason = "reads as p.bold(…) beside the palette's colours"
    )]
    pub fn bold(&self, c: Color) -> Style {
        Style::new().fg(c).add_modifier(Modifier::BOLD)
    }

    /// Selected row: accent background (reverse without colour).
    pub fn cursor(&self) -> Style {
        if self.color {
            Style::new()
                .bg(self.accent)
                .fg(blend(Color::Rgb(0, 0, 0), self.accent, 0.87))
                .add_modifier(Modifier::BOLD)
        } else {
            Style::new().add_modifier(Modifier::REVERSED)
        }
    }

    /// `$text-muted` over a given background: 60% of the text colour.
    pub fn muted_on(&self, bg: Color) -> Color {
        if self.color {
            blend(Color::Rgb(255, 255, 255), bg, 0.6)
        } else {
            Color::Reset
        }
    }

    /// Grade colour, as `grade_color()`.
    pub fn grade(&self, grade: char) -> Color {
        match grade {
            'A' | 'B' => self.green,
            'C' | 'D' => self.yellow,
            _ => self.red,
        }
    }

    /// Severity colour, as `SEV[sev][2]`.
    pub fn sev(&self, sev: &str) -> Color {
        match sev {
            "fail" => self.red,
            "warn" => self.yellow,
            "pass" => self.green,
            "skip" => self.muted,
            _ => self.accent,
        }
    }
}

/// `(icon, label)` per severity, as `SEV`.
pub fn sev_icon_label(sev: &str) -> (&'static str, &'static str) {
    match sev {
        "fail" => ("✗", "FAIL"),
        "warn" => ("!", "WARN"),
        "pass" => ("✓", "PASS"),
        "skip" => ("–", "SKIP"),
        _ => ("i", "NOTE"),
    }
}

#[cfg(test)]
mod tests {
    use ratatui::style::Color;

    #[test]
    fn blends_match_the_captured_textual_shades() {
        let (fg, bg, accent) = (
            Color::Rgb(230, 217, 219),
            Color::Rgb(44, 37, 37),
            Color::Rgb(243, 141, 112),
        );
        assert_eq!(
            super::blend(fg, bg, 0.5),
            Color::Rgb(137, 127, 128),
            "inactive tab"
        );
        assert_eq!(
            super::blend(fg, bg, 0.1),
            Color::Rgb(62, 55, 55),
            "underline bar"
        );
        assert_eq!(
            super::blend(accent, bg, 0.297),
            Color::Rgb(103, 67, 59),
            "list highlight"
        );
        let panel = Color::Rgb(29, 29, 29);
        assert_eq!(
            super::blend(fg, panel, 0.05),
            Color::Rgb(39, 38, 38),
            "table header"
        );
        assert_eq!(
            super::blend(accent, panel, 0.66),
            Color::Rgb(170, 102, 83),
            "disabled footer key"
        );
        assert_eq!(
            super::blend(accent, super::shade(bg, -16), 0.4),
            Color::Rgb(114, 69, 57),
            "scrollbar thumb"
        );
    }
}
