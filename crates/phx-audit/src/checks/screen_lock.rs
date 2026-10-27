//! "Screen lock": Omarchy shell idle lock (or hypridle), and lock on suspend.

use crate::audit::{Ctx, Report, Section, read_text};

pub(super) const SECTION: Section = Section {
    name: "Screen lock",
    run,
};

fn run(ctx: &Ctx, r: &mut Report) {
    let shell_conf = ctx.home.join(".config/omarchy/shell.json");
    let idle_conf = ctx.home.join(".config/hypr/hypridle.conf");
    if shell_conf.is_file() {
        omarchy_shell(ctx, r, &shell_conf);
    } else if idle_conf.is_file() {
        hypridle(ctx, r, &read_text(&idle_conf));
    } else {
        r.warn("screen_lock.idle", "No idle lock configuration found");
    }
}

/// `.idle.lock` as `jq -r` prints it: numbers and strings raw, else nothing.
fn idle_lock(json: &str) -> String {
    let v: serde_json::Value = serde_json::from_str(json).unwrap_or_default();
    match v.pointer("/idle/lock") {
        Some(serde_json::Value::String(s)) => s.clone(),
        Some(n @ serde_json::Value::Number(_)) => n.to_string(),
        Some(serde_json::Value::Bool(true)) => "true".into(),
        _ => String::new(),
    }
}

fn omarchy_shell(ctx: &Ctx, r: &mut Report, conf: &std::path::Path) {
    let lock = idle_lock(&read_text(conf));
    let seconds: Option<u64> = (!lock.is_empty() && lock.bytes().all(|b| b.is_ascii_digit()))
        .then(|| lock.parse().ok())
        .flatten();
    if ctx
        .home
        .join(".local/state/omarchy/indicators/stay-awake")
        .exists()
    {
        r.warn(
            "screen_lock.idle",
            "Omarchy 'stay awake' is on: idle lock disabled",
        )
        .fix("omarchy-toggle-idle allow-idle");
    } else if let Some(t) = seconds.filter(|t| *t > 0) {
        if t <= 600 {
            r.ok("screen_lock.idle", format!("Omarchy idle lock after {t}s"));
        } else {
            r.warn(
                "screen_lock.idle",
                format!("Omarchy idle lock after {t}s (consider <= 600)"),
            )
            .hint("Lower .idle.lock in ~/.config/omarchy/shell.json");
        }
    } else {
        r.warn(
            "screen_lock.idle",
            format!(
                "No idle lock timeout set in {} (.idle.lock)",
                ctx.tilde(conf)
            ),
        )
        .hint("Set .idle.lock to e.g. 300 in ~/.config/omarchy/shell.json");
    }

    let machine = format!("{}@", ctx.user);
    let active = ctx
        .run(&[
            "systemctl",
            "--user",
            "-M",
            &machine,
            "is-active",
            "-q",
            "omarchy-sleep-lock.service",
        ])
        .success
        || ctx
            .run(&[
                "systemctl",
                "--user",
                "is-active",
                "-q",
                "omarchy-sleep-lock.service",
            ])
            .success;
    if active {
        r.ok("screen_lock.suspend", "Lock-before-suspend service active");
    } else {
        r.warn(
            "screen_lock.suspend",
            "omarchy-sleep-lock.service not active (no lock on suspend)",
        )
        .fix("systemctl --user enable --now omarchy-sleep-lock.service");
    }
}

/// The lock listener's timeout, read exactly as Bash's awk does (docs/10, S7).
fn hypridle_timeout(conf: &str) -> Option<String> {
    let mut in_listener = false;
    let mut t = String::new();
    for line in conf.lines() {
        if line.contains("listener") {
            in_listener = true;
        }
        if in_listener && line.contains("timeout") {
            line.split_whitespace()
                .nth(2)
                .unwrap_or_default()
                .clone_into(&mut t);
        }
        if in_listener
            && line
                .split_once("on-timeout")
                .is_some_and(|(_, rest)| rest.contains("lock"))
        {
            return Some(t);
        }
    }
    None
}

fn hypridle(ctx: &Ctx, r: &mut Report, conf: &str) {
    if ctx.run(&["pgrep", "-x", "hypridle"]).success {
        r.ok("screen_lock.hypridle", "hypridle running");
    } else {
        r.warn(
            "screen_lock.hypridle",
            "hypridle not running (no automatic lock on idle)",
        );
    }
    match hypridle_timeout(conf).filter(|t| !t.is_empty()) {
        // Bash arithmetic reads a non-number as an unset variable, i.e. 0.
        Some(t) if t.parse::<u64>().unwrap_or(0) <= 600 => {
            r.ok("screen_lock.idle", format!("Idle lock after {t}s"));
        }
        Some(t) => {
            r.warn(
                "screen_lock.idle",
                format!("Idle lock after {t}s (consider <= 600)"),
            );
        }
        None => {
            r.warn(
                "screen_lock.idle",
                "No lock listener found in hypridle.conf",
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::checks::testing::{FakeSystem, msgs};
    use crate::contract::Severity::{Pass, Warn};

    #[test]
    fn omarchy_shell_config() {
        let sys = FakeSystem::new();
        sys.home_file(".config/omarchy/shell.json", r#"{"idle":{"lock":900}}"#);
        sys.cmd(
            "systemctl --user is-active -q omarchy-sleep-lock.service",
            true,
            "",
        );
        assert_eq!(
            msgs(&sys.ctx(false), super::SECTION),
            vec![
                (
                    Warn,
                    "Omarchy idle lock after 900s (consider <= 600)".into()
                ),
                (Pass, "Lock-before-suspend service active".into()),
            ]
        );
        sys.home_file(".config/omarchy/shell.json", r#"{"idle":{}}"#);
        sys.home_file(".local/state/omarchy/indicators/stay-awake", "");
        let got = msgs(&sys.ctx(false), super::SECTION);
        assert_eq!(
            got[0],
            (
                Warn,
                "Omarchy 'stay awake' is on: idle lock disabled".into()
            )
        );
    }

    #[test]
    fn hypridle_reads_like_bash_awk() {
        let conf = "listener {\n    timeout = 300\n    on-timeout = loginctl lock-session\n}\n";
        assert_eq!(
            super::hypridle_timeout(conf).as_deref(),
            Some("loginctl"),
            "Bash S7 quirk"
        );
        let sys = FakeSystem::new();
        sys.home_file(".config/hypr/hypridle.conf", conf);
        assert_eq!(
            msgs(&sys.ctx(false), super::SECTION),
            vec![
                (
                    Warn,
                    "hypridle not running (no automatic lock on idle)".into()
                ),
                (Pass, "Idle lock after loginctls".into()),
            ]
        );
    }

    #[test]
    fn nothing_configured() {
        let sys = FakeSystem::new();
        assert_eq!(
            msgs(&sys.ctx(false), super::SECTION),
            vec![(Warn, "No idle lock configuration found".into())]
        );
    }
}
