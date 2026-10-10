use phx_core::App;

use crate::out::{log, notify, say};

pub fn run(app: &App<'_>) -> Result<(), String> {
    match app.snapshot() {
        Ok(None) => say("no changes since the last snapshot"),
        Ok(Some(s)) => say(&format!("✓ snapshot {}  {}", s.id, s.time)),
        Err(e) => {
            // A held lock means another run is doing the job: nothing to raise.
            if !e.to_string().contains("already running") {
                notify("omaphoenix snapshot failed", &e.to_string());
            }
            return Err(e.to_string());
        }
    }
    if let Some(msg) = app.review_needed() {
        log("warn", &msg);
    }
    Ok(())
}

pub fn list(app: &App<'_>) -> Result<(), String> {
    for s in app.repo.list().map_err(|e| e.to_string())? {
        let changes = phx_core::app::changes(&s.subject);
        if changes.is_empty() {
            say(&format!("{}  {}", s.id, s.time));
        } else {
            say(&format!("{}  {}  {changes}", s.id, s.time));
        }
    }
    Ok(())
}
