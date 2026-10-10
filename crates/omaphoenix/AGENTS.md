# omaphoenix (CLI and TUI)

The `omaphoenix` binary: the command line, and the terminal app (`omaphoenix tui`, or bare `omaphoenix` on a terminal). All logic lives in `phx-core`; this crate parses, calls and presents. The root AGENTS.md rules apply too.

## Layout
- `main.rs` only sets the child environment, dispatches and maps the result to an exit code.
- `cli.rs` is the clap definition. `commands/` has one file per subcommand. `out.rs` handles printing and notifications, and `prompt.rs` the passphrase prompt. `tui/` is the terminal app.
- A new subcommand means a variant in `cli.rs`, one line in `commands/mod.rs`, and its own file in `commands/`.

## Rules
- **Output is part of the interface.** `list`, `doctor --json` and the restore report are read by scripts and the desktop app. Change them only on purpose, and update the tests.
- **Notifications are for runs nobody watches** (timer, login hook): normal urgency, 15 s timeout, never stuck on screen. No notification when the lock is held, and none when `OMAPHOENIX_NO_NOTIFY` is set (the apps set it because they show results themselves).
- **Unattended runs never prompt.** Without a terminal, the passphrase comes from `OMAPHOENIX_PASSPHRASE` or the command fails with a message saying so.
- **Long actions show progress** in the TUI: a bar and the current step. Destructive keys ask first.
- **Memory:** `doctor` and `list` ≤ 16 MB; TUI idle ≤ 24 MB.

## Testing
- `tests/pty.rs` drives the real TUI in a pseudo-terminal through `tests/pty/pty.rs`. Run it in release (`--release --test pty`); its time budget assumes release.
- `tests/go_snapshot.rs` restores a snapshot the Go version made. It must keep passing.
- Clap's definition is checked by `cli_definition_is_valid`.
