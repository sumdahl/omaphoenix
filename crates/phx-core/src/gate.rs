//! Audit before backup: failing findings leave the files and units they name out of the snapshot.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use phx_contract::{AuditSummary, GateItem};
use secaudit_core::audit::{self, Ctx, Section};
use secaudit_core::checks::SECTIONS;
use secaudit_core::store::{Store, accept_key};
use secaudit_core::{Severity, Target};

/// Sections in gate scope: what could carry a risk into a backup.
const GATE_SECTIONS: &[&str] = &[
    "Secrets & credentials in home",
    "Services & persistence",
    "Filesystem permissions",
    "Packages & updates",
    "Users & authentication",
];

/// What the gate decided for one snapshot.
#[derive(Debug, Default)]
pub struct Gate {
    /// Absolute paths left out of the capture.
    pub excluded_paths: BTreeSet<PathBuf>,
    /// `scope/unit` services not recorded as enabled.
    pub excluded_units: BTreeSet<String>,
    /// For the manifest.
    pub summary: Option<AuditSummary>,
}

/// Whether the gate runs: on unless `OMAPHOENIX_AUDIT=off`.
#[must_use]
pub fn enabled() -> bool {
    std::env::var("OMAPHOENIX_AUDIT").map_or(true, |v| v != "off")
}

/// The snapshot item a file belongs to, as `category:id`; `None` when nothing captures it.
#[must_use]
pub fn item_for(home: &Path, path: &Path) -> Option<String> {
    if let Ok(rel) = path.strip_prefix(home) {
        let rel = rel.to_string_lossy();
        if rel.is_empty() {
            return None; // the home directory itself
        }
        if let Some(local) = rel.strip_prefix(".local/") {
            return Some(if local.starts_with("bin/") {
                format!("bin:{rel}")
            } else {
                format!("local:{local}")
            });
        }
        return Some(if rel.starts_with("bin/") {
            format!("bin:{rel}")
        } else {
            format!("config:{rel}")
        });
    }
    let p = path.to_string_lossy();
    if p.starts_with("/etc/") {
        Some(format!("etc:{p}"))
    } else if p.starts_with("/usr/local/bin/") {
        Some(format!("bin:{p}"))
    } else {
        None
    }
}

/// Runs the gate for `home`. Never prompts and never needs root.
#[must_use]
pub fn run(home: &Path, log: &dyn Fn(&str, &str)) -> Gate {
    let sections: Vec<Section> = SECTIONS
        .iter()
        .filter(|s| GATE_SECTIONS.contains(&s.name))
        .copied()
        .collect();
    let ctx = Ctx::for_home(home);
    let report = audit::run(&ctx, &sections);
    let accepted = Store::from_env().accepted();
    let (score, grade) = report.score();
    let mut summary = AuditSummary::new(
        "omaphoenix (secaudit-core)",
        env!("CARGO_PKG_VERSION"),
        score,
        grade,
    );
    let mut gate = Gate::default();
    for f in report.findings() {
        let fail = match f.sev {
            Severity::Fail => true,
            Severity::Warn => false,
            _ => continue,
        };
        if accepted.contains_key(&accept_key(&f.section, &f.msg)) {
            continue;
        }
        let id = f.id.clone().unwrap_or_default();
        let sev = if fail { "fail" } else { "warn" };
        for t in &f.targets {
            let item = match t {
                Target::File { path } => {
                    let p = PathBuf::from(path);
                    let Some(item) = item_for(home, &p) else {
                        continue;
                    };
                    if fail {
                        gate.excluded_paths.insert(p);
                    }
                    item
                }
                Target::Unit { name } => {
                    if fail {
                        gate.excluded_units.insert(name.clone());
                    }
                    format!("service:{name}")
                }
                _ => continue,
            };
            let rec = GateItem::new(item, id.clone(), sev);
            if fail {
                summary.excluded.push(rec);
            } else {
                summary.warned.push(rec);
            }
        }
    }
    log(
        "info",
        &format!(
            "audit     score {score} ({grade}): {} item(s) left out, {} flagged",
            summary.excluded.len(),
            summary.warned.len()
        ),
    );
    gate.summary = Some(summary);
    gate
}

