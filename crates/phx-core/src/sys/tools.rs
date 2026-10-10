use std::collections::BTreeSet;
use std::sync::OnceLock;

use super::exec::capture;

/// Which of `paths` some package owns, asked of pacman in batches.
#[must_use]
pub fn pacman_owned(paths: &[String]) -> BTreeSet<String> {
    let mut owned = BTreeSet::new();
    for chunk in paths.chunks(200) {
        let mut argv = vec!["pacman", "-Qo", "--"];
        argv.extend(chunk.iter().map(String::as_str));
        let Some((_, out)) = capture(&argv) else {
            continue;
        };
        for line in out.lines() {
            if let Some((path, _)) = line.split_once(" is owned by ") {
                owned.insert(path.to_owned());
            }
        }
    }
    owned
}

/// Whether a command is on `PATH`.
#[must_use]
pub fn has(name: &str) -> bool {
    which(name).is_some()
}

/// The first executable `name` on `PATH`.
#[must_use]
pub fn which(name: &str) -> Option<std::path::PathBuf> {
    use std::os::unix::fs::PermissionsExt;
    std::env::split_paths(&std::env::var_os("PATH")?)
        .map(|d| d.join(name))
        .find(|p| {
            std::fs::metadata(p).is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
        })
}

/// git `-c` flags that authenticate https remotes with gh's token.
#[must_use]
pub fn git_auth() -> &'static [&'static str] {
    static AUTH: OnceLock<bool> = OnceLock::new();
    if *AUTH.get_or_init(|| has("gh")) {
        &[
            "-c",
            "credential.helper=",
            "-c",
            "credential.helper=!gh auth git-credential",
        ]
    } else {
        &[]
    }
}
