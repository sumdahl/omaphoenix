//! `secaudit-tui` and `secaudit-ui` in a real terminal (shared checks in
//! tests/pty).

#[path = "../../../tests/pty/pty.rs"]
mod pty;

#[test]
fn secaudit_tui_starts_fast_and_quits_cleanly() {
    let t = tempfile::tempdir().unwrap_or_else(|e| unreachable!("{e}"));
    let r = pty::run(
        env!("CARGO_BIN_EXE_secaudit-tui"),
        t.path(),
        "No scans yet",
        b"q",
    );
    pty::assert_well_behaved(&r, "secaudit-tui");
}

#[test]
fn secaudit_ui_shows_its_menu_and_quits() {
    // secaudit-ui draws inline like gum, not on the alternate screen; esc
    // leaves the menu, as in gum.
    let t = tempfile::tempdir().unwrap_or_else(|e| unreachable!("{e}"));
    let engine = env!("CARGO_BIN_EXE_secaudit");
    let cmd = format!(
        "env SECAUDIT_ENGINE={engine} {}",
        env!("CARGO_BIN_EXE_secaudit-ui")
    );
    let r = pty::run(&cmd, t.path(), "Quit", b"\x1b");
    assert!(
        r.first_frame < pty::BUDGET,
        "first menu after {:?}",
        r.first_frame
    );
    assert!(
        r.output.contains("No scans yet. Start with a full scan"),
        "{}",
        r.output
    );
    assert!(r.exited, "esc did not leave secaudit-ui");
}
