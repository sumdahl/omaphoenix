use phx_core::App;

use crate::out::{say, send};

pub fn run(app: &App<'_>, json: bool, notify: bool) -> Result<(), String> {
    if notify {
        login_notify(app);
        return Ok(());
    }
    let findings = app.doctor();
    if json {
        say(&serde_json::to_string(&findings).map_err(|e| e.to_string())?);
        return Ok(());
    }
    let mut risks = 0;
    for f in &findings {
        let mark = match f.level.as_str() {
            "risk" => "✗",
            "warn" => "!",
            _ => "✓",
        };
        say(&format!("{mark} {:<9} {}", f.area, f.msg));
        risks += usize::from(f.level == "risk");
    }
    if risks > 0 {
        return Err(format!("{risks} thing(s) would be lost in a reinstall"));
    }
    Ok(())
}

/// The login hook: at most one notification a day, never an error exit.
fn login_notify(app: &App<'_>) {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    let alert = app.backup_alert(now);
    if alert.is_empty() {
        return;
    }
    let mut lines: Vec<String> = alert.iter().take(3).map(|f| f.msg.clone()).collect();
    if alert.len() > 3 {
        lines.push(format!(
            "…and {} more: run omaphoenix doctor",
            alert.len() - 3
        ));
    }
    send("Your backups need a look", &lines.join("\n"));
}
