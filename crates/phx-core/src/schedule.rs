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

/// What the apps offer, most frequent first; no interval means manual only.
pub const CHOICES: [&str; 5] = ["15m", "30m", "1h", "6h", "daily"];

/// An interval in words, or what manual means.
#[must_use]
pub fn describe(interval: Option<&str>) -> &'static str {
    match interval {
        None => "Manual only: a snapshot is taken when you take one",
        Some("15m") => "Automatic, every 15 minutes",
        Some("30m") => "Automatic, every 30 minutes",
        Some("1h") => "Automatic, every hour",
        Some("6h") => "Automatic, every 6 hours",
        Some("daily") => "Automatic, once a day",
        Some(_) => "Automatic",
    }
}

/// A short label for status lines: "daily", "hourly", "every 30 min", or "manual".
#[must_use]
pub fn label(interval: Option<&str>) -> &'static str {
    match interval {
        None => "manual",
        Some("15m") => "every 15 min",
        Some("30m") => "every 30 min",
        Some("1h") => "hourly",
        Some("6h") => "every 6 hours",
        Some("daily") => "daily",
        Some(_) => "automatic",
    }
}

/// The timer's interval, or `None` when snapshots are manual only.
#[must_use]
pub fn interval() -> Option<&'static str> {
    let text = fs::read_to_string(unit_dir()?.join(format!("{UNIT}.timer"))).ok()?;
    let cal = text.lines().find_map(|l| l.strip_prefix("OnCalendar="))?;
    INTERVALS.iter().find(|(_, c)| *c == cal).map(|(n, _)| *n)
}

/// Automatic snapshots every `interval`, or manual only with `None`.
///
/// # Errors
/// When systemd or the filesystem fail.
pub fn set(interval: Option<&str>) -> Result<(), String> {
    match interval {
        Some(i) => on(i),
        None if self::interval().is_none() => Ok(()),
        None => off(),
    }
}

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
    let exe = std::env::current_exe()
        .and_then(fs::canonicalize)
        .map_err(|e| e.to_string())?;
    // The desktop app calls this too; the timer must run the CLI, never the app.
    let bin = if exe.file_name().is_some_and(|n| n == "omaphoenix") {
        exe.clone()
    } else {
        exe.with_file_name("omaphoenix")
    };
    if !bin.is_file() {
        return Err(format!(
            "the omaphoenix command isn't next to {}; rerun install.sh",
            exe.display()
        ));
    }
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

/// True while the timer's snapshot is running.
#[must_use]
pub fn running() -> bool {
    sys::command("systemctl")
        .args(["--user", "is-active", "--quiet", &format!("{UNIT}.service")])
        .status()
        .is_ok_and(|s| s.success())
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
    fn every_interval_has_a_short_label() {
        assert_eq!(super::label(Some("daily")), "daily");
        assert_eq!(super::label(Some("1h")), "hourly");
        assert_eq!(super::label(None), "manual");
        for c in super::CHOICES {
            assert_ne!(super::label(Some(c)), "automatic", "{c} needs a label");
        }
    }

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
