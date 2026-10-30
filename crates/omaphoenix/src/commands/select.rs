use std::io::{self, IsTerminal};

use phx_core::App;

use crate::out::say;
use crate::tui::{self, Section};

/// Opens Backup to choose what to capture; `all` keeps everything detected instead.
pub fn run(app: &App<'_>, all: bool) -> Result<(), String> {
    if !app.repo.exists() {
        return Err("not set up yet; run: omaphoenix init".into());
    }
    if all {
        return keep_everything(app);
    }
    if !io::stdin().is_terminal() || !io::stdout().is_terminal() {
        return Err(
            "choosing needs a terminal; use omaphoenix select --all to keep everything detected"
                .into(),
        );
    }
    tui::run(app, Some(Section::Backup)).map_err(|e| e.to_string())
}

pub(crate) fn keep_everything(app: &App<'_>) -> Result<(), String> {
    let mut choices: Vec<_> = app
        .choices()
        .map_err(|e| e.to_string())?
        .into_iter()
        .map(|(c, _)| c)
        .collect();
    for c in &mut choices {
        // Secrets stay opt-in (`secrets init`), even here.
        if c.category != "secret" {
            c.items.iter_mut().for_each(|(_, on)| *on = true);
        }
    }
    app.save_choices(&choices).map_err(|e| e.to_string())?;
    let n: usize = choices
        .iter()
        .map(|c| c.items.iter().filter(|(_, on)| *on).count())
        .sum();
    say(&format!(
        "✓ snapshots capture all {n} detected item(s); change it any time with: omaphoenix select"
    ));
    Ok(())
}
