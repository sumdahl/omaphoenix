use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use super::{App, Finding};
use crate::category::{hex, read_json, write_json};

impl App<'_> {
    /// The findings to notify about now (empty when nothing is new); records what it returns.
    #[must_use]
    pub fn backup_alert(&self, now: u64) -> Vec<Finding> {
        let worthy = notify_worthy(self.doctor());
        if worthy.is_empty() {
            return worthy;
        }
        let file = self.state_dir().join("notified.json");
        let prev: Notified = read_json(&file).ok().flatten().unwrap_or_default();
        let hash = findings_hash(&worthy);
        if !should_notify(&prev, &hash, now) {
            return Vec::new();
        }
        let _ = write_json(&file, &Notified { hash, at: now });
        worthy
    }
}

/// The findings worth interrupting the user for: backup health, not repo hygiene.
#[must_use]
pub fn notify_worthy(findings: Vec<Finding>) -> Vec<Finding> {
    findings
        .into_iter()
        .filter(|f| {
            f.level != "ok"
                && (f.area == "snapshots" || f.area == "restore")
                // The login hook that notifies only exists while the timer is on.
                && !f.msg.contains("automatic snapshots are off")
        })
        .collect()
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
struct Notified {
    hash: String,
    // Go stored an RFC 3339 time; an old file just re-notifies once.
    #[serde(default, deserialize_with = "lenient_secs")]
    at: u64,
}

fn lenient_secs<'de, D: serde::Deserializer<'de>>(d: D) -> Result<u64, D::Error> {
    Ok(serde_json::Value::deserialize(d)?.as_u64().unwrap_or(0))
}

const RENOTIFY_AFTER: u64 = 24 * 3600;

// Ages change every minute; hash what the problem is, not when it was measured.
fn findings_hash(fs: &[Finding]) -> String {
    let mut h = Sha256::new();
    for f in fs {
        let msg: String = f.msg.chars().filter(|c| !c.is_ascii_digit()).collect();
        h.update(format!("{}|{}|{msg}\n", f.level, f.area));
    }
    hex(&h.finalize())
}

fn should_notify(prev: &Notified, hash: &str, now: u64) -> bool {
    hash != prev.hash || now.saturating_sub(prev.at) >= RENOTIFY_AFTER
}

#[cfg(test)]
mod tests {
    use super::{Finding, Notified, findings_hash, notify_worthy, should_notify};

    fn f(level: &str, area: &str, msg: &str) -> Finding {
        Finding {
            level: level.into(),
            area: area.into(),
            msg: msg.into(),
        }
    }

    #[test]
    fn notifies_new_problems_at_once_and_old_ones_daily() {
        let worthy = notify_worthy(vec![
            f("risk", "snapshots", "no remote"),
            f("warn", "snapshots", "automatic snapshots are off. Run: x"),
            f("risk", "repos", "~/p: 2 unpushed commit(s)"),
            f("ok", "snapshots", "last snapshot 3m0s ago"),
        ]);
        assert_eq!(worthy, vec![f("risk", "snapshots", "no remote")]);
        let h = findings_hash(&[f("warn", "snapshots", "last snapshot 26h0m0s ago")]);
        assert_eq!(
            h,
            findings_hash(&[f("warn", "snapshots", "last snapshot 27h5m0s ago")]),
            "ages don't count as new"
        );
        let prev = Notified {
            hash: h.clone(),
            at: 1000,
        };
        assert!(!should_notify(&prev, &h, 1000 + 3600));
        assert!(should_notify(&prev, &h, 1000 + 25 * 3600));
        assert!(should_notify(&prev, "other", 1001));
    }
}
