# 11: OmaPhoenix port notes

How the Rust port of the Go OmaPhoenix is checked, where it deliberately differs, and what the port found.

## How parity is checked
- **`scripts/omaphoenix-parity`:**
  - Snapshots this machine's real home with the Go and Rust binaries (audit gate off) and diffs the trees.
  - Then restores across implementations into empty sandboxes, including each other's age-encrypted vaults. It uses a fake key in a fake home, never the real ones.
  - It never writes to the real home and never pushes.
- **Result, 2026-10-10:**
  - The snapshot trees are identical (149 MB) and `manifest.json` is byte-identical.
  - The Rust snapshot takes 6 s; the Go one takes 23 s.
  - Each implementation restores the other's snapshot and decrypts its vault.
- **Unit tests** cover the watchdog policy (Go's cases), the store (lock, traversal), every category's edge cases, and the bundle round trip (unpushed + uncommitted work).
- **Contracts:** `tests/fixtures/go` pins selection and manifest behaviour (#3).

## Deliberate differences from Go
- **Spotify `*.bnk` state blobs are junk** (2026-10-10). Spotify rewrites them as it runs, so with the timer on, five of the first eight snapshots on the reference machine were commits of `ad-state-storage.bnk` alone. Spotify's `prefs` are still captured.

| Area | Go | Rust | Why |
|---|---|---|---|
| `bin`, `etc` ownership | loads `pacman -Qlq` (every owned path) into a map | asks `pacman -Qo` about the candidates only, in batches | RUST.md §9: ask, don't dump |
| File comparison and secret scan | reads whole files | streams: size first, then 64 KiB chunks; lines capped at 1 MiB | RUST.md §9 |
| `#` in selection ids | escaped as `\#` since Go 2a0345a (K1) | same | was a Go bug, now fixed in both |
| Undo journal | kept across a no-op restore since Go 2a0345a (K2) | same | was a Go bug, now fixed in both |
| `notified.json` time | RFC 3339 string | Unix seconds; an old file re-notifies once | no date parsing without a dependency |
| Audit before backup | none | gate on by default (docs/04); `OMAPHOENIX_AUDIT=off` | the joint product |
| Gate's filesystem walk | n/a | "Filesystem permissions" reused for a day (`$XDG_STATE_HOME/omaphoenix/gate-daily.json`); `reaudit` always fresh | it was ~11 of 17 s CPU in every timer run |
| Choosing what to back up | `selection.conf` defaults to everything | nothing is decided for the user: the apps open on Backup until a choice is saved; items found later are asked about (`reviewed.txt` in the repo) | the owner wants full control |
| Timer | `timer on <interval>` | same, plus manual-only / interval choice in both apps; the unit always runs the CLI | the app's toggle pointed the unit at omaphoenix-gui |

## Found while porting
- **Memory figures:** the Go "126 MB" peak was `npm ls`, a child process (ADR-0017 correction). Measured on its own, the Rust `snapshot` peaks at 8.7 MB.
- **Edition 2024:** `std::env::set_var` is `unsafe`, and the workspace forbids unsafe code. Variables for child processes (`GIT_TERMINAL_PROMPT=0`, `OMARCHY_PATH`) go through `sys::set_child_env`, and every command is built with `sys::command`.
- **Single-file config entries:** `dst.join("")` adds a trailing slash, so a single-file config entry first failed to copy with "Is a directory". The parity run caught it.

## Not done
- The TUI's Settings section toggles the timer and sets up secrets. Per-category audit policy (`block`/`exclude`/`warn` in docs/04) is fixed at fail = exclude, warn = record. A policy file comes when someone asks for one.
- The re-audit after a restore (docs/04 "Restore side") is `omaphoenix reaudit`: it compares the gate audit now with the one the restored snapshot recorded, listing new and resolved failures. It is Migrate's last step in the TUI and the app.
