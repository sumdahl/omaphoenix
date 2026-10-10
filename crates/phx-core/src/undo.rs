//! `undo`: reverses the last restore newest-first without discarding work; packages are only listed.

use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;

use crate::app::{App, AppError};
use crate::category::{self, file_hash};
use crate::sys;

/// What undo did, or would do, with each change.
#[derive(Debug, Default)]
pub struct UndoReport {
    /// Snapshot whose restore is undone.
    pub snapshot: String,
    /// Reverted (or would be).
    pub reverted: Vec<String>,
    /// Left alone on purpose, with the reason.
    pub kept: Vec<String>,
    /// Could not be reversed.
    pub failed: Vec<String>,
    /// Manager → packages to remove by hand.
    pub packages: BTreeMap<String, Vec<String>>,
    /// Where created files went.
    pub trash: PathBuf,
}

impl App<'_> {
    /// Reverses the last restore.
    ///
    /// # Errors
    /// When no restore is recorded.
    pub fn undo(&self, dry_run: bool) -> Result<UndoReport, AppError> {
        let j = self.journal()?;
        let mut rep = UndoReport {
            snapshot: j.snapshot.clone(),
            trash: self.state_dir().join("undo-trash").join(category::stamp()),
            ..UndoReport::default()
        };
        let trash = |path: &str, rep: &UndoReport| -> std::io::Result<()> {
            let dst = rep.trash.join(path.trim_start_matches('/'));
            if let Some(dir) = dst.parent() {
                use std::os::unix::fs::DirBuilderExt;
                fs::DirBuilder::new()
                    .recursive(true)
                    .mode(0o700)
                    .create(dir)?;
            }
            fs::rename(path, dst)
        };
        for c in j.changes.iter().rev() {
            match c.kind.as_str() {
                "package" => rep
                    .packages
                    .entry(c.note.clone())
                    .or_default()
                    .push(c.path.clone()),
                "service" => {
                    let (scope, unit) = c.path.split_once('/').unwrap_or(("system", &c.path));
                    if dry_run {
                        rep.reverted.push(format!("disable {}", c.path));
                        continue;
                    }
                    let result = if scope == "user" {
                        sys::output(None, &["systemctl", "--user", "disable", unit]).map(|_| ())
                    } else {
                        sys::interactive(&["sudo", "systemctl", "disable", unit])
                    };
                    match result {
                        Ok(()) => rep.reverted.push(format!("disabled {}", c.path)),
                        Err(e) => rep.failed.push(format!("disable {}: {e}", c.path)),
                    }
                }
                "dir" => {
                    if fs::metadata(&c.path).is_err() {
                        rep.kept.push(format!("{}: already gone", c.path));
                    } else if dry_run {
                        rep.reverted.push(format!("move to undo trash {}", c.path));
                    } else {
                        match trash(&c.path, &rep) {
                            Ok(()) => rep.reverted.push(format!("moved to undo trash {}", c.path)),
                            Err(e) => rep.failed.push(format!("{}: {e}", c.path)),
                        }
                    }
                }
                "file" => {
                    if !c.hash.is_empty() && file_hash(c.path.as_ref()) != c.hash {
                        rep.kept.push(format!(
                            "{}: changed since the restore, so your newer version stays",
                            c.path
                        ));
                        continue;
                    }
                    if dry_run {
                        rep.reverted.push(if c.backup.is_empty() {
                            format!("move to undo trash {} (the restore created it)", c.path)
                        } else {
                            format!("put back the previous {}", c.path)
                        });
                        continue;
                    }
                    let result = match (c.backup.is_empty(), c.root) {
                        (false, true) => {
                            sys::interactive(&["sudo", "mv", "-f", "--", &c.backup, &c.path])
                                .map_err(|e| e.to_string())
                        }
                        (false, false) => fs::rename(&c.backup, &c.path).map_err(|e| e.to_string()),
                        (true, true) => sys::interactive(&["sudo", "rm", "-f", "--", &c.path])
                            .map_err(|e| e.to_string()),
                        (true, false) => trash(&c.path, &rep).map_err(|e| e.to_string()),
                    };
                    match result {
                        Err(e) => rep.failed.push(format!("{}: {e}", c.path)),
                        Ok(()) if c.backup.is_empty() => rep
                            .reverted
                            .push(format!("removed {} (the restore created it)", c.path)),
                        Ok(()) => rep
                            .reverted
                            .push(format!("put back the previous {}", c.path)),
                    }
                }
                _ => {}
            }
        }
        if !dry_run && rep.failed.is_empty() {
            self.retire_journal();
        }
        Ok(rep)
    }
}
