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
        let private = secret::private_ssh_keys(&self.home);
        if private == 0 || !self.repo.exists() {
            return;
        }
        let ready = secret::secrets_ready(&self.repo.dir);
        let selected = self
            .rules()
            .is_ok_and(|r| r.selected("secret", ".ssh/id_x"));
        if ready && selected {
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
                    "{private} SSH private key(s) are not in snapshots; a reinstall means new keys everywhere. Run: omaphoenix secrets init"
                ),
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::go_duration;

    #[test]
    fn durations_read_like_go() {
        assert_eq!(go_duration(3 * 60 + 29), "3m0s");
        assert_eq!(go_duration(26 * 3600 + 5 * 60), "26h5m0s");
    }
}
