use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};

use crate::audit::walk;

/// Most loose files listed, as Bash's `head -30`.
const MAX_LOOSE: usize = 30;

/// Directories the loose-file scan never enters, relative to home.
const PRUNE_PATHS: &[&str] = &[
    ".cache",
    ".local/share/Trash",
    ".local/share/mise",
    "go",
    ".cargo",
    ".rustup",
    ".npm",
    ".m2",
    ".gradle",
    ".local/lib",
];

/// Directory names never entered anywhere: toolchains ship public test certs.
const PRUNE_NAMES: &[&str] = &["site-packages", "node_modules", ".git", "testdata"];

/// Group/world-readable secret-like files under home, at most [`MAX_LOOSE`].
pub(super) fn loose_files(home: &Path) -> Vec<PathBuf> {
    let prune: Vec<PathBuf> = PRUNE_PATHS.iter().map(|p| home.join(p)).collect();
    let mut found = Vec::new();
    walk(
        home,
        |dir, _| {
            let name = dir.file_name().map(|n| n.to_string_lossy());
            !name.is_some_and(|n| PRUNE_NAMES.contains(&n.as_ref()))
                && !prune.iter().any(|p| p == dir)
        },
        |path, meta| {
            let name = path
                .file_name()
                .map(|n| n.to_string_lossy())
                .unwrap_or_default();
            if meta.is_file() && meta.mode() & 0o044 != 0 && secret_like(&name) {
                found.push(path.to_path_buf());
            }
            found.len() < MAX_LOOSE
        },
    );
    found
}

/// The file names Bash's `find` matches.
pub(super) fn secret_like(name: &str) -> bool {
    let matched = name == ".env"
        || name.starts_with(".env.")
        || [".pem", ".key", ".kdbx", ".ovpn"]
            .iter()
            .any(|ext| name.ends_with(ext))
        || (name.starts_with("id_") && !name.ends_with(".pub"))
        || name.starts_with("kubeconfig");
    matched && !name.ends_with(".example") && !name.ends_with(".sample")
}
