use std::io;
use std::path::Path;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::sys::{self, WatchError, Watchdog};

/// One thing a restore did, recorded so `undo` can reverse it.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct Change {
    /// `file`, `dir`, `service` or `package`.
    pub kind: String,
    /// What changed: a path, `scope/unit`, or a package name.
    pub path: String,
    /// Where the previous file was moved; empty when the restore created it.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub backup: String,
    /// sha256 of what the restore wrote, so undo can spot later edits.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub hash: String,
    /// Reverting needs sudo.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub root: bool,
    /// For packages: the manager.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub note: String,
}

impl Change {
    fn new(kind: &str, path: &str) -> Self {
        Self {
            kind: kind.into(),
            path: path.into(),
            backup: String::new(),
            hash: String::new(),
            root: false,
            note: String::new(),
        }
    }
}

/// What one category's restore did; `failed` is non-empty when any part did not apply.
#[derive(Clone, Debug, Default)]
pub struct Outcome {
    /// Items applied.
    pub applied: usize,
    /// Items already as in the snapshot.
    pub present: usize,
    /// What failed, worded for the user.
    pub failed: Vec<String>,
    /// What a dry run would do.
    pub planned: Vec<String>,
    /// Stopped because the download crawled or stalled; rerun to continue.
    pub deferred: Vec<String>,
    /// What this restore changed, in order, for `undo`.
    pub changes: Vec<Change>,
    /// Left out on purpose, worded for the user; not a failure.
    pub notes: Vec<String>,
}

impl Outcome {
    /// Records a change.
    pub fn changed(&mut self, kind: &str, path: &str, note: &str, root: bool) {
        let mut c = Change::new(kind, path);
        c.note = note.into();
        c.root = root;
        self.changes.push(c);
    }

    /// Records a written file, hashing it now so undo can spot later edits.
    pub fn wrote_file(&mut self, path: &Path, backup: Option<&Path>, root: bool) {
        let mut c = Change::new("file", &path.to_string_lossy());
        c.backup = backup
            .map(|b| b.to_string_lossy().into_owned())
            .unwrap_or_default();
        c.hash = file_hash(path);
        c.root = root;
        self.changes.push(c);
    }

    /// Runs one watched install and files it as applied, deferred or failed.
    pub fn install(&mut self, label: &str, argv: &[&str]) -> bool {
        match sys::interactive_watched(Watchdog::default(), argv, sys::WATCH_INTERVAL) {
            Ok(()) => {
                self.applied += 1;
                true
            }
            Err(e @ WatchError::Deferred { .. }) => {
                self.deferred.push(format!("{label}: {e}"));
                false
            }
            Err(e) => {
                self.failed.push(format!("{label}: {e}"));
                false
            }
        }
    }
}

/// sha256 of a file's content, or of a symlink's target text; empty when unreadable.
#[must_use]
pub fn file_hash(path: &Path) -> String {
    let mut h = Sha256::new();
    if let Ok(meta) = std::fs::symlink_metadata(path)
        && meta.file_type().is_symlink()
    {
        let Ok(t) = std::fs::read_link(path) else {
            return String::new();
        };
        h.update(t.as_os_str().as_encoded_bytes());
        return hex(&h.finalize());
    }
    let Ok(mut f) = std::fs::File::open(path) else {
        return String::new();
    };
    let mut buf = vec![0u8; 64 * 1024];
    loop {
        match io::Read::read(&mut f, &mut buf) {
            Ok(0) => return hex(&h.finalize()),
            Ok(n) => h.update(buf.get(..n).unwrap_or_default()),
            Err(_) => return String::new(),
        }
    }
}

/// Lowercase hex.
#[must_use]
pub fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write;
    bytes
        .iter()
        .fold(String::with_capacity(bytes.len() * 2), |mut s, b| {
            let _ = write!(s, "{b:02x}");
            s
        })
}

#[cfg(test)]
mod tests {
    use super::file_hash;

    #[test]
    fn hashes_files_and_links() {
        let t = tempfile::tempdir().unwrap_or_else(|e| unreachable!("{e}"));
        let f = t.path().join("f");
        std::fs::write(&f, "abc").ok();
        assert_eq!(
            file_hash(&f),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        std::os::unix::fs::symlink("abc", t.path().join("l")).ok();
        assert_eq!(
            file_hash(&t.path().join("l")),
            file_hash(&f),
            "a link hashes its target text"
        );
        assert_eq!(file_hash(&t.path().join("missing")), "");
    }
}
