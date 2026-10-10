//! `doctor`: what a reinstall would lose now, plus the daily backup-health notification.

mod notify;
mod repos;
mod snapshots;
mod units;

use serde::{Deserialize, Serialize};

use crate::app::App;
use crate::category::secret;
use crate::sys;

pub use notify::notify_worthy;

/// One thing a reinstall would lose or that weakens snapshots; `risk` fails `doctor`.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct Finding {
    /// `risk`, `warn` or `ok`.
    pub level: String,
    /// `snapshots`, `repos`, `services`, `secrets` or `restore`.
    pub area: String,
    /// What, worded for the user.
    pub msg: String,
}

/// Seconds since an RFC 3339 time, via `date`.
fn age_secs(time: &str) -> Option<u64> {
    let then: u64 = sys::output(None, &["date", "-d", time, "+%s"])
        .ok()?
        .parse()
        .ok()?;
    let now: u64 = sys::output(None, &["date", "+%s"]).ok()?.parse().ok()?;
    Some(now.saturating_sub(then))
}

/// Go's `Duration.Round(time.Minute).String()` for whole minutes: "3h25m0s".
fn go_duration(secs: u64) -> String {
    let mins = (secs + 30) / 60;
    let (h, m) = (mins / 60, mins % 60);
    if h > 0 {
        format!("{h}h{m}m0s")
    } else {
        format!("{m}m0s")
    }
}

impl App<'_> {
    /// Answers "what would I lose if I reinstalled right now?".
    #[must_use]
    pub fn doctor(&self) -> Vec<Finding> {
        let mut f = Vec::new();
        let mut add = |level: &str, area: &str, msg: String| {
            f.push(Finding {
                level: level.into(),
                area: area.into(),
                msg,
            });
        };
        if self.repo.exists() {
            self.check_snapshots(&mut add);
            if let Some(msg) = self.review_needed() {
                add("warn", "snapshots", msg);
            }
        } else {
            add("risk", "snapshots", "not set up: nothing is protected. Run: omaphoenix init --github <you>/omaphoenix-backup".into());
        }
        snapshots::check_audit(&mut add);
        self.check_snapshot_audit(&mut add);
        self.check_repos(&mut add);
        self.check_units(&mut add);
        self.check_secrets(&mut add);
        if let Some(d) = self.last_deferred() {
            add(
                "warn",
                "restore",
                format!(
                    "{} item(s) from the last restore were skipped for slow downloads ({}); run `omaphoenix restore` again to finish them",
                    d.items.len(),
                    d.items.join("; ")
                ),
            );
        }
        f
    }

    fn check_secrets(&self, add: &mut dyn FnMut(&str, &str, String)) {
        let keys = secret::private_ssh_keys(&self.home);
        if keys.is_empty() || !self.repo.exists() {
            return;
        }
        if !secret::secrets_ready(&self.repo.dir) {
            add(
                "warn",
                "secrets",
                format!(
                    "{} SSH private key(s) are not in snapshots; a reinstall means new keys everywhere. Run: omaphoenix secrets init",
                    keys.len()
                ),
            );
            return;
        }
        let rules = self.rules().unwrap_or_default();
        let missing: Vec<&str> = keys
            .iter()
            .filter(|k| !rules.selected("secret", k))
            .map(String::as_str)
            .collect();
        if missing.is_empty() {
            add(
                "ok",
                "secrets",
                "SSH keys are in snapshots, encrypted".into(),
            );
        } else {
            add(
                "warn",
                "secrets",
                format!(
                    "{} of {} SSH private key(s) are not in snapshots ({}). Tick them in Backup → secret: omaphoenix select",
                    missing.len(),
                    keys.len(),
                    missing.join(", ")
                ),
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::go_duration;

    fn secrets_findings(rules: &str, ready: bool) -> Vec<super::Finding> {
        let t = tempfile::tempdir().unwrap_or_else(|e| unreachable!("{e}"));
        let home = t.path().join("home");
        let repo = t.path().join("repo");
        for d in [home.join(".ssh"), repo.join(".git"), repo.join("secrets")] {
            std::fs::create_dir_all(d).unwrap_or_else(|e| unreachable!("{e}"));
        }
        for k in ["id_a", "id_b", "id_a.pub"] {
            std::fs::write(home.join(".ssh").join(k), "x").unwrap_or_else(|e| unreachable!("{e}"));
        }
        std::fs::write(repo.join("selection.conf"), rules).unwrap_or_else(|e| unreachable!("{e}"));
        if ready {
            std::fs::write(repo.join("secrets/recipient.txt"), "age1x")
                .unwrap_or_else(|e| unreachable!("{e}"));
        }
        let app = crate::App {
            home,
            repo: crate::store::Repo { dir: repo },
            log: &|_, _| {},
            passphrase: None,
        };
        let mut out = Vec::new();
        app.check_secrets(&mut |level, area, msg| {
            out.push(super::Finding {
                level: level.into(),
                area: area.into(),
                msg,
            });
        });
        out
    }

    #[test]
    fn secrets_are_judged_by_each_real_key() {
        let per_item = "secret:.ssh/id_a\nsecret:.ssh/id_a.pub\nsecret:.ssh/id_b\n";
        let f = secrets_findings(per_item, true);
        assert_eq!(f.first().map(|f| f.level.as_str()), Some("ok"), "{f:?}");
        let f = secrets_findings("secret:.ssh/id_a\n", true);
        assert!(
            f.first().is_some_and(|f| f.level == "warn"
                && f.msg.contains("1 of 2")
                && f.msg.contains(".ssh/id_b")),
            "{f:?}"
        );
        let f = secrets_findings("secret:.ssh/*\n", false);
        assert!(
            f.first().is_some_and(|f| f.msg.contains("secrets init")),
            "{f:?}"
        );
    }

    #[test]
    fn durations_read_like_go() {
        assert_eq!(go_duration(3 * 60 + 29), "3m0s");
        assert_eq!(go_duration(26 * 3600 + 5 * 60), "26h5m0s");
    }
}
