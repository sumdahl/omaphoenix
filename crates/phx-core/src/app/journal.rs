use std::fs;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use super::{App, AppError, StageReport, rfc3339_now};
use crate::category::{self, Change, read_json, write_json};
use crate::store::Snapshot;

/// Everything the last real restore changed, in order.
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct Journal {
    /// Snapshot id.
    pub snapshot: String,
    /// RFC 3339 time.
    pub time: String,
    /// The changes.
    #[serde(default)]
    pub changes: Vec<Change>,
}

/// What the last restore deferred for slow downloads.
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct Deferred {
    /// Snapshot id.
    pub snapshot: String,
    /// `"<category> <item>: <why>"`.
    #[serde(default)]
    pub items: Vec<String>,
}

impl App<'_> {
    fn journal_file(&self) -> PathBuf {
        self.state_dir().join("last-restore.json")
    }

    fn deferred_file(&self) -> PathBuf {
        self.state_dir().join("deferred.json")
    }

    // A restore that changed nothing keeps the earlier journal so the real one stays undoable.
    pub(super) fn save_journal(&self, snap: &Snapshot, reports: &[StageReport]) {
        let changes: Vec<Change> = reports
            .iter()
            .flat_map(|r| r.result.changes.iter().cloned())
            .collect();
        if changes.is_empty() {
            return;
        }
        let j = Journal {
            snapshot: snap.id.clone(),
            time: rfc3339_now(),
            changes,
        };
        if let Err(e) = write_json(&self.journal_file(), &j) {
            (self.log)(
                "warn",
                &format!("could not record this restore for undo: {e}"),
            );
        }
    }

    pub(super) fn save_deferred(&self, snap: &Snapshot, reports: &[StageReport]) {
        let items: Vec<String> = reports
            .iter()
            .flat_map(|r| {
                r.result
                    .deferred
                    .iter()
                    .map(move |d| format!("{} {d}", r.category))
            })
            .collect();
        if items.is_empty() {
            let _ = fs::remove_file(self.deferred_file());
            return;
        }
        if let Err(e) = write_json(
            &self.deferred_file(),
            &Deferred {
                snapshot: snap.id.clone(),
                items,
            },
        ) {
            (self.log)("warn", &format!("could not record deferred items: {e}"));
        }
    }

    /// What the last restore deferred, if anything.
    #[must_use]
    pub fn last_deferred(&self) -> Option<Deferred> {
        read_json::<Deferred>(&self.deferred_file())
            .ok()
            .flatten()
            .filter(|d| !d.items.is_empty())
    }

    /// The last restore's journal.
    ///
    /// # Errors
    /// When none is recorded or it can't be read.
    pub fn journal(&self) -> Result<Journal, AppError> {
        read_json(&self.journal_file())?.ok_or_else(|| {
            AppError::Other("nothing to undo: no restore has been recorded on this machine".into())
        })
    }

    /// The snapshot of the last restore, even when it was undone since (F-025).
    pub(super) fn last_restored_snapshot(&self) -> Option<String> {
        last_restored_in(&self.state_dir())
    }

    /// Marks the journal done, keeping it under a name undo won't read again.
    pub fn retire_journal(&self) {
        let stamp = category::stamp();
        let _ = fs::rename(
            self.journal_file(),
            self.state_dir()
                .join(format!("last-restore.undone-{stamp}.json")),
        );
    }
}

/// The live journal's snapshot, else the newest retired one's; retired names sort by their time stamp.
fn last_restored_in(state: &std::path::Path) -> Option<String> {
    if let Ok(Some(j)) = read_json::<Journal>(&state.join("last-restore.json")) {
        return Some(j.snapshot);
    }
    let mut retired: Vec<PathBuf> = fs::read_dir(state)
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            p.file_name()
                .is_some_and(|n| n.to_string_lossy().starts_with("last-restore.undone-"))
        })
        .collect();
    retired.sort();
    read_json::<Journal>(retired.last()?)
        .ok()
        .flatten()
        .map(|j| j.snapshot)
}

#[cfg(test)]
mod tests {
    use super::{Journal, last_restored_in};
    use crate::category::write_json;

    #[test]
    fn the_last_restore_is_found_after_an_undo() {
        let t = tempfile::tempdir().unwrap_or_else(|e| unreachable!("{e}"));
        let state = t.path();
        assert_eq!(last_restored_in(state), None, "no restore yet");
        for (stamp, snap) in [
            ("20261024-110000", "old1111"),
            ("20261024-120000", "new2222"),
        ] {
            let j = Journal {
                snapshot: snap.into(),
                ..Journal::default()
            };
            write_json(&state.join(format!("last-restore.undone-{stamp}.json")), &j)
                .unwrap_or_else(|e| unreachable!("{e}"));
        }
        assert_eq!(
            last_restored_in(state).as_deref(),
            Some("new2222"),
            "newest undone restore"
        );
        let live = Journal {
            snapshot: "live333".into(),
            ..Journal::default()
        };
        write_json(&state.join("last-restore.json"), &live).unwrap_or_else(|e| unreachable!("{e}"));
        assert_eq!(
            last_restored_in(state).as_deref(),
            Some("live333"),
            "a live journal wins"
        );
    }
}
