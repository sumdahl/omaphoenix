//! Old snapshots keep restoring after the cutover (P4.5): a snapshot repo
//! written by the Go OmaPhoenix (format 1), with an encrypted vault holding
//! a FAKE key, restored by this binary into an empty home.
//! Regenerate the fixture with `scripts/gen-go-snapshot-fixture`.

use std::path::Path;
use std::process::Command;

const BUNDLE: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../tests/fixtures/go/snapshot.bundle"
);
/// Throwaway: it only seals the fixture's fake key.
const PASSPHRASE: &str = "fixture-passphrase-not-secret";

fn run(home: &Path, repo: &Path, args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_omaphoenix"))
        .args(args)
        .env("HOME", home)
        .env("XDG_STATE_HOME", home.join(".local/state"))
        .env("XDG_CONFIG_HOME", home.join(".config"))
        .env("XDG_DATA_HOME", home.join(".local/share"))
        .env("OMAPHOENIX_REPO", repo)
        .env("OMAPHOENIX_PASSPHRASE", PASSPHRASE)
        .output()
        .unwrap_or_else(|e| unreachable!("omaphoenix: {e}"))
}

#[test]
fn restores_a_go_snapshot_with_its_vault() {
    let t = tempfile::tempdir().unwrap_or_else(|e| unreachable!("{e}"));
    let (home, repo) = (t.path().join("home"), t.path().join("repo"));
    std::fs::create_dir_all(&home).unwrap_or_else(|e| unreachable!("{e}"));
    let clone = Command::new("git")
        .args(["clone", "-q", BUNDLE])
        .arg(&repo)
        .status()
        .unwrap_or_else(|e| unreachable!("git: {e}"));
    assert!(clone.success(), "git clone of the fixture bundle");

    let listed = run(&home, &repo, &["list"]);
    assert!(
        listed.status.success(),
        "list: {}",
        String::from_utf8_lossy(&listed.stderr)
    );

    let out = run(
        &home,
        &repo,
        &["restore", "--only", "config,secret", "--no-restart"],
    );
    assert!(
        out.status.success(),
        "restore: {}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );

    let read = |p: &str| {
        std::fs::read_to_string(home.join(p)).unwrap_or_else(|e| unreachable!("{p}: {e}"))
    };
    assert_eq!(
        read(".config/hypr/bindings.conf"),
        "bind = SUPER, Q, exec, kitty\n"
    );
    assert_eq!(read(".config/git/config"), "[user]\n\tname = Fixture\n");
    assert!(
        read(".ssh/id_fake").contains("FAKE-NOT-A-REAL-KEY"),
        "the Go vault decrypts"
    );
    let mode = std::fs::metadata(home.join(".ssh/id_fake"))
        .map(|m| std::os::unix::fs::PermissionsExt::mode(&m.permissions()) & 0o777);
    assert_eq!(mode.ok(), Some(0o600), "private key comes back 600");
}
