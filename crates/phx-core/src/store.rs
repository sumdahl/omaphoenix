//! Snapshots as commits in a git repository: the commit date is the
//! timestamp, `git log` is the timeline, and an old snapshot is read back
//! with `git archive`.

use std::fs::{self, File};
use std::io;
use std::path::{Component, Path, PathBuf};
use std::process::Stdio;

use crate::sys::{self, CmdError};

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
    #[error("another omaphoenix run holds the lock")]
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
pub struct Lock(#[allow(dead_code, reason = "held for its lock")] File);

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

    /// An exclusive lock for the life of the returned guard, so a timer run
    /// and a manual run never write the same tree.
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
            Ok(()) => Ok(Lock(f)),
            Err(fs::TryLockError::WouldBlock) => Err(StoreError::Locked),
            Err(fs::TryLockError::Error(e)) => Err(e.into()),
        }
    }

    /// Commits as the user's own identity when they have one, so a personal
    /// backup repo shows only its owner.
    fn identity(&self) -> &'static [&'static str] {
        if self
            .git(&["config", "user.email"])
            .is_ok_and(|e| !e.is_empty())
        {
            &[]
        } else {
            &[
                "-c",
                "user.name=omaphoenix",
                "-c",
                "user.email=omaphoenix@localhost",
            ]
        }
    }

    /// Stages everything and commits only if the tree changed.
    ///
    /// # Errors
    /// When git fails.
    pub fn commit(&self, msg: &str) -> Result<bool, StoreError> {
        self.git(&["add", "-A"])?;
        if self.git(&["diff", "--cached", "--quiet"]).is_ok() {
            return Ok(false);
        }
        let mut args = self.identity().to_vec();
        args.extend_from_slice(&["commit", "-q", "-m", msg]);
        self.git(&args)?;
        Ok(true)
    }

    /// Pushes to `origin` if there is one.
    ///
    /// # Errors
    /// When the push fails.
    pub fn push(&self) -> Result<(), StoreError> {
        if self.remote().is_empty() {
            return Ok(());
        }
        let mut args = sys::git_auth().to_vec();
        args.extend_from_slice(&["push", "-q", "-u", "origin", "main"]);
        self.git(&args)?;
        Ok(())
    }

    /// Whether `main` has commits `origin` hasn't got (a push that failed
    /// earlier, or one never made). False without a remote.
    #[must_use]
    pub fn unpushed(&self) -> bool {
        if self.remote().is_empty() {
            return false;
        }
        match self.git(&["rev-list", "--count", "origin/main..main"]) {
            Ok(n) => n.trim() != "0",
            // No origin/main yet: never pushed.
            Err(_) => self.git(&["rev-parse", "--verify", "-q", "main"]).is_ok(),
        }
    }

    /// Snapshots, newest first.
    ///
    /// # Errors
    /// When git fails (an empty repo is not an error).
    pub fn list(&self) -> Result<Vec<Snapshot>, StoreError> {
        match sys::lines(Some(&self.dir), &["git", "log", "--format=%h%x09%cI%x09%s"]) {
            Ok(lines) => Ok(lines
                .iter()
                .filter_map(|l| {
                    let mut p = l.splitn(3, '\t');
                    Some(Snapshot {
                        id: p.next()?.into(),
                        time: p.next()?.into(),
                        subject: p.next()?.into(),
                    })
                })
                .collect()),
            Err(e) if e.stderr.contains("does not have any commits") => Ok(Vec::new()),
            Err(e) => Err(e.into()),
        }
    }

    /// The newest snapshot whose id or timestamp starts with `at`; empty
    /// `at` means the latest.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] when nothing matches.
    pub fn resolve(&self, at: &str) -> Result<Snapshot, StoreError> {
        self.list()?
            .into_iter()
            .find(|s| at.is_empty() || s.id.starts_with(at) || s.time.starts_with(at))
            .ok_or_else(|| {
                StoreError::NotFound(if at.is_empty() {
                    "no snapshots yet".into()
                } else {
                    format!("no snapshot matches {at:?}")
                })
            })
    }

    /// Writes the tree of snapshot `id` into `dst`, streaming the archive.
    ///
    /// # Errors
    /// When git fails or an entry would land outside `dst`.
    pub fn extract(&self, id: &str, dst: &Path) -> Result<(), StoreError> {
        let mut child = sys::command("git")
            .args(["archive", "--format=tar", id])
            .current_dir(&self.dir)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;
        let result = child.stdout.take().map_or(Ok(()), |out| untar(out, dst));
        let status = child.wait()?;
        result?;
        if !status.success() {
            return Err(CmdError {
                cmd: format!("git archive {id}"),
                status: format!("{status}"),
                stderr: String::new(),
            }
            .into());
        }
        Ok(())
    }
}

fn create_private_dir(dir: &Path) -> io::Result<()> {
    use std::os::unix::fs::DirBuilderExt;
    fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(dir)
}

