use std::io::{self, IsTerminal};

use phx_core::{App, sys};

use crate::out::say;
use crate::tui::{self, Section};

pub fn run(app: &App<'_>, mut remote: String, github: &str) -> Result<(), String> {
    // Checked before `gh repo create`, which would otherwise leave an orphan repo.
    if app.repo.exists() {
        return Err(format!(
            "already initialised at {}; to add a remote: git -C {} remote add origin <url>",
            app.repo.dir.display(),
            app.repo.dir.display()
        ));
    }
    if !github.is_empty() {
        let url = sys::output(
            None,
            &[
                "gh",
                "repo",
                "create",
                github,
                "--private",
                "--description",
                "omaphoenix snapshots",
            ],
        )
        .map_err(|e| e.to_string())?;
        remote = format!("{}.git", url.trim());
    }
    app.init(&remote).map_err(|e| e.to_string())?;
    say(&format!("initialised {}", app.repo.dir.display()));
    if !remote.is_empty() {
        say(&format!("remote      {remote}"));
    }
    if io::stdin().is_terminal() && io::stdout().is_terminal() {
        say("next: choose what to back up (opening Backup; w saves, q quits)");
        loop {
            tui::run(app, Some(Section::Backup)).map_err(|e| e.to_string())?;
            // Closed without saving: make the default an explicit choice, not a surprise.
            if app.has_chosen() {
                return Ok(());
            }
            if keep_all_answer(&mut io::stdin().lock(), &mut io::stdout())? {
                return super::select::keep_everything(app);
            }
        }
    }
    say("next: omaphoenix select, to choose what to back up");
    Ok(())
}

/// Asks whether to back up everything detected; Enter means yes.
fn keep_all_answer(input: &mut impl io::BufRead, out: &mut impl io::Write) -> Result<bool, String> {
    write!(
        out,
        "Nothing chosen yet. Back up everything detected? SSH keys and files that look like secrets are always left out. [Y/n] "
    )
    .and_then(|()| out.flush())
    .map_err(|e| e.to_string())?;
    let mut line = String::new();
    input.read_line(&mut line).map_err(|e| e.to_string())?;
    Ok(!matches!(line.trim(), "n" | "N" | "no" | "No"))
}

#[cfg(test)]
mod tests {
    #[test]
    fn enter_or_yes_keeps_everything_and_no_reopens_the_chooser() {
        let ask = |answer: &str| {
            let mut out = Vec::new();
            super::keep_all_answer(&mut answer.as_bytes(), &mut out).unwrap_or_default()
        };
        assert!(
            ask("\n") && ask("y\n") && ask("Y\n") && ask(""),
            "the default is yes"
        );
        assert!(!ask("n\n") && !ask("no\n"));
    }
}
