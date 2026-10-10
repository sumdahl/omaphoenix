use phx_core::{App, sys};

use crate::out::say;

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
    say("next: omaphoenix snapshot");
    Ok(())
}
