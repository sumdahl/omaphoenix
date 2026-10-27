//! "Secrets & credentials in home": locations and counts only, never secret values.

use std::fs::{self, File};
use std::io::Read;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;

use crate::audit::{Ctx, Report, Section, shell_quote};

mod history;
mod loose;

use history::count_secret_lines;
use loose::loose_files;

pub(super) const SECTION: Section = Section {
    name: "Secrets & credentials in home",
    run,
};

/// Credential files and dirs, relative to home, that must not be readable by others.
const CREDENTIAL_FILES: &[&str] = &[
    ".aws/credentials",
    ".kube/config",
    ".docker/config.json",
    ".netrc",
    ".git-credentials",
    ".pgpass",
    ".config/gh/hosts.yml",
    ".npmrc",
    ".pypirc",
    ".config/rclone/rclone.conf",
    ".vault-token",
    ".terraform.d/credentials.tfrc.json",
    ".claude.json",
    ".config/hcloud/cli.toml",
    ".gnupg",
];

const HISTORIES: &[&str] = &[
    ".zsh_history",
    ".bash_history",
    ".local/share/fish/fish_history",
];

/// Largest `~/.docker/config.json` read in full.
const MAX_DOCKER_CONFIG: u64 = 4 * 1024 * 1024;

fn run(ctx: &Ctx, r: &mut Report) {
    for rel in CREDENTIAL_FILES {
        check_perm(ctx, r, &ctx.home.join(rel));
    }

    let git_credentials = ctx.home.join(".git-credentials");
    if git_credentials.is_file() {
        r.warn("secrets.git_credentials_plaintext", "~/.git-credentials stores tokens in plaintext")
            .file(&git_credentials)
            .hint("Switch to the keyring: git config --global credential.helper /usr/lib/git-core/git-credential-libsecret, then delete the file");
    }
    let docker = ctx.home.join(".docker/config.json");
    if docker.is_file() && file_contains(&docker, b"\"auth\"") {
        r.warn("secrets.docker_auth_plaintext", "~/.docker/config.json contains plaintext registry auth")
            .file(&docker)
            .hint("Use a credential store (docker-credential-secretservice) and set \"credsStore\" in config.json");
    }

    for rel in HISTORIES {
        let path = ctx.home.join(rel);
        if !path.is_file() {
            continue;
        }
        // Unreadable counts as no secrets, as in Bash.
        let n = count_secret_lines(&path).unwrap_or(0);
        let shown = ctx.tilde(&path);
        if n > 0 {
            r.warn("secrets.shell_history", format!("{shown}: {n} line(s) look like secrets/tokens (values not shown)"))
                .file(&path)
                .hint("Rotate those credentials, then delete the lines; in zsh, setopt HIST_IGNORE_SPACE and prefix secret commands with a space");
        } else {
            r.ok(
                "secrets.shell_history",
                format!("{shown}: no obvious secrets"),
            )
            .file(&path);
        }
    }

    r.info(
        "secrets.loose_files_scan",
        "Scanning home for .env / key files readable by others...",
    );
    let loose = loose_files(&ctx.home);
    if loose.is_empty() {
        r.ok(
            "secrets.loose_files",
            "No group/world-readable .env/key files found",
        );
    } else {
        let detail: Vec<String> = loose.iter().map(|p| ctx.tilde(p)).collect();
        let quoted: Vec<String> = loose
            .iter()
            .map(|p| shell_quote(&p.display().to_string()))
            .collect();
        let f = r.warn(
            "secrets.loose_files",
            "Secret-like files readable by group/others:",
        );
        f.detail(detail.join("\n"))
            .fix(format!("chmod 600 {}", quoted.join(" ")));
        for p in &loose {
            f.file(p);
        }
    }
}

/// Fails a credential file readable by others; reads a symlink's own mode, as `stat -c %a`.
fn check_perm(ctx: &Ctx, r: &mut Report, path: &Path) {
    if !path.exists() {
        return;
    }
    let Ok(meta) = fs::symlink_metadata(path) else {
        return;
    };
    let mode = meta.permissions().mode() & 0o7777;
    let shown = ctx.tilde(path);
    if mode & 0o047 != 0 {
        let want = if path.is_dir() { "700" } else { "600" };
        r.fail(
            "secrets.credential_perms",
            format!("{shown} perms {mode:o} (readable by others, want 600)"),
        )
        .file(path)
        .fix(format!(
            "chmod {want} {}",
            shell_quote(&path.display().to_string())
        ));
    } else {
        r.ok(
            "secrets.credential_perms",
            format!("{shown} perms {mode:o}"),
        )
        .file(path);
    }
}

/// Whether the file contains `needle`, reading at most [`MAX_DOCKER_CONFIG`].
fn file_contains(path: &Path, needle: &[u8]) -> bool {
    let mut buf = Vec::new();
    File::open(path)
        .and_then(|f| f.take(MAX_DOCKER_CONFIG).read_to_end(&mut buf))
        .is_ok_and(|_| buf.windows(needle.len()).any(|w| w == needle))
}

#[cfg(test)]
mod tests;
