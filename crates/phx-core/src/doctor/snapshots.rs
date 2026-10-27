use super::{App, age_secs, go_duration};
use crate::category::read_json;
use crate::sys;

impl App<'_> {
    pub(super) fn check_snapshots(&self, add: &mut dyn FnMut(&str, &str, String)) {
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

    // Scores only the backup-relevant sections, so it differs from a full scan's score.
    pub(super) fn check_snapshot_audit(&self, add: &mut dyn FnMut(&str, &str, String)) {
        let manifest: Option<phx_contract::manifest::Manifest> =
            read_json(&self.repo.dir.join("manifest.json"))
                .ok()
                .flatten();
        let Some(a) = manifest.and_then(|m| m.audit) else {
            return;
        };
        let level = if a.excluded.is_empty() { "ok" } else { "warn" };
        add(
            level,
            "audit",
            format!(
                "last snapshot's audit (backup-relevant checks): score {} ({}), {} left out, {} flagged",
                a.score,
                a.grade,
                a.excluded.len(),
                a.warned.len()
            ),
        );
    }
}

/// The last Security Audit scan, from the history both apps share.
pub(super) fn check_audit(add: &mut dyn FnMut(&str, &str, String)) {
    let Some(run) = phx_audit::store::Store::from_env()
        .runs()
        .into_iter()
        .next()
    else {
        add("warn", "audit", "no security audit yet; snapshots audit themselves, but run one to see the whole picture (TUI: 3 Audit)".into());
        return;
    };
    let c = run.counts();
    let level = if c.fail > 0 { "warn" } else { "ok" };
    let age = go_duration(phx_audit::store::now().saturating_sub(run.meta.started));
    add(
        level,
        "audit",
        format!(
            "last full audit {age} ago: score {} ({}), {} fail, {} warn",
            c.score(),
            c.grade(),
            c.fail,
            c.warn
        ),
    );
}
