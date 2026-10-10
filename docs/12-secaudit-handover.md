# 12: Handing Security Audit to Manish (P4.8, ADR-0013)

What moves, how, and what is left for its owner. **When** is Q7 (#16): both owners decide.

## What moves
| Item | From (this repo) | To (`mnshchtri/secaudit`) |
|---|---|---|
| Engine + contract + store | `crates/secaudit-core` | a Rust workspace in the secaudit repo, published to crates.io as `secaudit-core` |
| Engine CLI + both front ends | `crates/secaudit` (`secaudit`, `secaudit-tui`, `secaudit-ui`) | same workspace; replaces `bin/secaudit`, `bin/secaudit-tui` (Textual) and `bin/secaudit-ui` (gum) one for one, same names and flags, when Manish chooses |
| Installer pieces | `install.sh` (`--sysctl`, `--uninstall`), `share/applications/secaudit.desktop.in`, `share/icons/secaudit.svg`, `extras/sysctl/` | his `install.sh`, `share/`, `extras/` (the desktop entry now uses the new icon) |
| Parity harness | `scripts/secaudit-parity` | same (it diffs Bash and Rust while both exist) |
| Fixtures | `tests/fixtures/secaudit/` | same |
| Port notes | `docs/10-secaudit-port-notes.md`, `docs/patches/` | his docs |

## How
1. **History:** keep it by splitting the two crates out: `git subtree split -P crates/secaudit-core` (and the same for `crates/secaudit`), then merge into his repo. Or copy them, if he prefers a clean start.
2. **Publish** `secaudit-core` (RUST.md §11: no git dependencies in releases).
3. **Switch OmaPhoenix:** `secaudit-core = "x.y"` from crates.io instead of the path. The gate's tests in `phx-core` must stay green.
4. **Delete** the two crates here; this repo becomes OmaPhoenix only.

## Left for the owner
- **Bash quirks S1–S7:** fixes proposed in `docs/patches/` (applies to `8a20e26`). After they land, the Rust port should switch to the fixed behaviour, which today it reproduces for parity.
- **Check ids** (`secrets.credential_perms`, …) were chosen during the port. Revise before 1.0 if wanted, with a `schema` bump if fields change (ADR-0016).
- **A desktop app:** blocked by the same decision as OmaPhoenix's (Q9 stack, Q10 Slint license).
- **Full and deep scans from the TUI** need sudo with a password, and were not tested end to end on the reference machine.
- **The exact ports** were compared in tmux against his apps on a copy of real history (`secaudit-tui`: 14 screens, `secaudit-ui`: 6). Two Textual quirks were left out on purpose: a compact button keeping its first label's width, and the keys panel's longest lists. His `uv`, `gum` and `jq` dependencies are gone.
