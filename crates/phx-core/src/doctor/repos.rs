use std::path::Path;

use super::App;
use crate::category::files::{Walk, walk};
use crate::sys;

/// Directories holding other tools' checkouts, not the user's work.
const REPO_SCAN_SKIP: &[&str] = &[
    ".cache",
    ".local/share",
    ".local/state",
    ".cargo",
    ".rustup",
    "go",
    ".npm",
    ".config/omarchy/plugins",
    ".config/omarchy/themes",
    "node_modules",
    ".git",
    "Downloads",
    ".Trash",
    ".var",
    "snap",
];

impl App<'_> {
    /// Git repositories in `$HOME` whose work exists nowhere else.
    pub(super) fn check_repos(&self, add: &mut dyn FnMut(&str, &str, String)) {
        let (mut risky, mut clean) = (0, 0);
        let mut found = Vec::new();
        walk(&self.home, &mut |p, meta| {
            if !meta.is_dir() {
                return Walk::Continue;
            }
            let rel = p
                .strip_prefix(&self.home)
                .map(|r| r.to_string_lossy().into_owned())
                .unwrap_or_default();
            let name = p
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
            if !rel.is_empty()
                && (rel.matches('/').count() >= 4
                    || name.starts_with('.')
                    || REPO_SCAN_SKIP.iter().any(|s| *s == rel || *s == name))
            {
                return Walk::Skip;
            }
            if p.join(".git").exists() {
                found.push((p.to_path_buf(), rel));
            }
            // A repo can contain other repos.
            Walk::Continue
        });
        for (dir, rel) in found {
            let name = if rel.is_empty() {
                "~ (home itself)".to_owned()
            } else {
                format!("~/{rel}")
            };
            match repo_risk(&dir) {
                Some(msg) => {
                    risky += 1;
                    add("risk", "repos", format!("{name}: {msg}"));
                }
                None => clean += 1,
            }
        }
        if risky == 0 {
            add(
                "ok",
                "repos",
                format!("{clean} git repos in ~ are fully pushed"),
            );
        }
    }
}

fn repo_risk(dir: &Path) -> Option<String> {
    let remotes = sys::output(Some(dir), &["git", "remote"]).unwrap_or_default();
    let commits = sys::output(Some(dir), &["git", "rev-list", "--count", "--all"])
        .ok()
        .filter(|c| c != "0")?;
    if remotes.is_empty() {
        return Some(format!(
            "no remote at all ({commits} commits exist only here)"
        ));
    }
    let mut parts = Vec::new();
    if let Ok(n) = sys::output(
        Some(dir),
        &[
            "git",
            "rev-list",
            "--count",
            "--branches",
            "--not",
            "--remotes",
        ],
    ) && !n.is_empty()
        && n != "0"
    {
        parts.push(format!("{n} unpushed commit(s)"));
    }
    if let Ok(s) = sys::output(Some(dir), &["git", "status", "--porcelain"])
        && !s.is_empty()
    {
        parts.push(format!(
            "{} uncommitted change(s)",
            s.matches('\n').count() + 1
        ));
    }
    (!parts.is_empty()).then(|| parts.join(", "))
}
