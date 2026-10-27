//! Scan history and accepted risks under `$XDG_STATE_HOME/omaphoenix-audit` (`secaudit` before 0.1.0).

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

mod model;
mod report;
mod run;
mod scan;
#[cfg(test)]
mod tests;

pub use model::{Accepted, Counts, IssueKey, Item, Meta, accept_key, sev_name, sev_order};
pub use run::Run;

/// Scans kept; older complete scans are deleted after each new one.
pub const KEEP_RUNS: usize = 30;

/// Seconds since the epoch.
#[must_use]
pub fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

/// `base/omaphoenix-audit`, first moving `base/secaudit` there if only the old one exists.
#[must_use]
pub fn renamed_dir(base: &Path) -> PathBuf {
    let new = base.join("omaphoenix-audit");
    let old = base.join("secaudit");
    if !new.exists() && old.is_dir() {
        let _ = fs::rename(&old, &new);
    }
    new
}

/// Where state lives.
#[derive(Clone, Debug)]
pub struct Store {
    /// `$XDG_STATE_HOME/omaphoenix-audit`.
    pub dir: PathBuf,
}

impl Store {
    /// `$XDG_STATE_HOME/omaphoenix-audit`, or `~/.local/state/omaphoenix-audit`.
    #[must_use]
    pub fn from_env() -> Self {
        let base = std::env::var_os("XDG_STATE_HOME")
            .filter(|v| !v.is_empty())
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|h| Path::new(&h).join(".local/state")))
            .unwrap_or_default();
        Self {
            dir: renamed_dir(&base),
        }
    }

    /// `runs/`.
    #[must_use]
    pub fn runs_dir(&self) -> PathBuf {
        self.dir.join("runs")
    }

    /// Accepted risks; empty if missing or unreadable.
    #[must_use]
    pub fn accepted(&self) -> BTreeMap<String, Accepted> {
        fs::read_to_string(self.dir.join("accepted.json"))
            .ok()
            .and_then(|t| serde_json::from_str(&t).ok())
            .unwrap_or_default()
    }

    /// Saves accepted risks atomically.
    ///
    /// # Errors
    /// If the file can't be written.
    pub fn save_accepted(&self, accepted: &BTreeMap<String, Accepted>) -> std::io::Result<()> {
        fs::create_dir_all(&self.dir)?;
        let body = serde_json::to_vec_pretty(accepted).map_err(std::io::Error::other)?;
        let tmp = self.dir.join(".accepted.json.tmp");
        fs::write(&tmp, body)?;
        fs::rename(tmp, self.dir.join("accepted.json"))
    }

    /// Complete scans, newest first.
    #[must_use]
    pub fn runs(&self) -> Vec<Run> {
        let accepted = self.accepted();
        let mut dirs: Vec<PathBuf> = fs::read_dir(self.runs_dir())
            .into_iter()
            .flatten()
            .flatten()
            .map(|e| e.path())
            .collect();
        dirs.sort();
        dirs.reverse();
        dirs.iter()
            .filter(|d| fs::metadata(d.join("findings.jsonl")).is_ok_and(|m| m.len() > 0))
            .filter_map(|d| Run::load(d, &accepted).ok())
            .filter(Run::complete)
            .collect()
    }

    /// Starts a scan directory with its `meta.json`.
    ///
    /// # Errors
    /// If the directory can't be created.
    pub fn begin(&self, stamp: &str, meta: &Meta) -> std::io::Result<PathBuf> {
        let dir = self.runs_dir().join(stamp);
        fs::create_dir_all(&dir)?;
        write_meta(&dir, meta)?;
        Ok(dir)
    }

    /// Marks a scan finished and drops scans beyond [`KEEP_RUNS`].
    ///
    /// # Errors
    /// If `meta.json` can't be written.
    pub fn finish(&self, dir: &Path, meta: &mut Meta) -> std::io::Result<()> {
        meta.finished = Some(now());
        write_meta(dir, meta)?;
        for old in self.runs().iter().skip(KEEP_RUNS) {
            let _ = fs::remove_dir_all(&old.path);
        }
        Ok(())
    }

    /// Removes scans that never finished and are older than `min_age` seconds.
    pub fn prune_incomplete(&self, min_age: u64) {
        for e in fs::read_dir(self.runs_dir())
            .into_iter()
            .flatten()
            .flatten()
        {
            let dir = e.path();
            let meta: Meta = fs::read_to_string(dir.join("meta.json"))
                .ok()
                .and_then(|t| serde_json::from_str(&t).ok())
                .unwrap_or_default();
            let age = e
                .metadata()
                .ok()
                .and_then(|m| m.modified().ok())
                .and_then(|t| t.elapsed().ok())
                .map_or(0, |d| d.as_secs());
            if meta.finished.is_none() && age > min_age {
                let _ = fs::remove_dir_all(&dir);
            }
        }
    }
}

fn write_meta(dir: &Path, meta: &Meta) -> std::io::Result<()> {
    fs::write(
        dir.join("meta.json"),
        serde_json::to_vec(meta).map_err(std::io::Error::other)?,
    )
}
