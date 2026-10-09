//! "Secrets & credentials in home". Reports file locations and counts only;
//! secret values are never read into a finding.

use std::fs::{self, File};
use std::io::{BufRead, BufReader, Read};
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::sync::LazyLock;

use regex::bytes::Regex;

use crate::audit::{Ctx, Report, Section, shell_quote, walk};

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

/// Token shapes, matched on bytes as Bash's `grep -aE` under `LC_ALL=C` does.
static SECRET_RE: LazyLock<Option<Regex>> = LazyLock::new(|| {
    Regex::new(
        r"(?-u)(AKIA[0-9A-Z]{16}|ghp_[A-Za-z0-9]{36}|github_pat_[A-Za-z0-9_]{40,}|glpat-[A-Za-z0-9_-]{20}|sk-ant-[A-Za-z0-9_-]{20,}|sk-[A-Za-z0-9]{40,}|xox[baprs]-[A-Za-z0-9-]{10,}|-----BEGIN [A-Z ]*PRIVATE KEY-----|(password|passwd|secret|token)=[^ ]{6,})",
    )
    .ok()
});

/// Longest history line examined; the rest of a longer line is skipped.
const MAX_LINE: u64 = 64 * 1024;

/// Largest `~/.docker/config.json` read in full.
const MAX_DOCKER_CONFIG: u64 = 4 * 1024 * 1024;

/// Most loose files listed, as Bash's `head -30`.
const MAX_LOOSE: usize = 30;

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

/// Fails a credential file readable by group or others. Like Bash's
/// `stat -c %a`, a symlink's own mode is read, not its target's.
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

/// Lines that look like secrets, streamed with a bounded line buffer.
fn count_secret_lines(path: &Path) -> std::io::Result<usize> {
    let Some(re) = SECRET_RE.as_ref() else {
        return Ok(0);
    };
    let mut reader = BufReader::new(File::open(path)?);
    let mut line = Vec::new();
    let mut n = 0;
    loop {
        line.clear();
        let read = reader
            .by_ref()
            .take(MAX_LINE)
            .read_until(b'\n', &mut line)?;
        if read == 0 {
            return Ok(n);
        }
        if re.is_match(&line) {
            n += 1;
        }
        if line.last() != Some(&b'\n') && read as u64 == MAX_LINE {
            // Over-long line: skip to its end.
            reader.skip_until(b'\n')?;
        }
    }
}

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

