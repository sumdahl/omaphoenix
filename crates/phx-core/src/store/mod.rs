//! Snapshots as commits in a git repository, read back with `git archive`.

mod archive;
mod history;
#[cfg(test)]
mod tests;

use std::fs::{self, File};
use std::io;
use std::path::{Path, PathBuf};

use crate::sys::{self, CmdError};

pub use archive::untar;

/// Why a store operation failed.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum StoreError {
    /// A git command failed.
    #[error(transparent)]
    Git(#[from] CmdError),
    /// Filesystem trouble.
    #[error(transparent)]
    Io(#[from] io::Error),
    /// Another run holds the repo lock.
    #[error(
        "a snapshot or restore is already running (maybe the automatic one); try again when it finishes"
    )]
    Locked,
    /// No snapshot matches.
    #[error("{0}")]
    NotFound(String),
    /// A snapshot archive tried to write outside the extraction directory.
    #[error("refusing path outside the snapshot: {0}")]
    Traversal(String),
}

/// The snapshot repository.
#[derive(Clone, Debug)]
pub struct Repo {
    /// Its working tree.
    pub dir: PathBuf,
}

/// One snapshot.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Snapshot {
    /// Short commit hash.
    pub id: String,
    /// ISO 8601 commit date.
    pub time: String,
    /// Commit subject.
    pub subject: String,
}

/// The repo lock; released on drop.
#[derive(Debug)]
pub struct Lock {
    _file: File,
    /// Git lock files left by an interrupted run, removed when the lock was taken.
    pub cleared: Vec<PathBuf>,
}

impl Repo {
    fn git(&self, args: &[&str]) -> Result<String, CmdError> {
        let mut cmd = vec!["git"];
        cmd.extend_from_slice(args);
        sys::output(Some(&self.dir), &cmd)
    }

    /// Whether `init` (or a clone) has run.
    #[must_use]
    pub fn exists(&self) -> bool {
        self.dir.join(".git").exists()
    }

    /// Creates the repo, optionally with an `origin` remote.
    ///
    /// # Errors
    /// When git or the filesystem fails.
    pub fn init(&self, remote: &str) -> Result<(), StoreError> {
        create_private_dir(&self.dir)?;
        self.git(&["init", "-q", "-b", "main"])?;
        if !remote.is_empty() {
            self.git(&["remote", "add", "origin", remote])?;
        }
        Ok(())
    }

    /// Clones an existing snapshot repo (a fresh machine).
    ///
    /// # Errors
    /// When the clone fails.
    pub fn clone_from(url: &str, dir: &Path) -> Result<Self, StoreError> {
        if let Some(parent) = dir.parent() {
            create_private_dir(parent)?;
        }
        let dir_s = dir.to_string_lossy();
        let mut argv = vec!["git"];
        argv.extend_from_slice(sys::git_auth());
        argv.extend_from_slice(&["clone", "-q", "--", url, &dir_s]);
        sys::output(None, &argv)?;
        Ok(Self {
            dir: dir.to_path_buf(),
        })
    }

    /// The `origin` URL, or empty.
    #[must_use]
    pub fn remote(&self) -> String {
        self.git(&["remote", "get-url", "origin"])
            .unwrap_or_default()
    }

    /// An exclusive lock held by the guard, so a timer run and a manual run never collide.
    ///
    /// # Errors
    /// [`StoreError::Locked`] when another run holds it.
    pub fn lock(&self) -> Result<Lock, StoreError> {
        let f = File::options()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(self.dir.join(".git/omaphoenix.lock"))?;
        match f.try_lock() {
            Ok(()) => Ok(Lock {
                _file: f,
                cleared: self.clear_stale_git_locks(),
            }),
            Err(fs::TryLockError::WouldBlock) => Err(StoreError::Locked),
            Err(fs::TryLockError::Error(e)) => Err(e.into()),
        }
    }
}

impl Repo {
    // Only OmaPhoenix writes this repo, and it holds the repo lock, so a git lock now is a crash's leftover.
    fn clear_stale_git_locks(&self) -> Vec<PathBuf> {
        let git = self.dir.join(".git");
        let heads = fs::read_dir(git.join("refs/heads"))
            .into_iter()
            .flatten()
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.extension().is_some_and(|x| x == "lock"));
        ["index.lock", "HEAD.lock", "packed-refs.lock", "config.lock"]
            .iter()
            .map(|n| git.join(n))
            .chain(heads)
            .filter(|p| p.is_file() && fs::remove_file(p).is_ok())
            .collect()
    }
}

fn create_private_dir(dir: &Path) -> io::Result<()> {
    use std::os::unix::fs::DirBuilderExt;
    fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(dir)
}
