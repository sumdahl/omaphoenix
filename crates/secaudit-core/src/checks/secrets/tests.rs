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
        super::history::SECRET_RE.is_some(),
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
        assert!(super::loose::secret_like(yes), "{yes}");
    }
    for no in [
        "env",
        "id_rsa.pub",
        ".env.example",
        "a.pem.sample",
        "keys.txt",
        "mykubeconfig",
    ] {
        assert!(!super::loose::secret_like(no), "{no}");
    }
}
