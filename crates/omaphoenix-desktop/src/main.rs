//! `OmaPhoenix` desktop app: GUI-stack spike (issue #12).
//!
//! The spike answers one question (Q9): can Tauri 2 meet DESIGN.md? The Rust
//! side does what the web side can't: it reads the live Omarchy theme and
//! font, follows theme switches, and reports whether the desktop wants
//! reduced motion. A bench mode records frame timings from scripted
//! interactions and writes them to a file, so results are repeatable.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, SystemTime};
use std::{env, fs, thread};

use serde::Serialize;
use tauri::{AppHandle, Emitter};

/// Design tokens (DESIGN.md §5.1) resolved from Omarchy's `colors.toml`.
#[derive(Serialize, Clone, Debug, PartialEq, Eq)]
struct Theme {
    /// `dark` or `light`, from the theme's `mode` key.
    mode: String,
    /// CSS custom property name → colour, e.g. `--surface-base` → `#2c2525`.
    tokens: BTreeMap<String, String>,
    /// The user's Omarchy font family.
    font: String,
}

/// DESIGN.md §5.1: design token ← theme key, with fallbacks for themes that
/// don't define every key.
const TOKEN_MAP: &[(&str, &[&str])] = &[
    ("--surface-base", &["background"]),
    ("--surface-sunken", &["dark_background", "background"]),
    (
        "--surface-deep",
        &["darker_background", "dark_background", "background"],
    ),
    (
        "--surface-raised",
        &["lighter_background", "selection", "background"],
    ),
    ("--text-primary", &["foreground"]),
    ("--text-secondary", &["light_foreground", "foreground"]),
    ("--text-muted", &["muted", "dark_foreground", "foreground"]),
    ("--accent", &["accent", "blue"]),
    ("--selection", &["selection", "lighter_background"]),
    ("--state-risk", &["red"]),
    ("--state-attention", &["yellow"]),
    ("--state-fine", &["green"]),
    ("--state-info", &["cyan", "blue"]),
];

/// Where the live theme lives. `OMAPHOENIX_SPIKE_THEME` points the bench at
/// a scratch copy, so tests can switch "themes" without touching the user's.
fn theme_file() -> Option<PathBuf> {
    if let Ok(p) = env::var("OMAPHOENIX_SPIKE_THEME") {
        return Some(PathBuf::from(p));
    }
    let state = env::var_os("XDG_STATE_HOME")
        .map(PathBuf::from)
        .or_else(|| env::var_os("HOME").map(|h| Path::new(&h).join(".local/state")))?;
    Some(state.join("omarchy/current/theme/colors.toml"))
}

fn read_theme(path: &Path) -> Option<Theme> {
    let text = fs::read_to_string(path).ok()?;
    let table: toml::Table = text.parse().ok()?;
    let get = |k: &str| {
        table
            .get(k)
            .and_then(toml::Value::as_str)
            .map(str::to_owned)
    };
    let tokens = TOKEN_MAP
        .iter()
        .filter_map(|(token, keys)| {
            keys.iter()
                .find_map(|k| get(k))
                .map(|v| ((*token).to_owned(), v))
        })
        .collect();
    Some(Theme {
        mode: get("mode").unwrap_or_else(|| "dark".to_owned()),
        tokens,
        font: omarchy_font(),
    })
}

fn omarchy_font() -> String {
    Command::new("omarchy-font-current")
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_owned())
        .filter(|f| !f.is_empty())
        .unwrap_or_else(|| "monospace".to_owned())
}

/// The theme as it is right now; `None` when Omarchy isn't present.
#[tauri::command]
fn theme() -> Option<Theme> {
    read_theme(&theme_file()?)
}

/// Reduced motion when Hyprland animations are off (DESIGN.md §6.1, law 6).
/// The web side also honours `prefers-reduced-motion` itself.
#[tauri::command]
fn reduced_motion() -> bool {
    Command::new("hyprctl")
        .args(["getoption", "animations:enabled", "-j"])
        .output()
        .ok()
        .and_then(|o| serde_json::from_slice::<serde_json::Value>(&o.stdout).ok())
        .and_then(|v| {
            v.get("int")
                .and_then(serde_json::Value::as_i64)
                .or_else(|| {
                    v.get("bool")
                        .and_then(serde_json::Value::as_bool)
                        .map(i64::from)
                })
        })
        .is_some_and(|enabled| enabled == 0)
}

