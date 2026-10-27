//! Restored shell init files must not source files this machine doesn't have (F-023).

use std::fmt::Write as _;
use std::fs;
use std::path::Path;

/// Files a login or interactive shell reads at startup; `uwsm` runs `sh -l`, where sourcing a missing file is fatal.
const SHELL_INIT: &[&str] = &[
    ".profile",
    ".bash_profile",
    ".bash_login",
    ".bashrc",
    ".zprofile",
    ".zshenv",
    ".zlogin",
    ".zshrc",
];

/// Whether `rel` (relative to home) is a shell init file.
pub(super) fn is_shell_init(rel: &str) -> bool {
    SHELL_INIT.contains(&rel)
}

/// Guards each `. <home path>` / `source <home path>` line in `file` whose target is missing in `home`,
/// so it runs only when the file exists. Returns the guarded targets, as `~/…`.
pub(super) fn guard_missing_sources(file: &Path, home: &Path) -> std::io::Result<Vec<String>> {
    let text = fs::read_to_string(file)?;
    let mut guarded = Vec::new();
    let mut out = String::with_capacity(text.len() + 64);
    for line in text.split_inclusive('\n') {
        let body = line.trim_end_matches('\n');
        match sourced_home_path(body) {
            Some(rel) if !home.join(&rel).exists() => {
                let indent = &body[..body.len() - body.trim_start().len()];
                let _ = writeln!(out, "{indent}[ -f \"$HOME/{rel}\" ] && . \"$HOME/{rel}\"");
                guarded.push(format!("~/{rel}"));
            }
            _ => out.push_str(line),
        }
    }
    if !guarded.is_empty() {
        fs::write(file, out)?;
    }
    Ok(guarded)
}

/// The home-relative path of a bare `. file` or `source file` line, if that's all the line does.
fn sourced_home_path(line: &str) -> Option<String> {
    let t = line.trim();
    let rest = t
        .strip_prefix(". ")
        .or_else(|| t.strip_prefix("source "))?
        .trim();
    let unquoted = rest
        .strip_prefix('"')
        .and_then(|r| r.strip_suffix('"'))
        .or_else(|| rest.strip_prefix('\'').and_then(|r| r.strip_suffix('\'')))
        .unwrap_or(rest);
    let rel = unquoted
        .strip_prefix("$HOME/")
        .or_else(|| unquoted.strip_prefix("${HOME}/"))
        .or_else(|| unquoted.strip_prefix("~/"))?;
    let simple = !rel.is_empty()
        && rel
            .chars()
            .all(|c| c.is_alphanumeric() || matches!(c, '/' | '.' | '_' | '-'));
    simple.then(|| rel.to_owned())
}

#[cfg(test)]
mod tests {
    use super::{guard_missing_sources, sourced_home_path};
    use std::fs;

    #[test]
    fn recognises_only_bare_home_sources() {
        assert_eq!(
            sourced_home_path(". \"$HOME/.cargo/env\"").as_deref(),
            Some(".cargo/env")
        );
        assert_eq!(
            sourced_home_path("  source ~/.local/bin/env").as_deref(),
            Some(".local/bin/env")
        );
        assert_eq!(
            sourced_home_path(". \"${HOME}/x/y\"").as_deref(),
            Some("x/y")
        );
        assert_eq!(
            sourced_home_path("[ -f ~/.x ] && . ~/.x"),
            None,
            "already guarded"
        );
        assert_eq!(sourced_home_path(". /etc/profile"), None, "outside home");
        assert_eq!(
            sourced_home_path(". \"$HOME/$X/env\""),
            None,
            "variables are left alone"
        );
        assert_eq!(sourced_home_path("export PATH=$HOME/bin:$PATH"), None);
    }

    #[test]
    fn guards_missing_sources_and_keeps_everything_else() {
        let t = tempfile::tempdir().unwrap_or_else(|e| unreachable!("{e}"));
        let home = t.path();
        fs::create_dir_all(home.join(".present")).unwrap_or_else(|e| unreachable!("{e}"));
        fs::write(home.join(".present/env"), "").unwrap_or_else(|e| unreachable!("{e}"));
        let profile = home.join(".profile");
        let original = ". \"$HOME/.cargo/env\"\n. \"$HOME/.present/env\"\nexport A=1\n";
        fs::write(&profile, original).unwrap_or_else(|e| unreachable!("{e}"));
        let guarded = guard_missing_sources(&profile, home).unwrap_or_default();
        assert_eq!(guarded, ["~/.cargo/env"]);
        assert_eq!(
            fs::read_to_string(&profile).unwrap_or_default(),
            "[ -f \"$HOME/.cargo/env\" ] && . \"$HOME/.cargo/env\"\n. \"$HOME/.present/env\"\nexport A=1\n"
        );
        let ok = std::process::Command::new("sh")
            .args(["-c", &format!(". '{}'", profile.display())])
            .env("HOME", home)
            .status()
            .is_ok_and(|s| s.success());
        assert!(ok, "sh can now load the restored profile");
    }
}
