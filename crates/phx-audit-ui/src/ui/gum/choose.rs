//! `gum choose`.

use std::io::{self, Write};

use ratatui::crossterm::event::{KeyCode, KeyModifiers};
use ratatui::crossterm::terminal;

use super::Env;
use super::color::strip;
use super::frame::{Frame, N, aborted, help, key};

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
