//! Shared file handling for categories: junk and secret names, secret-shaped
//! content, bounded tree copies, and streamed file comparison (RUST.md §9:
//! files are compared by size, then in chunks, never both read whole).

use std::fs::{self, File};
use std::io::{self, BufRead, BufReader, Read};
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::Path;
use std::sync::LazyLock;

use phx_contract::glob::path_match;
use regex::bytes::Regex;

use super::Env;

/// Names never worth snapshotting at any depth: VCS internals, caches, logs,
/// histories, editor and upgrade backups, our own restore backups.
pub const JUNK: &[&str] = &[
    ".git",
    "node_modules",
    "__pycache__",
    ".serena",
    "_backups",
    "Cache",
    "Cache*",
    "cache",
    "*Cache",
    "*.log",
    "*.sock",
    "*history*",
    "*.bak*",
    "*.backup",
    "*.backup.*",
    "*.jsonl",
    "sessions",
    "logs",
    "event-log-metadata",
    "telemetry",
    // Spotify's state blobs (ad state, offline keys): rewritten as it runs,
    // so every timer snapshot became a new commit. Its prefs stay captured.
    "*.bnk",
    "*.omarchy-upgrade-to-*",
    "*.pre-restore.*",
    ".DS_Store",
];

/// Names skipped outright: encrypted secrets are a separate, opt-in category.
pub const SECRET_NAMES: &[&str] = &[
    "*token*",
    "*cred*",
    "*secret*",
    "*.env",
    ".env*",
    "*.pem",
    "*.key",
    "id_*",
    ".netrc",
    ".pgpass",
    ".git-credentials",
    ".claude.json",
    ".Xauthority",
    ".ICEauthority",
    ".wget-hsts",
    ".lesshst",
    ".viminfo",
    ".zcompdump*",
    ".sudo_as_admin_successful",
];

/// Config files are small; anything larger under `~/.config` is a database,
/// a binary or a dump. Plugins and themes legitimately ship wallpapers.
pub const MAX_CONFIG_FILE: u64 = 5 << 20;
/// Limit for plugin, theme, font and icon files.
pub const MAX_ASSET_FILE: u64 = 50 << 20;

/// Vendor token shapes, not keywords: configs and READMEs legitimately say
/// "password" and "token".
pub static SECRET_SHAPE: LazyLock<Option<Regex>> = LazyLock::new(|| {
    Regex::new(r"gh[pousr]_[A-Za-z0-9]{36,}|github_pat_[A-Za-z0-9_]{40,}|sk-[A-Za-z0-9]{32,}|sk-ant-[A-Za-z0-9_-]{40,}|AKIA[0-9A-Z]{16}|xox[baprs]-[A-Za-z0-9-]{10,}|BEGIN (RSA |OPENSSH |EC |DSA |PGP )?PRIVATE KEY|AIza[0-9A-Za-z_-]{35}|glpat-[A-Za-z0-9_-]{20,}").ok()
});

