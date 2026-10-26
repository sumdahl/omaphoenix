//! The parts of `gum` that the gum `secaudit-ui` used, drawn inline the way gum 2 draws them.

use std::collections::HashMap;

mod choose;
mod color;
mod confirm;
mod frame;

pub use choose::choose;
use color::ansi256;
pub use color::hex;
pub use confirm::confirm;

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
