//! `doctor`: what would be lost if this machine were reinstalled right now,
//! and the once-a-day desktop notification about backup health.

use std::fs;
use std::path::Path;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::app::App;
use crate::category::{files::Walk, files::walk, hex, read_json, secret, write_json};
use crate::sys;

/// One thing a reinstall would lose, or that keeps snapshots from
/// protecting the machine. `risk` makes `doctor` exit non-zero.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct Finding {
    /// `risk`, `warn` or `ok`.
    pub level: String,
    /// `snapshots`, `repos`, `services`, `secrets` or `restore`.
    pub area: String,
    /// What, worded for the user.
    pub msg: String,
}

/// Directories holding other tools' checkouts, not the user's work.
const REPO_SCAN_SKIP: &[&str] = &[
    ".cache",
    ".local/share",
    ".local/state",
    ".cargo",
    ".rustup",
    "go",
    ".npm",
    ".config/omarchy/plugins",
    ".config/omarchy/themes",
    "node_modules",
    ".git",
    "Downloads",
    ".Trash",
    ".var",
    "snap",
];

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

    fn check_snapshots(&self, add: &mut dyn FnMut(&str, &str, String)) {
        let snaps = self.repo.list().unwrap_or_default();
        match snaps.first() {
            None => add(
                "risk",
                "snapshots",
                "no snapshot taken yet. Run: omaphoenix snapshot".into(),
            ),
            Some(s) => {
                if let Some(age) = age_secs(&s.time) {
                    let level = if age > 24 * 3600 { "warn" } else { "ok" };
                    add(
                        level,
                        "snapshots",
                        format!("last snapshot {} ago ({})", go_duration(age), s.id),
                    );
                }
            }
        }
        let remote = self.repo.remote();
        if remote.is_empty() {
            add(
                "risk",
                "snapshots",
                format!(
                    "no remote: snapshots live only on this disk and die with it. Run: git -C {} remote add origin <url>",
                    self.repo.dir.display()
                ),
            );
        } else if let Ok(n) = sys::output(
            Some(&self.repo.dir),
            &["git", "rev-list", "--count", "HEAD", "--not", "--remotes"],
        ) && n != "0"
        {
            add(
                "risk",
                "snapshots",
                format!(
                    "{n} snapshot(s) not pushed to {remote} yet (offline when taken?); the next snapshot retries"
                ),
            );
        }
        if sys::output(
            None,
            &[
                "systemctl",
                "--user",
                "is-enabled",
                "omaphoenix-snapshot.timer",
            ],
        )
        .unwrap_or_default()
            != "enabled"
        {
            add(
                "warn",
                "snapshots",
                "automatic snapshots are off. Run: omaphoenix timer on 30m".into(),
            );
        }
    }

    /// Git repositories in `$HOME` whose work exists nowhere else.
    fn check_repos(&self, add: &mut dyn FnMut(&str, &str, String)) {
        let (mut risky, mut clean) = (0, 0);
        let mut found = Vec::new();
        walk(&self.home, &mut |p, meta| {
            if !meta.is_dir() {
                return Walk::Continue;
            }
            let rel = p
                .strip_prefix(&self.home)
                .map(|r| r.to_string_lossy().into_owned())
                .unwrap_or_default();
            let name = p
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
            if !rel.is_empty()
                && (rel.matches('/').count() >= 4
                    || name.starts_with('.')
                    || REPO_SCAN_SKIP.iter().any(|s| *s == rel || *s == name))
            {
                return Walk::Skip;
            }
            if p.join(".git").exists() {
                found.push((p.to_path_buf(), rel));
            }
            // Keep descending: a repo can contain other repos.
            Walk::Continue
        });
        for (dir, rel) in found {
            let name = if rel.is_empty() {
                "~ (home itself)".to_owned()
            } else {
                format!("~/{rel}")
            };
            match repo_risk(&dir) {
                Some(msg) => {
                    risky += 1;
                    add("risk", "repos", format!("{name}: {msg}"));
                }
                None => clean += 1,
            }
        }
        if risky == 0 {
            add(
                "ok",
                "repos",
                format!("{clean} git repos in ~ are fully pushed"),
            );
        }
    }

    /// Enabled user services that run something from `$HOME` that snapshots
    /// don't carry.
    fn check_units(&self, add: &mut dyn FnMut(&str, &str, String)) {
        let dir = self.home.join(".config/systemd/user");
        let home = format!("{}/", self.home.display());
        let mut entries: Vec<_> = fs::read_dir(&dir).into_iter().flatten().flatten().collect();
        entries.sort_by_key(fs::DirEntry::file_name);
        let mut seen = std::collections::BTreeSet::new();
        for e in entries {
            let unit = e.file_name().to_string_lossy().into_owned();
            if !unit.ends_with(".service") {
                continue;
            }
            let text =
                String::from_utf8_lossy(&sys::read_bounded(&e.path(), 1 << 20).unwrap_or_default())
                    .into_owned();
            for line in text.lines() {
                let Some((key, val)) = line.trim().split_once('=') else {
                    continue;
                };
                if !key.starts_with("Exec") {
                    continue;
                }
                for tok in val.split_whitespace() {
                    let tok = tok
                        .trim_start_matches(['-', '@', ':', '+', '!'])
                        .replace("%h", &self.home.to_string_lossy());
                    let Some(rel) = tok.strip_prefix(&home) else {
                        continue;
                    };
                    if !covered_by_snapshot(rel) && seen.insert(format!("{unit}{rel}")) {
                        add(
                            "warn",
                            "services",
                            format!(
                                "{unit} runs ~/{rel}, which snapshots don't include; it won't work after a restore until that is back"
                            ),
                        );
                    }
                    break;
                }
            }
        }
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

    /// The findings to notify about now, or empty when nothing is new.
    /// Records what it returns, so calling it is "showing" it.
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

fn covered_by_snapshot(rel: &str) -> bool {
    [
        ".config/",
        ".local/bin/",
        ".local/share/applications/",
        ".local/share/fonts/",
    ]
    .iter()
    .any(|p| rel.starts_with(p))
}

fn repo_risk(dir: &Path) -> Option<String> {
    let remotes = sys::output(Some(dir), &["git", "remote"]).unwrap_or_default();
    let commits = sys::output(Some(dir), &["git", "rev-list", "--count", "--all"])
        .ok()
        .filter(|c| c != "0")?;
    if remotes.is_empty() {
        return Some(format!(
            "no remote at all ({commits} commits exist only here)"
        ));
    }
    let mut parts = Vec::new();
    if let Ok(n) = sys::output(
        Some(dir),
        &[
            "git",
            "rev-list",
            "--count",
            "--branches",
            "--not",
            "--remotes",
        ],
    ) && !n.is_empty()
        && n != "0"
    {
        parts.push(format!("{n} unpushed commit(s)"));
    }
    if let Ok(s) = sys::output(Some(dir), &["git", "status", "--porcelain"])
        && !s.is_empty()
    {
        parts.push(format!(
            "{} uncommitted change(s)",
            s.matches('\n').count() + 1
        ));
    }
    (!parts.is_empty()).then(|| parts.join(", "))
}

/// Backup health interrupts the user; repo hygiene stays in doctor. "Timer
/// off" is left out: the login hook that notifies only exists while it's on.
#[must_use]
pub fn notify_worthy(findings: Vec<Finding>) -> Vec<Finding> {
    findings
        .into_iter()
        .filter(|f| {
            f.level != "ok"
                && (f.area == "snapshots" || f.area == "restore")
                && !f.msg.contains("automatic snapshots are off")
        })
        .collect()
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
struct Notified {
    hash: String,
    /// Unix seconds. (Go stored an RFC 3339 time; an old file just re-notifies once.)
    #[serde(default, deserialize_with = "lenient_secs")]
    at: u64,
}

fn lenient_secs<'de, D: serde::Deserializer<'de>>(d: D) -> Result<u64, D::Error> {
    Ok(serde_json::Value::deserialize(d)?.as_u64().unwrap_or(0))
}

const RENOTIFY_AFTER: u64 = 24 * 3600;

/// Ages change every minute; hash what the problem is, not when measured.
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
    use super::{Finding, Notified, findings_hash, go_duration, notify_worthy, should_notify};

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

    #[test]
    fn durations_read_like_go() {
        assert_eq!(go_duration(3 * 60 + 29), "3m0s");
        assert_eq!(go_duration(26 * 3600 + 5 * 60), "26h5m0s");
    }
}
