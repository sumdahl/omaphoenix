use std::fs::{self, Metadata};
use std::io::Read;
use std::os::unix::fs::MetadataExt;
use std::path::Path;

/// Small config files are read whole, up to this size.
pub const MAX_TEXT: u64 = 1024 * 1024;

/// A small text file, at most [`MAX_TEXT`] bytes; empty if unreadable.
#[must_use]
pub fn read_text(path: &Path) -> String {
    let mut buf = Vec::new();
    let _ = fs::File::open(path).and_then(|f| f.take(MAX_TEXT).read_to_end(&mut buf));
    String::from_utf8_lossy(&buf).into_owned()
}

/// Pre-order walk like `find -xdev`; `visit` returning `false` stops, `enter` gates descent.
pub fn walk(
    start: &Path,
    mut enter: impl FnMut(&Path, &Metadata) -> bool,
    mut visit: impl FnMut(&Path, &Metadata) -> bool,
) {
    let Ok(meta) = fs::symlink_metadata(start) else {
        return;
    };
    if !visit(start, &meta) || !meta.is_dir() || !enter(start, &meta) {
        return;
    }
    let dev = meta.dev();
    let mut stack: Vec<fs::ReadDir> = fs::read_dir(start).into_iter().collect();
    while let Some(dir) = stack.last_mut() {
        let Some(entry) = dir.next() else {
            stack.pop();
            continue;
        };
        let Ok(entry) = entry else { continue };
        let path = entry.path();
        let Ok(meta) = fs::symlink_metadata(&path) else {
            continue;
        };
        if !visit(&path, &meta) {
            return;
        }
        if meta.is_dir()
            && meta.dev() == dev
            && enter(&path, &meta)
            && let Ok(rd) = fs::read_dir(&path)
        {
            stack.push(rd);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::walk;
    use std::fs;

    #[test]
    fn walk_is_preorder_and_stops_on_request() {
        let t = tempfile::tempdir().unwrap_or_else(|e| unreachable!("{e}"));
        for d in ["a/b", "c"] {
            fs::create_dir_all(t.path().join(d)).ok();
        }
        fs::write(t.path().join("a/b/f"), "").ok();
        let mut seen = Vec::new();
        walk(
            t.path(),
            |_, _| true,
            |p, _| {
                seen.push(
                    p.strip_prefix(t.path())
                        .map(|r| r.display().to_string())
                        .unwrap_or_default(),
                );
                true
            },
        );
        let pos = |s: &str| seen.iter().position(|x| x == s);
        assert_eq!(seen.len(), 5, "{seen:?}");
        assert!(
            pos("a") < pos("a/b") && pos("a/b") < pos("a/b/f"),
            "a's subtree right after a: {seen:?}"
        );
        let mut n = 0;
        walk(
            t.path(),
            |_, _| true,
            |_, _| {
                n += 1;
                n < 2
            },
        );
        assert_eq!(n, 2);
    }
}
