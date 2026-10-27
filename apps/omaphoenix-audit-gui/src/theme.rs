use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};
use std::{env, fs, thread};

use slint::{ComponentHandle, Weak};

use crate::ui::{App, Theme};

pub(crate) const TOKEN_MAP: &[(&str, &[&str])] = &[
    ("surface-base", &["background"]),
    ("surface-sunken", &["dark_background", "background"]),
    (
        "surface-deep",
        &["darker_background", "dark_background", "background"],
    ),
    (
        "surface-raised",
        &["lighter_background", "selection", "background"],
    ),
    ("text-primary", &["foreground"]),
    ("text-secondary", &["light_foreground", "foreground"]),
    ("text-muted", &["muted", "dark_foreground", "foreground"]),
    ("accent", &["accent", "blue"]),
    ("selection", &["selection", "lighter_background"]),
    ("state-risk", &["red"]),
    ("state-attention", &["yellow"]),
    ("state-fine", &["green"]),
    ("state-info", &["cyan", "blue"]),
];

pub(crate) fn theme_file() -> Option<PathBuf> {
    let state = env::var_os("XDG_STATE_HOME")
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .or_else(|| env::var_os("HOME").map(|h| Path::new(&h).join(".local/state")))?;
    Some(state.join("omarchy/current/theme/colors.toml"))
}

pub(crate) fn parse_hex(s: &str) -> Option<slint::Color> {
    let s = s.trim().strip_prefix('#')?;
    let byte = |i: usize| s.get(i..i + 2).and_then(|b| u8::from_str_radix(b, 16).ok());
    Some(slint::Color::from_rgb_u8(byte(0)?, byte(2)?, byte(4)?))
}

pub(crate) fn read_tokens(path: &Path) -> Option<BTreeMap<&'static str, slint::Color>> {
    let table: toml::Table = fs::read_to_string(path).ok()?.parse().ok()?;
    let hex = |k: &str| {
        table
            .get(k)
            .and_then(toml::Value::as_str)
            .and_then(parse_hex)
    };
    Some(
        TOKEN_MAP
            .iter()
            .filter_map(|(token, keys)| keys.iter().find_map(|k| hex(k)).map(|c| (*token, c)))
            .collect(),
    )
}

pub(crate) fn apply_theme(app: &App, tokens: &BTreeMap<&'static str, slint::Color>) {
    let t = app.global::<Theme>();
    for (k, c) in tokens {
        match *k {
            "surface-base" => t.set_surface_base(*c),
            "surface-sunken" => t.set_surface_sunken(*c),
            "surface-deep" => t.set_surface_deep(*c),
            "surface-raised" => t.set_surface_raised(*c),
            "text-primary" => t.set_text_primary(*c),
            "text-secondary" => t.set_text_secondary(*c),
            "text-muted" => t.set_text_muted(*c),
            "accent" => t.set_accent(*c),
            "selection" => t.set_selection(*c),
            "state-risk" => t.set_state_risk(*c),
            "state-attention" => t.set_state_attention(*c),
            "state-fine" => t.set_state_fine(*c),
            "state-info" => t.set_state_info(*c),
            _ => {}
        }
    }
}

/// Follows Omarchy theme switches (the `current/theme` symlink is swapped).
pub(crate) fn watch_theme(app: Weak<App>) {
    thread::spawn(move || {
        let stamp = |p: &Path| -> Option<(PathBuf, SystemTime)> {
            let real = fs::canonicalize(p).ok()?;
            Some((real.clone(), fs::metadata(&real).ok()?.modified().ok()?))
        };
        let Some(path) = theme_file() else { return };
        let mut last = stamp(&path);
        loop {
            thread::sleep(Duration::from_millis(400));
            let now = stamp(&path);
            if now == last {
                continue;
            }
            last = now;
            if let Some(tokens) = read_tokens(&path) {
                let app = app.clone();
                let _ = slint::invoke_from_event_loop(move || {
                    if let Some(app) = app.upgrade() {
                        apply_theme(&app, &tokens);
                    }
                });
            }
        }
    });
}
