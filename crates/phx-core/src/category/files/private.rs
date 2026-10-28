//! Git keeps no file modes, so owner-only files are listed in the snapshot and made private again on restore (F-031).

use std::fs;
use std::io;
use std::os::unix::fs::PermissionsExt;
use std::path::{Component, Path};

use super::walk::{Walk, walk};

/// Paths under `files/` that only their owner can read, one per line, relative to the snapshot root.
pub const PRIVATE: &str = "private.txt";

/// Writes [`PRIVATE`] for the captured tree at `root` (removed when nothing is private).
///
/// # Errors
/// When the list can't be written or removed.
pub fn record_private(root: &Path) -> io::Result<()> {
    let files = root.join("files");
    let mut private = Vec::new();
    walk(&files, &mut |path, meta| {
        if meta.is_file()
            && owner_only(meta.permissions().mode())
            && let Ok(rel) = path.strip_prefix(root)
        {
            private.push(rel.to_string_lossy().into_owned());
        }
        Walk::Continue
    });
    let list = root.join(PRIVATE);
    if private.is_empty() {
        return match fs::remove_file(&list) {
            Err(e) if e.kind() != io::ErrorKind::NotFound => Err(e),
            _ => Ok(()),
        };
    }
    private.sort();
    fs::write(list, private.join("\n") + "\n")
}

/// Makes each file in the extracted snapshot's [`PRIVATE`] list owner-only, before anything is restored from it.
pub fn apply_private(root: &Path) {
    let Ok(text) = fs::read_to_string(root.join(PRIVATE)) else {
        return;
    };
    for rel in text.lines().map(Path::new) {
        if !rel.components().all(|c| matches!(c, Component::Normal(_))) {
            continue; // stays inside the snapshot
        }
        let path = root.join(rel);
        if let Ok(m) = fs::symlink_metadata(&path)
            && m.is_file()
        {
            let _ = fs::set_permissions(
                &path,
                fs::Permissions::from_mode(m.permissions().mode() & 0o700),
            );
        }
    }
}

/// No group or other permission bits (the low six).
fn owner_only(mode: u32) -> bool {
    mode.trailing_zeros() >= 6
}

/// Whether `target` lets others read what `src` keeps to its owner.
#[must_use]
pub fn less_private(src: &Path, target: &Path) -> bool {
    match (fs::metadata(src), fs::metadata(target)) {
        (Ok(s), Ok(t)) => owner_only(s.permissions().mode()) && !owner_only(t.permissions().mode()),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::os::unix::fs::PermissionsExt;

    #[test]
    fn owner_only_files_survive_a_round_trip_through_git() {
        let t = tempfile::tempdir().unwrap_or_else(|e| unreachable!("{e}"));
        let root = t.path();
        let hosts = root.join("files/config/.config/gh/hosts.yml");
        let open = root.join("files/config/.config/gh/config.yml");
        fs::create_dir_all(hosts.parent().unwrap_or(root)).unwrap_or_else(|e| unreachable!("{e}"));
        for (p, mode) in [(&hosts, 0o600), (&open, 0o644)] {
            fs::write(p, "x").unwrap_or_else(|e| unreachable!("{e}"));
            fs::set_permissions(p, fs::Permissions::from_mode(mode))
                .unwrap_or_else(|e| unreachable!("{e}"));
        }
        super::record_private(root).unwrap_or_else(|e| unreachable!("{e}"));
        assert_eq!(
            fs::read_to_string(root.join(super::PRIVATE)).unwrap_or_default(),
            "files/config/.config/gh/hosts.yml\n"
        );
        // A clone checks every file out as 644.
        fs::set_permissions(&hosts, fs::Permissions::from_mode(0o644))
            .unwrap_or_else(|e| unreachable!("{e}"));
        fs::write(
            root.join(super::PRIVATE),
            "files/config/.config/gh/hosts.yml\n../escape\n",
        )
        .unwrap_or_else(|e| unreachable!("{e}"));
        super::apply_private(root);
        let mode =
            |p: &std::path::Path| fs::metadata(p).map_or(0, |m| m.permissions().mode() & 0o777);
        assert_eq!((mode(&hosts), mode(&open)), (0o600, 0o644));
        assert!(
            super::less_private(&hosts, &open),
            "a world-readable copy of a private file is not the same file"
        );
        assert!(!super::less_private(&open, &hosts));
        assert!(super::owner_only(0o600) && super::owner_only(0o700) && !super::owner_only(0o640));
    }
}
