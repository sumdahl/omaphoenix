use std::fs;
use std::path::Path;

use super::is_junk;

/// Total size of regular files under `dir`, skipping junk, stopping once over `limit`.
#[must_use]
pub fn dir_size(dir: &Path, limit: u64) -> (u64, bool) {
    let mut total = 0u64;
    let mut over = false;
    walk(dir, &mut |path, meta| {
        if meta.is_dir()
            && path != dir
            && path
                .file_name()
                .is_some_and(|n| is_junk(&n.to_string_lossy()))
        {
            return Walk::Skip;
        }
        if meta.is_file() {
            total += meta.len();
            if total > limit {
                over = true;
                return Walk::Stop;
            }
        }
        Walk::Continue
    });
    (total, over)
}

/// Whether `dir` holds any non-directory entry.
#[must_use]
pub fn has_files(dir: &Path) -> bool {
    let mut found = false;
    walk(dir, &mut |_, meta| {
        if meta.is_dir() {
            Walk::Continue
        } else {
            found = true;
            Walk::Stop
        }
    });
    found
}

/// What a walk visitor wants next.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Walk {
    /// Go on (and into this directory).
    Continue,
    /// Don't enter this directory.
    Skip,
    /// End the walk.
    Stop,
}

/// Sorted depth-first walk like Go's `filepath.WalkDir`, not following symlinks.
pub fn walk(root: &Path, visit: &mut dyn FnMut(&Path, &fs::Metadata) -> Walk) {
    fn inner(
        path: &Path,
        meta: &fs::Metadata,
        visit: &mut dyn FnMut(&Path, &fs::Metadata) -> Walk,
    ) -> bool {
        match visit(path, meta) {
            Walk::Stop => return false,
            Walk::Skip => return true,
            Walk::Continue => {}
        }
        if !meta.is_dir() {
            return true;
        }
        let Ok(rd) = fs::read_dir(path) else {
            return true;
        };
        let mut entries: Vec<_> = rd.flatten().map(|e| e.path()).collect();
        entries.sort();
        for p in entries {
            if let Ok(m) = fs::symlink_metadata(&p)
                && !inner(&p, &m, visit)
            {
                return false;
            }
        }
        true
    }
    if let Ok(meta) = fs::symlink_metadata(root) {
        inner(root, &meta, visit);
    }
}
