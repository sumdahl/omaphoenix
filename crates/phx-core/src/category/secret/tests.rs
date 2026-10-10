use super::{Secret, setup_secrets};
use crate::category::{Category, Env};
use phx_contract::Item;
use std::fs;
use std::os::unix::fs::PermissionsExt;

#[test]
fn vault_round_trip_never_stores_plaintext() {
    let t = tempfile::tempdir().unwrap_or_else(|e| unreachable!("{e}"));
    let (home, tree) = (t.path().join("home"), t.path().join("tree"));
    fs::create_dir_all(home.join(".ssh")).ok();
    fs::write(
        home.join(".ssh/id_ed25519"),
        "-----BEGIN OPENSSH PRIVATE KEY-----\nfake\n",
    )
    .ok();
    fs::write(home.join(".ssh/id_ed25519.pub"), "ssh-ed25519 AAAA").ok();
    setup_secrets(&tree.join("secrets"), "correct horse battery")
        .unwrap_or_else(|e| unreachable!("{e}"));
    let log = |_: &str, _: &str| {};
    let env = Env::new(&home, &tree, &log);
    let items = vec![
        Item::new(".ssh/id_ed25519", ""),
        Item::new(".ssh/id_ed25519.pub", ""),
    ];
    Secret
        .capture(&env, &items)
        .unwrap_or_else(|e| unreachable!("{e}"));
    let vault = fs::read(tree.join("secrets/vault.age")).unwrap_or_default();
    assert!(
        !vault.windows(11).any(|w| w == b"PRIVATE KEY"),
        "vault is encrypted"
    );
    let before = fs::read(tree.join("secrets/vault.age")).unwrap_or_default();
    Secret.capture(&env, &items).ok();
    assert_eq!(
        fs::read(tree.join("secrets/vault.age")).unwrap_or_default(),
        before,
        "unchanged secrets aren't re-encrypted"
    );

    let fresh = t.path().join("fresh");
    let wrong = || Ok(zeroize::Zeroizing::new("wrong passphrase!".to_owned()));
    let mut env2 = Env::new(&fresh, &tree, &log);
    env2.passphrase = Some(&wrong);
    let res = Secret.restore(&env2).unwrap_or_default();
    assert!(
        res.failed.iter().any(|f| f.contains("wrong passphrase")),
        "{:?}",
        res.failed
    );

    let right = || Ok(zeroize::Zeroizing::new("correct horse battery".to_owned()));
    env2.passphrase = Some(&right);
    let res = Secret.restore(&env2).unwrap_or_default();
    assert_eq!(res.failed, Vec::<String>::new());
    assert_eq!(
        fs::read_to_string(fresh.join(".ssh/id_ed25519"))
            .ok()
            .as_deref(),
        Some("-----BEGIN OPENSSH PRIVATE KEY-----\nfake\n")
    );
    let mode = |p: &str| fs::metadata(fresh.join(p)).map_or(0, |m| m.permissions().mode() & 0o777);
    assert_eq!(
        (
            mode(".ssh"),
            mode(".ssh/id_ed25519"),
            mode(".ssh/id_ed25519.pub")
        ),
        (0o700, 0o600, 0o644)
    );
}
