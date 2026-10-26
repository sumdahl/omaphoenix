//! Commands run before `omaphoenix init` say so, instead of leaking an OS error (F-010).

use std::process::Command;

#[test]
fn list_before_setup_says_to_run_init() {
    let home = tempfile::tempdir().unwrap_or_else(|e| unreachable!("{e}"));
    let out = Command::new(env!("CARGO_BIN_EXE_omaphoenix"))
        .arg("list")
        .env("HOME", home.path())
        .env("XDG_DATA_HOME", home.path().join(".local/share"))
        .env_remove("OMAPHOENIX_REPO")
        .output()
        .unwrap_or_else(|e| unreachable!("omaphoenix: {e}"));
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(!out.status.success());
    assert!(
        err.contains("omaphoenix init") && !err.contains("os error"),
        "{err}"
    );
}
