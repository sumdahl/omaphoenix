use std::io::{self, IsTerminal, Write};

use phx_core::sys;

/// Engine log lines, printed as the Go version did.
pub fn log(level: &str, msg: &str) {
    let _ = match level {
        "warn" => writeln!(io::stderr(), "  ! {msg}"),
        "step" => writeln!(io::stdout(), "▸ {msg}"),
        _ => writeln!(io::stdout(), "  {msg}"),
    };
}

pub fn say(msg: &str) {
    let _ = writeln!(io::stdout(), "{msg}");
}

/// A desktop notification for runs nobody watches (timer, hooks). The apps
/// set `OMAPHOENIX_NO_NOTIFY` since they show results themselves.
pub fn notify(title: &str, body: &str) {
    if io::stdout().is_terminal() || std::env::var_os("OMAPHOENIX_NO_NOTIFY").is_some() {
        return;
    }
    send(title, body);
}

/// Normal urgency with a timeout: never stuck on screen until dismissed.
pub fn send(title: &str, body: &str) {
    if sys::has("notify-send") {
        let _ = sys::output(
            None,
            &[
                "notify-send",
                "-u",
                "normal",
                "-t",
                "15000",
                "-a",
                "omaphoenix",
                title,
                body,
            ],
        );
    }
}

/// With `OMAPHOENIX_REPORT_HWM`, this process's own peak RSS for
/// `scripts/peak-rss` (wait4's figure counts children too).
pub fn report_hwm() {
    if std::env::var_os("OMAPHOENIX_REPORT_HWM").is_none() {
        return;
    }
    let status = std::fs::read_to_string("/proc/self/status").unwrap_or_default();
    if let Some(hwm) = status.lines().find(|l| l.starts_with("VmHWM:")) {
        let _ = writeln!(io::stderr(), "omaphoenix {hwm}");
    }
}
