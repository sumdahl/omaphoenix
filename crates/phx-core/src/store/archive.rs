use std::fs::{self, File};
use std::io;
use std::path::{Component, Path};
use std::process::{Child, Stdio};

use super::{Repo, StoreError};
use crate::sys::{self, CmdError};

impl Repo {
    /// Writes the tree of snapshot `id` into `dst`, streaming the archive.
    ///
    /// # Errors
    /// When git fails or an entry would land outside `dst`.
    pub fn extract(&self, id: &str, dst: &Path) -> Result<(), StoreError> {
        let child = sys::command("git")
            .args(["archive", "--format=tar", id])
            .current_dir(&self.dir)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;
        untar_from(child, dst, &format!("git archive {id}"))
    }
}

/// Unpacks `child`'s tar output, then reads it to the end: closing early kills a producer still writing padding (SIGPIPE).
fn untar_from(mut child: Child, dst: &Path, what: &str) -> Result<(), StoreError> {
    let result = match child.stdout.take() {
        Some(mut out) => untar(&mut out, dst).and_then(|()| {
            io::copy(&mut out, &mut io::sink())?;
            Ok(())
        }),
        None => Ok(()),
    };
    let status = child.wait()?;
    result?;
    if !status.success() {
        return Err(CmdError {
            cmd: what.to_owned(),
            status: format!("{status}"),
            stderr: String::new(),
        }
        .into());
    }
    Ok(())
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
    use std::process::{Command, Stdio};

    #[test]
    fn reads_the_producer_to_the_end() {
        let t = tempfile::tempdir().unwrap_or_else(|e| unreachable!("{e}"));
        let mut b = tar::Builder::new(Vec::new());
        let mut h = tar::Header::new_ustar();
        h.set_path("a.txt").unwrap_or_else(|e| unreachable!("{e}"));
        h.set_size(2);
        h.set_mode(0o644);
        h.set_cksum();
        b.append(&h, &b"hi"[..])
            .unwrap_or_else(|e| unreachable!("{e}"));
        let tar = t.path().join("x.tar");
        std::fs::write(&tar, b.into_inner().unwrap_or_default())
            .unwrap_or_else(|e| unreachable!("{e}"));
        // A valid tar, a pause, then more padding than a pipe holds: git archive's shape under load.
        let child = Command::new("sh")
            .args([
                "-c",
                "cat \"$1\"; sleep 0.3; head -c 200000 /dev/zero",
                "sh",
            ])
            .arg(&tar)
            .stdout(Stdio::piped())
            .spawn()
            .unwrap_or_else(|e| unreachable!("{e}"));
        let out = t.path().join("out");
        super::untar_from(child, &out, "producer").unwrap_or_else(|e| unreachable!("{e}"));
        assert_eq!(
            std::fs::read_to_string(out.join("a.txt")).ok().as_deref(),
            Some("hi")
        );
    }
}