/// A key named like a credential assigned a long opaque value, in TOML,
/// INI, env or JSON form. Found the hard way: wrangler keeps OAuth tokens in
/// plain TOML. 16+ characters with no spaces, so prose doesn't match.
static SECRET_ASSIGNMENT: LazyLock<Option<Regex>> = LazyLock::new(|| {
    Regex::new(r#"(?i)^[ \t]*(export[ \t]+)?"?[a-z0-9_.-]*(token|secret|passw(or)?d|passphrase|api[_-]?key|access[_-]?key|private[_-]?key|client[_-]?secret|credential)s?"?[ \t]*[:=][ \t]*"?([A-Za-z0-9+/=._~-]{16,})"?[ \t]*,?[ \t]*$"#).ok()
});

/// Values that are clearly not secrets even though the key name says so.
static NOT_SECRET_VALUE: LazyLock<Option<Regex>> = LazyLock::new(|| {
    Regex::new(r"(?i)^(true|false|null|none|changeme|your[_-]|example|placeholder|xxx+|\$\{|<)")
        .ok()
});

/// Source and docs say `token: someVariable`; dropping a plugin's `.qml`
/// would break it on restore, so the key-name rule is for config and data
/// only. Vendor shapes are still checked everywhere.
const CODE_OR_DOC_EXT: &[&str] = &[
    "qml", "js", "mjs", "cjs", "ts", "tsx", "jsx", "py", "go", "rs", "c", "h", "cpp", "java", "kt",
    "rb", "php", "swift", "cs", "md", "markdown", "rst", "adoc", "txt", "html", "css", "scss",
];

/// Longest line examined by the secret scanner; longer ones are scanned in pieces.
const MAX_SCAN_LINE: u64 = 1 << 20;

/// Whether `name` matches any of Go's `filepath.Match` patterns.
#[must_use]
pub fn match_any(name: &str, patterns: &[&str]) -> bool {
    patterns.iter().any(|p| path_match(p, name) == Some(true))
}

/// A junk name ([`JUNK`]).
#[must_use]
pub fn is_junk(name: &str) -> bool {
    match_any(name, JUNK)
}

/// A secret-looking name ([`SECRET_NAMES`]).
#[must_use]
pub fn is_secret(name: &str) -> bool {
    match_any(name, SECRET_NAMES)
}

/// Whether the file holds something shaped like a secret. Streams the file
/// line by line with a bounded line length.
#[must_use]
pub fn has_secret_shape(path: &Path) -> bool {
    let (Some(shape), Some(assign), Some(not_secret)) = (
        SECRET_SHAPE.as_ref(),
        SECRET_ASSIGNMENT.as_ref(),
        NOT_SECRET_VALUE.as_ref(),
    ) else {
        return false;
    };
    let Ok(file) = File::open(path) else {
        return false;
    };
    let mut reader = BufReader::new(file);
    let ext = path
        .extension()
        .map(|e| e.to_string_lossy().to_lowercase())
        .unwrap_or_default();
    let mut code_or_doc = CODE_OR_DOC_EXT.contains(&ext.as_str());
    let mut line = Vec::new();
    let mut first = true;
    loop {
        line.clear();
        let Ok(n) = reader
            .by_ref()
            .take(MAX_SCAN_LINE)
            .read_until(b'\n', &mut line)
        else {
            return false;
        };
        if n == 0 {
            return false;
        }
        if first {
            code_or_doc |= line.starts_with(b"#!");
            first = false;
        }
        if shape.is_match(&line) {
            return true;
        }
        if !code_or_doc {
            // Like Go's multiline `$`: only the newline is stripped, so a CRLF line
            // doesn't match, exactly as it didn't in Go.
            let trimmed = line.strip_suffix(b"\n").unwrap_or(&line);
            if let Some(m) = assign.captures(trimmed).and_then(|c| c.get(4))
                && !not_secret.is_match(m.as_bytes())
            {
                return true;
            }
        }
    }
}

/// Whether the file starts with the ELF magic: a compiled executable, which
/// belongs to its installer, not a snapshot.
#[must_use]
pub fn is_binary(path: &Path) -> bool {
    let mut magic = [0u8; 4];
    File::open(path)
        .and_then(|mut f| f.read_exact(&mut magic))
        .is_ok()
        && &magic == b"\x7fELF"
}

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

/// `lstat` succeeds.
#[must_use]
pub fn exists(path: &Path) -> bool {
    fs::symlink_metadata(path).is_ok()
}

thread_local! {
    /// While a snapshot captures: (the repo, where its previous files were
    /// moved). `copy_file` takes an unchanged file back from there instead
    /// of copying it again, so a snapshot writes only what changed.
    static REUSE: std::cell::RefCell<Option<(std::path::PathBuf, std::path::PathBuf)>> =
        const { std::cell::RefCell::new(None) };
}

/// Turns reuse on for a capture into `repo` from `prev` (or off with `None`).
pub fn set_reuse(dirs: Option<(std::path::PathBuf, std::path::PathBuf)>) {
    REUSE.with(|r| *r.borrow_mut() = dirs);
}

/// The previous snapshot's copy of `dst`, if it still matches `src`: same
/// size and modified time (copies keep the source's mtime), so nothing is read.
fn reusable(src: &Path, dst: &Path) -> Option<std::path::PathBuf> {
    let prev = REUSE.with(|r| {
        let r = r.borrow();
        let (repo, prev) = r.as_ref()?;
        Some(prev.join(dst.strip_prefix(repo).ok()?))
    })?;
    let (ms, mp) = (fs::metadata(src).ok()?, fs::symlink_metadata(&prev).ok()?);
    (mp.is_file() && ms.len() == mp.len() && ms.modified().ok()? == mp.modified().ok()?)
        .then_some(prev)
}

/// Copies a file, creating parents, with `perm`. During a snapshot an
/// unchanged file is moved back from the previous snapshot, not rewritten.
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

/// Total size of regular files under `dir`, skipping junk dirs, stopping
/// once over `limit`. Returns (size, over).
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

/// Lexically ordered, depth-first walk like Go's `filepath.WalkDir`
/// (which sorts each directory), not following symlinks. `visit` sees
/// `root` too. Unreadable directories are skipped.
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

/// Copies `src` into `dst`, skipping junk, secret-shaped files, files over
/// `limit`, and whatever `skip(rel, name, meta)` rejects. Returns the count
/// of files copied.
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

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp() -> tempfile::TempDir {
        tempfile::tempdir().unwrap_or_else(|e| unreachable!("{e}"))
    }

    #[test]
    fn patterns_compile() {
        assert!(
            SECRET_SHAPE.is_some() && SECRET_ASSIGNMENT.is_some() && NOT_SECRET_VALUE.is_some()
        );
    }

    #[test]
    fn names() {
        for j in [
            ".git",
            "Cache",
            "GPUCache",
            "x.log",
            "zsh_history",
            "a.bak1",
            "f.pre-restore.2026",
            "ad-state-storage.bnk",
        ] {
            assert!(is_junk(j), "{j}");
        }
        for s in [
            "github-token",
            ".env",
            "id_ed25519",
            "server.key",
            ".zcompdump-host",
        ] {
            assert!(is_secret(s), "{s}");
        }
        assert!(!is_junk("hyprland.conf") && !is_secret("hyprland.conf"));
    }

    #[test]
    fn secret_shapes_like_go() {
        let t = tmp();
        let check = |name: &str, body: &str| {
            let p = t.path().join(name);
            fs::write(&p, body).ok();
            has_secret_shape(&p)
        };
        assert!(check("a.conf", &format!("x = ghp_{}\n", "a".repeat(36))));
        assert!(check(
            "wrangler.toml",
            "oauth_token = \"abcdefghijklmnop1234\"\n"
        ));
        assert!(check(
            "cfg.json",
            "  \"api_key\": \"ABCDEFGHIJKLMNOPQRST\",\n"
        ));
        assert!(!check(
            "hyprlock.conf",
            "placeholder_text = Enter password\n"
        ));
        assert!(!check("x.conf", "token = changeme_please_now_1234\n"));
        assert!(!check("x.conf", "use_token = true\n"));
        assert!(!check(
            "Plugin.qml",
            "property string token: \"abcdefghijklmnop1234\"\n"
        ));
        assert!(!check("run.sh", "#!/bin/sh\nTOKEN=abcdefghijklmnop1234\n"));
        assert!(
            check("k.md", "-----BEGIN OPENSSH PRIVATE KEY-----\n"),
            "vendor shapes are checked in docs too"
        );
    }

    #[test]
    fn same_file_streams() {
        let t = tmp();
        let (a, b, c) = (t.path().join("a"), t.path().join("b"), t.path().join("c"));
        let big = "x".repeat(200_000);
        fs::write(&a, &big).ok();
        fs::write(&b, &big).ok();
        fs::write(&c, format!("{}y", &big[1..])).ok();
        assert!(same_file(&a, &b));
        assert!(!same_file(&a, &c));
        assert!(!same_file(&a, &t.path().join("missing")));
    }

    #[test]
    fn copy_tree_skips_junk_secrets_and_big_files() {
        let t = tmp();
        let src = t.path().join("src");
        for (p, body) in [
            ("keep.conf", "a"),
            ("sub/x.conf", "b"),
            (".git/HEAD", "c"),
            ("leak.conf", "k = ghp_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"),
        ] {
            let f = src.join(p);
            fs::create_dir_all(f.parent().unwrap_or(&src)).ok();
            fs::write(&f, body).ok();
        }
        fs::write(src.join("big.bin"), vec![0u8; 2048]).ok();
        let logs = std::cell::RefCell::new(Vec::new());
        let log = |l: &str, m: &str| logs.borrow_mut().push(format!("{l} {m}"));
        let env = Env::new(t.path(), t.path(), &log);
        let dst = t.path().join("dst");
        let n = copy_tree(&env, &src, &dst, 1024, &|_, _, _| false).unwrap_or(0);
        assert_eq!(n, 2);
        assert!(
            dst.join("sub/x.conf").exists()
                && !dst.join(".git").exists()
                && !dst.join("leak.conf").exists()
        );
        assert!(!dst.join("big.bin").exists());
        assert_eq!(logs.borrow().len(), 2, "{:?}", logs.borrow());
    }

    /// A config entry can be a single file, not a directory.
    #[test]
    fn a_snapshot_moves_unchanged_files_back_and_copies_changed_ones() {
        let t = tempfile::tempdir().unwrap_or_else(|e| unreachable!("{e}"));
        let (src, repo, prev) = (
            t.path().join("src"),
            t.path().join("repo"),
            t.path().join("prev"),
        );
        for d in [&src, &repo, &prev] {
            fs::create_dir_all(d).unwrap_or_else(|e| unreachable!("{e}"));
        }
        fs::write(src.join("same"), "kept").ok();
        fs::write(src.join("changed"), "new!").ok();
        // The last snapshot's copies, as copy_file left them (source mtime kept).
        super::copy_file(&src.join("same"), &prev.join("same"), 0o644)
            .unwrap_or_else(|e| unreachable!("{e}"));
        fs::write(prev.join("changed"), "old").ok();
        let inode = |p: &std::path::Path| {
            std::os::unix::fs::MetadataExt::ino(
                &fs::metadata(p).unwrap_or_else(|e| unreachable!("{e}")),
            )
        };
        let reused = inode(&prev.join("same"));

        super::set_reuse(Some((repo.clone(), prev.clone())));
        super::copy_file(&src.join("same"), &repo.join("same"), 0o644)
            .unwrap_or_else(|e| unreachable!("{e}"));
        super::copy_file(&src.join("changed"), &repo.join("changed"), 0o644)
            .unwrap_or_else(|e| unreachable!("{e}"));
        super::set_reuse(None);

        assert_eq!(
            inode(&repo.join("same")),
            reused,
            "unchanged: the old copy moved back, not rewritten"
        );
        assert!(!prev.join("same").exists());
        assert_eq!(
            fs::read_to_string(repo.join("changed")).unwrap_or_default(),
            "new!",
            "changed: copied"
        );
        assert!(
            prev.join("changed").exists(),
            "the stale copy is left behind to be dropped"
        );
    }

    #[test]
    fn copy_tree_copies_a_single_file() {
        let t = tmp();
        let src = t.path().join("brave-flags.conf");
        fs::write(&src, "--x").ok();
        let log = |_: &str, _: &str| {};
        let env = Env::new(t.path(), t.path(), &log);
        let dst = t.path().join("out/brave-flags.conf");
        assert_eq!(
            copy_tree(&env, &src, &dst, 1024, &|_, _, _| false).ok(),
            Some(1)
        );
        assert_eq!(fs::read_to_string(&dst).ok().as_deref(), Some("--x"));
    }
}
