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
