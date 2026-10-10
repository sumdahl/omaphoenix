//! `gum confirm`.

#![allow(
    clippy::similar_names,
    reason = "fg/bg pairs per gum style: p_fg, s_fg, u_fg"
)]

use std::io::{self, Write};

use ratatui::crossterm::event::KeyCode;
use ratatui::crossterm::terminal;

use super::Env;
use super::frame::{Frame, aborted, help, key};

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