/// Unpacks a tar stream into `dst`, refusing absolute paths and `..`.
///
/// # Errors
/// On a bad archive, an escaping path, or a write failure.
pub fn untar(rd: impl io::Read, dst: &Path) -> Result<(), StoreError> {
    let mut archive = tar::Archive::new(rd);
    for entry in archive.entries()? {
        let mut entry = entry?;
        let name = entry.path()?.into_owned();
        if name
            .components()
            .any(|c| !matches!(c, Component::Normal(_) | Component::CurDir))
        {
            return Err(StoreError::Traversal(name.display().to_string()));
        }
        let target = dst.join(&name);
        let kind = entry.header().entry_type();
        if kind.is_dir() {
            fs::create_dir_all(&target)?;
        } else if kind.is_symlink() {
            if let Some(parent) = target.parent() {
                fs::create_dir_all(parent)?;
            }
            if let Some(link) = entry.link_name()? {
                std::os::unix::fs::symlink(link, &target)?;
            }
        } else if kind.is_file() {
            if let Some(parent) = target.parent() {
                fs::create_dir_all(parent)?;
            }
            let mode = entry.header().mode().unwrap_or(0o644) & 0o777;
            let mut out = {
                use std::os::unix::fs::OpenOptionsExt;
                File::options()
                    .create(true)
                    .write(true)
                    .truncate(true)
                    .mode(mode)
                    .open(&target)?
            };
            io::copy(&mut entry, &mut out)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{Repo, StoreError};
    use std::fs;

    fn repo() -> (tempfile::TempDir, Repo) {
        let t = tempfile::tempdir().unwrap_or_else(|e| unreachable!("{e}"));
        let r = Repo {
            dir: t.path().join("repo"),
        };
        r.init("").unwrap_or_else(|e| unreachable!("init: {e}"));
        (t, r)
    }

    #[test]
    fn knows_when_commits_still_need_pushing() {
        let t = tempfile::tempdir().unwrap_or_else(|e| unreachable!("{e}"));
        let bare = t.path().join("remote.git");
        let ok = std::process::Command::new("git")
            .args(["init", "-q", "--bare", "-b", "main"])
            .arg(&bare)
            .status()
            .is_ok_and(|s| s.success());
        assert!(ok, "bare remote");
        let r = Repo {
            dir: t.path().join("repo"),
        };
        r.init(&bare.to_string_lossy())
            .unwrap_or_else(|e| unreachable!("init: {e}"));
        assert!(!r.unpushed(), "nothing committed yet");
        fs::write(r.dir.join("a"), "1").ok();
        assert!(r.commit("one").unwrap_or(false));
        assert!(r.unpushed(), "never pushed");
        r.push().unwrap_or_else(|e| unreachable!("push: {e}"));
        assert!(!r.unpushed(), "in step after a push");
        fs::write(r.dir.join("a"), "2").ok();
        assert!(r.commit("two").unwrap_or(false));
        assert!(r.unpushed(), "a new commit waits for the next push");
    }

    #[test]
    fn commits_only_changes_and_lists_newest_first() {
        let (_t, r) = repo();
        assert!(
            r.list().unwrap_or_default().is_empty(),
            "empty repo lists nothing"
        );
        fs::write(r.dir.join("a"), "1").ok();
        assert!(r.commit("one").unwrap_or(false));
        assert!(!r.commit("same").unwrap_or(true), "no change, no commit");
        fs::write(r.dir.join("a"), "2").ok();
        assert!(r.commit("two").unwrap_or(false));
        let list = r.list().unwrap_or_default();
        assert_eq!(
            list.iter().map(|s| s.subject.as_str()).collect::<Vec<_>>(),
            ["two", "one"]
        );
        assert_eq!(
            r.resolve("").map(|s| s.subject).ok().as_deref(),
            Some("two")
        );
        assert!(matches!(r.resolve("zzz"), Err(StoreError::NotFound(_))));
    }

    #[test]
    fn extracts_a_snapshot_with_modes_and_links() {
        use std::os::unix::fs::PermissionsExt;
        let (t, r) = repo();
        fs::create_dir_all(r.dir.join("d")).ok();
        fs::write(r.dir.join("d/x.sh"), "echo").ok();
        fs::set_permissions(r.dir.join("d/x.sh"), fs::Permissions::from_mode(0o755)).ok();
        std::os::unix::fs::symlink("d/x.sh", r.dir.join("link")).ok();
        r.commit("s").ok();
        let id = r.resolve("").map(|s| s.id).unwrap_or_default();
        let out = t.path().join("out");
        r.extract(&id, &out)
            .unwrap_or_else(|e| unreachable!("extract: {e}"));
        assert_eq!(
            fs::read_to_string(out.join("d/x.sh")).ok().as_deref(),
            Some("echo")
        );
        assert_eq!(
            fs::metadata(out.join("d/x.sh"))
                .map(|m| m.permissions().mode() & 0o111)
                .ok(),
            Some(0o111)
        );
        assert_eq!(fs::read_link(out.join("link")).ok(), Some("d/x.sh".into()));
    }

    #[test]
    fn untar_refuses_escaping_paths() {
        let mut builder = tar::Builder::new(Vec::new());
        let mut h = tar::Header::new_gnu();
        h.set_size(1);
        h.set_mode(0o644);
        // Bypass tar's own path checks to forge the entry name.
        let name = b"../evil";
        if let Some(old) = h.as_old_mut().name.get_mut(..name.len()) {
            old.copy_from_slice(name);
        }
        h.set_cksum();
        builder.append(&h, &b"x"[..]).ok();
        let bytes = builder.into_inner().unwrap_or_default();
        let t = tempfile::tempdir().unwrap_or_else(|e| unreachable!("{e}"));
        let r = super::untar(&bytes[..], &t.path().join("out"));
        assert!(matches!(r, Err(StoreError::Traversal(_))), "{r:?}");
        assert!(!t.path().join("evil").exists());
    }

    #[test]
    fn lock_is_exclusive() {
        let (_t, r) = repo();
        let held = r.lock();
        assert!(held.is_ok());
        assert!(matches!(r.lock(), Err(StoreError::Locked)));
        drop(held);
        assert!(r.lock().is_ok());
    }
}
