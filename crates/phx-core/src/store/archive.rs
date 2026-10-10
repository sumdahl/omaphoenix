use std::fs::{self, File};
use std::io;
use std::path::{Component, Path};
use std::process::Stdio;

use super::{Repo, StoreError};
use crate::sys::{self, CmdError};

impl Repo {
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
