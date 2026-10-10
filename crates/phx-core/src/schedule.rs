//! Snapshots on a systemd user timer, plus Omarchy update and login hooks.

use std::fs;
use std::path::{Path, PathBuf};

use crate::sys;

const UNIT: &str = "omaphoenix-snapshot";

/// Accepted interval names and their `OnCalendar` specs.
pub const INTERVALS: &[(&str, &str)] = &[
    ("15m", "*:0/15"),
    ("1h", "hourly"),
    ("30m", "*:0/30"),
    ("6h", "0/6:00"),
    ("daily", "daily"),
];

/// "15m, 1h, 30m, 6h, daily" (sorted like Go's).
#[must_use]
pub fn interval_names() -> String {
    let mut n: Vec<&str> = INTERVALS.iter().map(|(n, _)| *n).collect();
    n.sort_unstable();
    n.join(", ")
}

fn unit_dir() -> Option<PathBuf> {
    std::env::var_os("XDG_CONFIG_HOME")
        .filter(|d| !d.is_empty())
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| Path::new(&h).join(".config")))
        .map(|c| c.join("systemd/user"))
}

fn hooks_dir() -> Option<PathBuf> {
    std::env::var_os("HOME").map(|h| Path::new(&h).join(".config/omarchy/hooks"))
}

/// Go's `%q` for a path: double-quoted with `"` and `\` escaped.
fn quote(s: &str) -> String {
    format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\""))
}

/// The service and timer units for `binary` and `interval`.
///
/// # Errors
/// On an unknown interval.
pub fn units(binary: &str, interval: &str) -> Result<(String, String), String> {
    let cal = INTERVALS
        .iter()
        .find(|(n, _)| *n == interval)
        .map(|(_, c)| *c)
        .ok_or_else(|| {
            format!(
                "unknown interval {interval:?}; use one of: {}",
                interval_names()
            )
        })?;
    let service = format!(
        "[Unit]\nDescription=omaphoenix snapshot\nAfter=network-online.target\n\n[Service]\nType=oneshot\nExecStart={binary} snapshot\nNice=19\nIOSchedulingClass=idle\n"
    );
    let timer = format!(
        "[Unit]\nDescription=omaphoenix snapshot every {interval}\n\n[Timer]\nOnCalendar={cal}\nPersistent=true\nRandomizedDelaySec=60\n\n[Install]\nWantedBy=timers.target\n"
    );
    Ok((service, timer))
}

fn hooks(bin: &str) -> [(&'static str, String); 2] {
    [
        (
            "post-update.d/90-omaphoenix-snapshot",
            format!(
                "# Added by omaphoenix: snapshot the updated system.\n{} snapshot\n",
                quote(bin)
            ),
        ),
        (
            "post-boot.d/90-omaphoenix-health",
            format!(
                "# Added by omaphoenix: notify if backups need attention.\n{} doctor --notify\n",
                quote(bin)
            ),
        ),
    ]
}

/// Writes the units for the running binary and enables the timer.
///
/// # Errors
/// When the binary lives in a temp dir, or systemd/fs fail.
pub fn on(interval: &str) -> Result<(), String> {
    let bin = std::env::current_exe()
        .and_then(fs::canonicalize)
        .map_err(|e| e.to_string())?;
    if bin.starts_with(std::env::temp_dir()) {
        return Err(format!(
            "refusing to schedule {}: it lives in a temp dir; install omaphoenix first",
            bin.display()
        ));
    }
    let bin_s = bin.to_string_lossy();
    let (svc, tmr) = units(&bin_s, interval)?;
    let dir = unit_dir().ok_or("HOME is not set")?;
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    fs::write(dir.join(format!("{UNIT}.service")), svc).map_err(|e| e.to_string())?;
    fs::write(dir.join(format!("{UNIT}.timer")), tmr).map_err(|e| e.to_string())?;
    sys::output(None, &["systemctl", "--user", "daemon-reload"]).map_err(|e| e.to_string())?;
    sys::output(
        None,
        &[
            "systemctl",
            "--user",
            "enable",
            "--now",
            &format!("{UNIT}.timer"),
        ],
    )
    .map_err(|e| e.to_string())?;
    // Hooks only where Omarchy is.
    if let Some(hd) = hooks_dir().filter(|d| d.exists()) {
        for (rel, body) in hooks(&bin_s) {
            let f = hd.join(rel);
            if let Some(p) = f.parent() {
                fs::create_dir_all(p).map_err(|e| e.to_string())?;
            }
            fs::write(f, body).map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

/// Stops automatic snapshots and removes the units and hooks.
///
/// # Errors
/// When systemd fails.
pub fn off() -> Result<(), String> {
    sys::output(
        None,
        &[
            "systemctl",
            "--user",
            "disable",
            "--now",
            &format!("{UNIT}.timer"),
        ],
    )
    .map_err(|e| e.to_string())?;
    if let Some(dir) = unit_dir() {
        let _ = fs::remove_file(dir.join(format!("{UNIT}.timer")));
        let _ = fs::remove_file(dir.join(format!("{UNIT}.service")));
    }
    if let Some(hd) = hooks_dir() {
        for (rel, _) in hooks("") {
            let _ = fs::remove_file(hd.join(rel));
        }
    }
    sys::output(None, &["systemctl", "--user", "daemon-reload"])
        .map(|_| ())
        .map_err(|e| e.to_string())
}

/// The timer state and next run, for people.
#[must_use]
pub fn status() -> String {
    match sys::output(
        None,
        &[
            "systemctl",
            "--user",
            "list-timers",
            &format!("{UNIT}.timer"),
            "--no-legend",
        ],
    ) {
        Ok(out) if !out.is_empty() => {
            let mut s = format!("timer on: {out}");
            if hooks_dir().is_some_and(|d| d.join("post-update.d/90-omaphoenix-snapshot").exists())
            {
                s.push_str("\nalso snapshots after every omarchy update, and checks backup health at login");
            }
            s
        }
        _ => "timer off".into(),
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn units_like_go() {
        let (svc, tmr) = super::units("/usr/bin/omaphoenix", "30m").unwrap_or_default();
        assert!(
            svc.contains(
                "ExecStart=/usr/bin/omaphoenix snapshot\nNice=19\nIOSchedulingClass=idle\n"
            )
        );
        assert!(tmr.contains("OnCalendar=*:0/30\nPersistent=true\n"));
        assert!(super::units("x", "2m").is_err());
        assert_eq!(super::interval_names(), "15m, 1h, 30m, 6h, daily");
        assert_eq!(super::quote("/a b/\"x\""), "\"/a b/\\\"x\\\"\"");
    }
}
