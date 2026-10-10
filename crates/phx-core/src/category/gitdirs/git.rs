use std::fs;
use std::path::Path;
use std::sync::LazyLock;

use regex::Regex;

use super::{GitRef, GitState};
use crate::sys::{self, CmdError};

// scp-style remotes, including host aliases like `git@github.com-personal:o/r.git`.
static SSH_ALIAS: LazyLock<Option<Regex>> =
    LazyLock::new(|| Regex::new(r"^[^@/]+@(github\.com)[^:]*:(.+)$").ok());

/// GitHub ssh remotes as https, so a fresh machine without keys can clone them.
pub(super) fn https_url(url: &str) -> String {
    if let Some(c) = SSH_ALIAS.as_ref().and_then(|re| re.captures(url)) {
        let (host, path) = (
            c.get(1).map_or("", |m| m.as_str()),
            c.get(2).map_or("", |m| m.as_str()),
        );
        return format!("https://{host}/{path}");
    }
    url.strip_prefix("ssh://git@github.com/").map_or_else(
        || url.to_owned(),
        |rest| format!("https://github.com/{rest}"),
    )
}

pub(super) fn git(dir: &Path, args: &[&str]) -> Result<String, CmdError> {
    let mut cmd = vec!["git"];
    cmd.extend_from_slice(args);
    sys::output(Some(dir), &cmd)
}

/// The git state of `dir` when it is the root of its own checkout with an origin.
pub(super) fn inspect(dir: &Path) -> Option<GitState> {
    let top = git(dir, &["rev-parse", "--show-toplevel"]).ok()?;
    if Path::new(&top) != fs::canonicalize(dir).ok()? {
        return None;
    }
    let url = git(dir, &["remote", "get-url", "origin"])
        .ok()
        .filter(|u| !u.is_empty())?;
    let commit = git(dir, &["rev-parse", "HEAD"]).ok()?;
    let count = |args: &[&str]| {
        git(dir, args)
            .ok()
            .and_then(|n| n.parse().ok())
            .unwrap_or(0)
    };
    Some(GitState {
        git_ref: GitRef {
            url: https_url(&url),
            commit,
            ..GitRef::default()
        },
        unpushed: count(&["rev-list", "--count", "HEAD", "--not", "--remotes"]),
        total: count(&["rev-list", "--count", "HEAD"]),
        dirty: git(dir, &["status", "--porcelain"]).is_ok_and(|s| !s.is_empty()),
    })
}