#[tauri::command]
fn bench_mode() -> bool {
    env::var_os("OMAPHOENIX_SPIKE_BENCH").is_some()
}

/// Writes the bench report where the harness asked for it, then exits.
#[tauri::command]
#[allow(
    clippy::needless_pass_by_value,
    reason = "Tauri commands receive owned, deserialized arguments"
)]
fn bench_report(app: AppHandle, report: serde_json::Value) -> Result<(), String> {
    let path = env::var("OMAPHOENIX_SPIKE_REPORT")
        .map_err(|_| "OMAPHOENIX_SPIKE_REPORT is not set".to_owned())?;
    let body = serde_json::to_vec_pretty(&report).map_err(|e| e.to_string())?;
    fs::write(&path, body).map_err(|e| format!("write {path}: {e}"))?;
    app.exit(0);
    Ok(())
}

/// Polls the theme file and emits `theme-changed` when Omarchy switches
/// theme. Omarchy swaps the `current/theme` symlink, so both the resolved
/// path and the file's modification time are compared.
fn watch_theme(app: AppHandle) {
    thread::spawn(move || {
        let stamp = |p: &Path| -> Option<(PathBuf, SystemTime)> {
            let real = fs::canonicalize(p).ok()?;
            let modified = fs::metadata(&real).ok()?.modified().ok()?;
            Some((real, modified))
        };
        let Some(path) = theme_file() else { return };
        let mut last = stamp(&path);
        loop {
            thread::sleep(Duration::from_millis(300));
            let now = stamp(&path);
            if now != last {
                last = now;
                if let Some(theme) = read_theme(&path) {
                    let _ = app.emit("theme-changed", theme);
                }
            }
        }
    });
}

fn main() {
    let result = tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            theme,
            reduced_motion,
            bench_mode,
            bench_report
        ])
        .setup(|app| {
            watch_theme(app.handle().clone());
            Ok(())
        })
        .run(tauri::generate_context!());
    if let Err(e) = result {
        eprintln!("omaphoenix-desktop: {e}");
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::read_theme;
    use std::fs;
    use std::sync::atomic::{AtomicUsize, Ordering};

    /// Writes `toml` to a unique scratch file and reads it back as a theme.
    fn theme_from(toml: &str) -> super::Theme {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let n = NEXT.fetch_add(1, Ordering::Relaxed);
        let file =
            std::env::temp_dir().join(format!("omaphoenix-theme-{}-{n}.toml", std::process::id()));
        fs::write(&file, toml).ok();
        let theme = read_theme(&file);
        fs::remove_file(&file).ok();
        theme.unwrap_or_else(|| super::Theme {
            mode: String::new(),
            tokens: std::collections::BTreeMap::new(),
            font: String::new(),
        })
    }

    #[test]
    fn maps_every_design_token_from_a_full_theme() {
        let t = theme_from(
            r##"mode = "light"
accent = "#f38d70"
background = "#2c2525"
dark_background = "#211b1b"
darker_background = "#181414"
lighter_background = "#3d2f2a"
foreground = "#e6d9db"
light_foreground = "#c3b7b8"
muted = "#72696a"
selection = "#403e41"
red = "#fd6883"
yellow = "#f9cc6c"
green = "#adda78"
cyan = "#85dacc"
"##,
        );
        assert_eq!(t.mode, "light");
        assert_eq!(t.tokens.len(), super::TOKEN_MAP.len());
        assert_eq!(
            t.tokens.get("--surface-deep").map(String::as_str),
            Some("#181414")
        );
        assert_eq!(
            t.tokens.get("--accent").map(String::as_str),
            Some("#f38d70")
        );
    }

    #[test]
    fn falls_back_for_themes_that_leave_keys_out() {
        let t = theme_from(
            r##"background = "#101010"
foreground = "#eeeeee"
blue = "#3388ff"
"##,
        );
        assert_eq!(t.mode, "dark", "mode defaults to dark");
        assert_eq!(
            t.tokens.get("--surface-deep").map(String::as_str),
            Some("#101010")
        );
        assert_eq!(
            t.tokens.get("--text-muted").map(String::as_str),
            Some("#eeeeee")
        );
        assert_eq!(
            t.tokens.get("--accent").map(String::as_str),
            Some("#3388ff")
        );
        assert!(
            !t.tokens.contains_key("--state-risk"),
            "no invented colours"
        );
    }
}
