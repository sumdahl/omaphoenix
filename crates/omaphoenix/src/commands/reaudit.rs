use phx_core::App;

use crate::out::say;

pub fn run(app: &App<'_>) -> Result<(), String> {
    let (snap, r) = app.reaudit().map_err(|e| e.to_string())?;
    say(&format!(
        "re-audit against snapshot {snap}: score {} → {}",
        r.before, r.after
    ));
    r.new_fails
        .iter()
        .for_each(|f| say(&format!("  ✗ new since the snapshot: {f}")));
    r.resolved
        .iter()
        .for_each(|f| say(&format!("  ✓ resolved: {f}")));
    if !r.new_fails.is_empty() {
        return Err(format!(
            "{} new risk(s) since the snapshot",
            r.new_fails.len()
        ));
    }
    say("  ✓ the restore brought no new risks");
    Ok(())
}
