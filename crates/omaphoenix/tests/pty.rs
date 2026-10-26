//! `omaphoenix tui` in a real terminal (shared checks in tests/pty).

#[path = "../../../tests/pty/pty.rs"]
mod pty;

#[test]
fn tui_starts_fast_and_quits_cleanly() {
    let t = tempfile::tempdir().unwrap_or_else(|e| unreachable!("{e}"));
    let r = pty::run(
        &format!("{} tui", env!("CARGO_BIN_EXE_omaphoenix")),
        t.path(),
        "Pulse",
        b"q",
    );
    pty::assert_well_behaved(&r, "omaphoenix tui");
}

#[test]
fn audit_tui_starts_fast_and_quits_cleanly() {
    let t = tempfile::tempdir().unwrap_or_else(|e| unreachable!("{e}"));
    let r = pty::run(
        &format!("{} audit tui", env!("CARGO_BIN_EXE_omaphoenix")),
        t.path(),
        "No scans yet",
        b"q",
    );
    pty::assert_well_behaved(&r, "omaphoenix audit tui");
}

#[test]
fn audit_menu_shows_its_menu_and_quits() {
    // The menu draws inline like gum, not on the alternate screen; esc
    // leaves the menu, as in gum.
    let t = tempfile::tempdir().unwrap_or_else(|e| unreachable!("{e}"));
    let r = pty::run(
        &format!("{} audit menu", env!("CARGO_BIN_EXE_omaphoenix")),
        t.path(),
        "Quit",
        b"\x1b",
    );
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
    assert!(r.exited, "esc did not leave omaphoenix audit menu");
}
