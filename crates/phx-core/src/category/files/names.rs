use phx_contract::glob::path_match;

/// Names never worth snapshotting at any depth: VCS internals, caches, logs, backups.
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
    // Spotify state blobs, rewritten as it runs; its prefs stay captured.
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

#[cfg(test)]
mod tests {
    use super::{is_junk, is_secret};

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
}