#[cfg(test)]
mod tests {
    use super::item_for;
    use std::path::Path;

    #[test]
    fn maps_paths_to_items() {
        let h = Path::new("/home/u");
        let item = |p: &str| item_for(h, Path::new(p));
        assert_eq!(
            item("/home/u/.config/app/.env").as_deref(),
            Some("config:.config/app/.env")
        );
        assert_eq!(item("/home/u/.netrc").as_deref(), Some("config:.netrc"));
        assert_eq!(
            item("/home/u/.local/share/x").as_deref(),
            Some("local:share/x")
        );
        assert_eq!(
            item("/home/u/.local/bin/s").as_deref(),
            Some("bin:.local/bin/s")
        );
        assert_eq!(
            item("/etc/sudoers.d/u").as_deref(),
            Some("etc:/etc/sudoers.d/u")
        );
        assert_eq!(
            item("/usr/local/bin/x").as_deref(),
            Some("bin:/usr/local/bin/x")
        );
        assert_eq!(item("/home/u"), None, "home itself is report-only");
        assert_eq!(
            item("/opt/spotify"),
            None,
            "outside every snapshot: report-only"
        );
    }
}

/// The audit after a restore compared with the one the restored snapshot recorded.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Reaudit {
    /// Gate score the snapshot recorded.
    pub before: u32,
    /// Gate score on this machine now.
    pub after: u32,
    /// Failing items now that the snapshot didn't have (`category:id (check)`).
    pub new_fails: Vec<String>,
    /// Failing items the snapshot had that are gone now.
    pub resolved: Vec<String>,
}

/// Compares `before` (from a snapshot) with `after` (this machine now).
#[must_use]
pub fn compare(before: &AuditSummary, after: &AuditSummary) -> Reaudit {
    let key = |g: &GateItem| format!("{} ({})", g.item, g.id);
    let had: BTreeSet<String> = before
        .excluded
        .iter()
        .chain(&before.warned)
        .map(key)
        .collect();
    let fails_before: BTreeSet<String> = before.excluded.iter().map(key).collect();
    let fails_after: BTreeSet<String> = after.excluded.iter().map(key).collect();
    Reaudit {
        before: before.score,
        after: after.score,
        new_fails: fails_after
            .iter()
            .filter(|k| !had.contains(*k))
            .cloned()
            .collect(),
        resolved: fails_before.difference(&fails_after).cloned().collect(),
    }
}

#[cfg(test)]
mod compare_tests {
    use super::compare;
    use phx_contract::{AuditSummary, GateItem};

    #[test]
    fn finds_new_and_resolved_failures() {
        let mut before = AuditSummary::new("t", "0", 80, 'B');
        before.excluded.push(GateItem::new(
            "config:.netrc",
            "secrets.credential_perms",
            "fail",
        ));
        before.warned.push(GateItem::new(
            "config:.zshrc",
            "services.rc_remote_exec",
            "warn",
        ));
        let mut after = AuditSummary::new("t", "0", 70, 'C');
        after.excluded.push(GateItem::new(
            "config:.zshrc",
            "services.rc_remote_exec",
            "fail",
        ));
        after.excluded.push(GateItem::new(
            "etc:/etc/sudoers.d/x",
            "users.sudo_nopasswd",
            "fail",
        ));
        let r = compare(&before, &after);
        assert_eq!((r.before, r.after), (80, 70));
        assert_eq!(
            r.new_fails,
            ["etc:/etc/sudoers.d/x (users.sudo_nopasswd)"],
            "a known warning that now fails isn't new"
        );
        assert_eq!(r.resolved, ["config:.netrc (secrets.credential_perms)"]);
    }
}
