use std::cell::RefCell;
use std::fs::{self, File};
use std::io::{self, BufRead, BufReader};
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};

use super::walk::{Walk, walk};
use super::{has_secret_shape, is_junk};
use crate::category::Env;

/// Same size and content, compared in chunks.
#[must_use]
pub fn same_file(a: &Path, b: &Path) -> bool {
    let (Ok(ma), Ok(mb)) = (fs::metadata(a), fs::metadata(b)) else {
        return false;
    };
    if !ma.is_file() || !mb.is_file() || ma.len() != mb.len() {
        return false;
    }
    let (Ok(fa), Ok(fb)) = (File::open(a), File::open(b)) else {
        return false;
    };
    let (mut ra, mut rb) = (
        BufReader::with_capacity(64 * 1024, fa),
        BufReader::with_capacity(64 * 1024, fb),
    );
    loop {
        let (Ok(ba), Ok(bb)) = (ra.fill_buf(), rb.fill_buf()) else {
            return false;
        };
        if ba.is_empty() || bb.is_empty() {
            return ba.is_empty() && bb.is_empty();
        }
        let n = ba.len().min(bb.len());
        if ba.get(..n) != bb.get(..n) {
            return false;
        }
        ra.consume(n);
        rb.consume(n);
    }
}

thread_local! {
    // During a snapshot: (repo, where its previous files were moved), so unchanged files move back.
    static REUSE: RefCell<Option<(PathBuf, PathBuf)>> = const { RefCell::new(None) };
}

/// Turns reuse on for a capture into `repo` from `prev` (or off with `None`).
pub fn set_reuse(dirs: Option<(PathBuf, PathBuf)>) {
    REUSE.with(|r| *r.borrow_mut() = dirs);
}

// Same size and mtime (copies keep the source's mtime), so nothing is read.
fn reusable(src: &Path, dst: &Path) -> Option<PathBuf> {
    let prev = REUSE.with(|r| {
        let r = r.borrow();
        let (repo, prev) = r.as_ref()?;
        Some(prev.join(dst.strip_prefix(repo).ok()?))
    })?;
    let (ms, mp) = (fs::metadata(src).ok()?, fs::symlink_metadata(&prev).ok()?);
    (mp.is_file() && ms.len() == mp.len() && ms.modified().ok()? == mp.modified().ok()?)
        .then_some(prev)
}

/// Copies a file with `perm`, creating parents; during a snapshot an unchanged file is moved back.
///
/// # Errors
/// On any I/O failure.
pub fn copy_file(src: &Path, dst: &Path, perm: u32) -> io::Result<()> {
    if let Some(parent) = dst.parent() {
        fs::create_dir_all(parent)?;
    }
    if let Some(prev) = reusable(src, dst)
        && fs::rename(&prev, dst).is_ok()
    {
        return fs::set_permissions(dst, fs::Permissions::from_mode(perm));
    }
    let mut input = File::open(src)?;
    let mut out = File::options()
        .create(true)
        .write(true)
        .truncate(true)
        .mode(perm)
        .open(dst)?;
    io::copy(&mut input, &mut out)?;
    // Keep the source's mtime: next snapshot can see "unchanged" from metadata.
    if let Ok(t) = input.metadata().and_then(|m| m.modified()) {
        let _ = out.set_modified(t);
    }
    // An existing file keeps its old mode through truncate; set it explicitly.
    fs::set_permissions(dst, fs::Permissions::from_mode(perm))
}

/// Copies `src` into `dst`, skipping junk, secret-shaped, oversized and `skip`ped files; returns the count.
///
/// # Errors
/// When `src` can't be read at all, or a write fails.
pub fn copy_tree(
    env: &Env<'_>,
    src: &Path,
    dst: &Path,
    limit: u64,
    skip: &dyn Fn(&Path, &str, &fs::Metadata) -> bool,
) -> io::Result<usize> {
    fs::symlink_metadata(src)?;
    let mut n = 0;
    let mut err = None;
    walk(src, &mut |path, meta| {
        let rel = path.strip_prefix(src).unwrap_or(path);
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        if (!rel.as_os_str().is_empty() && (is_junk(&name) || skip(rel, &name, meta)))
            || env.audit_excludes(path)
        {
            return Walk::Skip;
        }
        // `dst.join("")` would add a trailing slash; a single-file src copies to dst itself.
        let target = if rel.as_os_str().is_empty() {
            dst.to_path_buf()
        } else {
            dst.join(rel)
        };
        let result = if meta.is_dir() {
            fs::create_dir_all(&target)
        } else if meta.file_type().is_symlink() {
            n += 1;
            fs::read_link(path).and_then(|link| {
                if let Some(parent) = target.parent() {
                    fs::create_dir_all(parent)?;
                }
                std::os::unix::fs::symlink(link, &target)
            })
        } else if meta.is_file() {
            if meta.len() > limit {
                env.warn(&format!(
                    "skip {}: {} MB is over the {} MB file limit",
                    path.display(),
                    meta.len() >> 20,
                    limit >> 20
                ));
                Ok(())
            } else if has_secret_shape(path) {
                env.warn(&format!(
                    "skip {}: contains something shaped like a secret",
                    path.display()
                ));
                Ok(())
            } else {
                n += 1;
                copy_file(path, &target, meta.permissions().mode() & 0o777)
            }
        } else {
            Ok(()) // sockets, devices, fifos
        };
        match result {
            Ok(()) => Walk::Continue,
            Err(e) => {
                err = Some(e);
                Walk::Stop
            }
        }
    });
    err.map_or(Ok(n), Err)
}
