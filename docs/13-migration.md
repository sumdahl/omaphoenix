# 13: Moving to the Rust versions (P4.5 cutover)

For anyone running the Go OmaPhoenix or Manish's Bash/Python Security Audit today. One command does it, and nothing you have is lost.

```sh
curl -fsSL https://raw.githubusercontent.com/sumdahl/omaphoenix-secaudit/<tag>/install.sh | bash
```

(The repo is private, ADR-0022: with access, `gh release download -R sumdahl/omaphoenix-secaudit -p install.sh` then `bash install.sh`.)

## What carries over, unchanged

| You have | After installing | Why it just works |
|---|---|---|
| Go `omaphoenix` in `~/.local/bin` | replaced by the Rust binary, same name | same commands and flags |
| Your snapshot repo (`~/.local/share/omaphoenix/repo`, GitHub remote) | used as is | same format 1; old snapshots restore (CI test `go_snapshot.rs` restores a Go-made snapshot) |
| Encrypted secrets (`secrets/vault.age`) | open with the same passphrase | same age vault format, both directions (`scripts/omaphoenix-parity`) |
| The snapshot timer | keeps running | the unit calls `omaphoenix` at the same path |
| `secaudit`, `secaudit-tui`, `secaudit-ui` (symlinks from Manish's `install.sh`) | replaced by the Rust binaries, same names and flags | `install` replaces the symlinks; his repo checkout is not touched |
| Scan history and accepted risks (`~/.local/state/secaudit`) | shown by all three commands | same layout as the Python and gum apps |
| `/etc/sysctl.d/99-zz-hardening.conf` | kept | the same file; `--sysctl` reinstalls it |

## What changes
- **No more runtime dependencies:** `uv` (Python TUI), `gum` and `jq` are no longer needed. The terminal binaries are static.
- **OmaPhoenix gains** a terminal app (`omaphoenix tui`), an optional desktop app (`omaphoenix gui`), launchers with icons, and the audit before every snapshot (ADR-0021: failing items stay out, warnings are reported).
- **Restores keep their undo journal** even when nothing changed (Go K2, fixed in both).

## Going back
Install the Go release (`v0.3.0-rc.1`) or rerun Manish's `install.sh`. Both read the same repo and history, so going back loses nothing.

## The freeze
- **Go OmaPhoenix:** frozen at `v0.3.0-rc.1`. Bug fixes only, no features; its README points here. Its own freeze gate (G3, the second-machine test) still decides when it is called final.
- **Bash/Python Security Audit:** Manish's repo, his call. The Rust versions replace it one for one at the handover (ADR-0023, docs/12).
