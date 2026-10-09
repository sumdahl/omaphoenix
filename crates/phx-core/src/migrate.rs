//! The Migrate wizard (DESIGN.md §4): audit → clean → snapshot → restore on
//! the new machine → re-audit. Progress is saved, so quitting halfway and
//! coming back later is normal. Shared by the TUI and the desktop app.

use std::path::Path;

use serde::{Deserialize, Serialize};

/// Step titles, in order.
pub const STEPS: [&str; 5] = [
    "Audit this machine",
    "Clean up the risks",
    "Take a snapshot",
    "Restore on the new machine",
    "Re-audit the new machine",
];

/// Saved progress.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq, Eq)]
pub struct Migration {
    /// An audit ran.
    #[serde(default)]
    pub audited: bool,
    /// The user reviewed the risks.
    #[serde(default)]
    pub cleaned: bool,
    /// Snapshot taken for the move.
    #[serde(default)]
    pub snapshot: String,
    /// The user has the restore command.
    #[serde(default)]
    pub restored_note_seen: bool,
}

impl Migration {
    /// Loads progress from `state_dir/migrate.json` (empty when missing).
    #[must_use]
    pub fn load(state_dir: &Path) -> Self {
        std::fs::read_to_string(state_dir.join("migrate.json"))
            .ok()
            .and_then(|t| serde_json::from_str(&t).ok())
            .unwrap_or_default()
    }

    /// Saves progress.
    pub fn save(&self, state_dir: &Path) {
        let _ = std::fs::create_dir_all(state_dir);
        if let Ok(body) = serde_json::to_vec_pretty(self) {
            let _ = std::fs::write(state_dir.join("migrate.json"), body);
        }
    }

    /// Which steps are done.
    #[must_use]
    pub fn done(&self) -> [bool; 5] {
        [
            self.audited,
            self.cleaned,
            !self.snapshot.is_empty(),
            self.restored_note_seen,
            false,
        ]
    }
}

/// What step `i` does, worded for the user. `remote` is the snapshot repo URL.
#[must_use]
pub fn explain(i: usize, remote: &str) -> String {
    match i {
        0 => "Runs a quick audit, so nothing risky (exposed keys, unknown services) travels with you.".into(),
        1 => "Review the risks in Audit: fix them, or accept the ones you mean to keep.".into(),
        2 => "Takes a snapshot now. Risky items the audit found are left out of it.".into(),
        3 => {
            let from = if remote.is_empty() { "<your snapshot repo URL>" } else { remote };
            format!("On the new machine, install OmaPhoenix, then run: omaphoenix restore --from {from}")
        }
        _ => "On the new machine after restoring: audits it and compares with the snapshot's audit (omaphoenix reaudit).".into(),
    }
}

#[cfg(test)]
mod tests {
    use super::Migration;

    #[test]
    fn progress_round_trips() {
        let t = tempfile::tempdir().unwrap_or_else(|e| unreachable!("{e}"));
        let m = Migration {
            audited: true,
            snapshot: "abc".into(),
            ..Migration::default()
        };
        m.save(t.path());
        assert_eq!(Migration::load(t.path()), m);
        assert_eq!(m.done(), [true, false, true, false, false]);
    }
}