/// Secret-like files under home readable by group or others, at most
/// [`MAX_LOOSE`], without crossing into other filesystems (`find -xdev`).
fn loose_files(home: &Path) -> Vec<PathBuf> {
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

/// Names Bash's `find` matches: `.env`, `.env.*`, `*.pem`, `*.key`, `id_*`
/// (not `*.pub`), `*.kdbx`, `*.ovpn`, `kubeconfig*`, never `*.example` or `*.sample`.
fn secret_like(name: &str) -> bool {
    let matched = name == ".env"
        || name.starts_with(".env.")
        || [".pem", ".key", ".kdbx", ".ovpn"]
            .iter()
            .any(|ext| name.ends_with(ext))
        || (name.starts_with("id_") && !name.ends_with(".pub"))
        || name.starts_with("kubeconfig");
    matched && !name.ends_with(".example") && !name.ends_with(".sample")
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::os::unix::fs::PermissionsExt;
    use std::path::Path;

    use crate::audit::{Ctx, run};
    use crate::contract::Severity;

    fn write(home: &Path, rel: &str, body: &str, mode: u32) {
        let p = home.join(rel);
        if let Some(dir) = p.parent() {
            fs::create_dir_all(dir).ok();
        }
        fs::write(&p, body).ok();
        fs::set_permissions(&p, fs::Permissions::from_mode(mode)).ok();
    }

    fn audit(home: &Path) -> Vec<(Severity, String)> {
        run(&Ctx::for_home(home), &[super::SECTION])
            .findings()
            .iter()
            .map(|f| (f.sev, f.msg.clone()))
            .collect()
    }

    fn tmp() -> tempfile::TempDir {
        tempfile::tempdir().unwrap_or_else(|e| unreachable!("tempdir: {e}"))
    }

    #[test]
    fn pattern_compiles() {
        assert!(
            super::SECRET_RE.is_some(),
            "a bad pattern would skip the check silently"
        );
    }

    #[test]
    fn empty_home_passes() {
        let home = tmp();
        assert_eq!(
            audit(home.path()),
            vec![
                (
                    Severity::Info,
                    "Scanning home for .env / key files readable by others...".into()
                ),
                (
                    Severity::Pass,
                    "No group/world-readable .env/key files found".into()
                ),
            ]
        );
    }

    #[test]
    fn credential_perms_and_plaintext_stores() {
        let home = tmp();
        write(home.path(), ".netrc", "machine x", 0o644);
        write(home.path(), ".aws/credentials", "[default]", 0o600);
        write(home.path(), ".git-credentials", "https://u:t@h", 0o600);
        write(
            home.path(),
            ".docker/config.json",
            r#"{"auths":{"r":{"auth":"eA=="}}}"#,
            0o600,
        );
        let got = audit(home.path());
        assert!(got.contains(&(
            Severity::Fail,
            "~/.netrc perms 644 (readable by others, want 600)".into()
        )));
        assert!(got.contains(&(Severity::Pass, "~/.aws/credentials perms 600".into())));
        assert!(got.contains(&(
            Severity::Warn,
            "~/.git-credentials stores tokens in plaintext".into()
        )));
        assert!(got.contains(&(
            Severity::Warn,
            "~/.docker/config.json contains plaintext registry auth".into()
        )));
    }

    #[test]
    fn fail_names_the_file_and_a_quoted_fix() {
        let home = tmp();
        write(home.path(), ".pgpass", "x", 0o640);
        let report = run(&Ctx::for_home(home.path()), &[super::SECTION]);
        let f = report.findings().iter().find(|f| f.sev == Severity::Fail);
        let f = f.unwrap_or_else(|| unreachable!("a fail"));
        assert_eq!(f.id.as_deref(), Some("secrets.credential_perms"));
        assert_eq!(
            f.fix,
            format!("chmod 600 {}", home.path().join(".pgpass").display())
        );
        assert_eq!(f.targets.len(), 1);
    }

    #[test]
    fn history_counts_secret_lines_without_showing_them() {
        let home = tmp();
        let token = format!("ghp_{}", "a".repeat(36));
        write(
            home.path(),
            ".zsh_history",
            &format!("ls\nexport GH={token}\ncurl -u x token=abcdef12\n"),
            0o600,
        );
        write(home.path(), ".bash_history", "ls\ncd /\n", 0o600);
        let got = audit(home.path());
        assert!(got.contains(&(
            Severity::Warn,
            "~/.zsh_history: 2 line(s) look like secrets/tokens (values not shown)".into()
        )));
        assert!(got.contains(&(Severity::Pass, "~/.bash_history: no obvious secrets".into())));
        assert!(got.iter().all(|(_, m)| !m.contains(&token)));
    }

    #[test]
    fn loose_files_respect_names_modes_and_prunes() {
        let home = tmp();
        write(home.path(), "proj/.env", "K=v", 0o644);
        write(home.path(), "proj/.env.example", "K=", 0o644);
        write(home.path(), "keys/id_ed25519", "k", 0o644);
        write(home.path(), "keys/id_ed25519.pub", "k", 0o644);
        write(home.path(), "keys/private.pem", "k", 0o600);
        write(home.path(), "proj/node_modules/x/test.key", "k", 0o644);
        write(home.path(), ".cache/a.pem", "k", 0o644);
        let report = run(&Ctx::for_home(home.path()), &[super::SECTION]);
        let f = report
            .findings()
            .iter()
            .find(|f| f.id.as_deref() == Some("secrets.loose_files"));
        let f = f.unwrap_or_else(|| unreachable!("loose finding"));
        assert_eq!(f.sev, Severity::Warn);
        let mut listed: Vec<&str> = f.detail.lines().collect();
        listed.sort_unstable();
        assert_eq!(listed, ["~/keys/id_ed25519", "~/proj/.env"]);
        assert_eq!(f.targets.len(), 2);
    }

    #[test]
    fn secret_names() {
        for yes in [
            ".env",
            ".env.prod",
            "a.pem",
            "tls.key",
            "id_rsa",
            "db.kdbx",
            "vpn.ovpn",
            "kubeconfig-dev",
        ] {
            assert!(super::secret_like(yes), "{yes}");
        }
        for no in [
            "env",
            "id_rsa.pub",
            ".env.example",
            "a.pem.sample",
            "keys.txt",
            "mykubeconfig",
        ] {
            assert!(!super::secret_like(no), "{no}");
        }
    }
}
