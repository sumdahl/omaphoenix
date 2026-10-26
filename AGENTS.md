# AGENTS.md

Rules for any coding agent (Claude Code or other) working in this repo. Read it before touching anything. [RUST.md](RUST.md) has the long form; [DESIGN.md](DESIGN.md) covers how the apps look and speak.

Each crate and app has its own `AGENTS.md` with the rules for that part: its contracts, traps and tests. Read the one nearest the file you're editing as well as this one. When the two conflict, this file wins.

| Path | What it is |
|---|---|
| `crates/phx-contract` | snapshot formats (`manifest.json`, `selection.conf`) |
| `crates/phx-core` | the OmaPhoenix engine |
| `crates/omaphoenix` | OmaPhoenix CLI and TUI |
| `crates/phx-audit` | the audit engine and finding contract |
| `crates/secaudit` | Security Audit CLI, TUI, gum UI, AI |
| `apps/` | the two Slint desktop apps |
| `crates/omaphoenix-desktop`, `ui/desktop`, `spikes/` | decision records; don't extend |

## Orientation

1. `README.md`, then `docs/05-roadmap.md`. The first task whose status isn't `done` is next.
2. Decisions live in `docs/adr/`. Open questions live in `docs/07-decisions.md`; never build on one, ask instead. A new decision gets a new ADR. Accepted ADRs are superseded, never edited.
3. The Go OmaPhoenix (`~/personal/omaphoenix`) is frozen and is the parity reference only. Manish's Python/gum secaudit (`mnshchtri/secaudit`) is the reference for the exact ports (`docs/10`, `docs/12`).

## Hard rules

- **Owners.** OmaPhoenix: Sumiran Dahal. secaudit: Manish Karki. Never push to `mnshchtri/secaudit`; fetch it read-only.
- **Authorship:** `git -c user.email=sumirandahal46@gmail.com commit`, no `Co-Authored-By` trailer.
- **The repo is private** (ADR-0022). Ask before pushing; never tag a release without an explicit OK.
- **`main` is protected.** Nobody pushes to it directly, admins included. Work on a branch (`feat/…`, `fix/…`), push it, and open a pull request with `gh pr create`. It merges only with:
  - one approving review from someone other than the last pusher;
  - green CI (`check`, `gui`, `msrv`) on an up-to-date branch;
  - all conversations resolved.
  
  History stays linear (squash or rebase merges), and force-pushes and deleting `main` are blocked. Release tags are still pushed by hand after the release PR merges.
- **Secrets.** Never print a token, key or `.env` value: not in output, logs, errors, fixtures or docs. Name the file, never its content. Tests use generated fake keys only.
- **The audit is read-only.** Nothing applies a fix without a human confirming it.
- **Unattended runs never prompt.** The timer has no terminal: no sudo, no passphrase prompt, no blocking.
- **Restores are tested in the VM or a temp `HOME`.** On the real home, only `--dry-run`. The VM test and its rules are in docs/14.

## Rust: safe and idiomatic

These are enforced by `[workspace.lints]`. Never silence one to get green; fix the code.

- **No `unsafe`** (`unsafe_code = "forbid"`).
- **No panics.** No `unwrap`, `panic!`, `todo!`, `dbg!`, or indexing that can go out of bounds. Use `?`, `get()`, `let … else`, and checked or saturating arithmetic. Use `expect("why it holds")` only for a local, provable invariant.
- **Errors carry meaning.** Library crates (`*-core`, `phx-contract`) return `thiserror` enums or `Result<_, String>` with a user-readable message. Binaries turn them into one `error: …` line and an exit code. Don't swallow errors with `let _ =` unless failure is genuinely harmless (a best-effort notification, a cleanup).
- **Outside input is hostile.** That covers snapshot repos, manifests, JSON, tar entries, `/proc` and command output. Parse it into types at the edge, bound its size, and reject `..` and absolute paths.
- **No shell strings.** Use `Command::new(prog).args([...])`. The one exception is running a finding's fix command, which is shell by nature: `bash -c`, only after the user confirms.
- **Secrets in memory** go in `Zeroizing<…>`. Keep them out of `Debug`/`Display`, and an API key never goes on argv.
- **Every external command** has a timeout or the watchdog (`docs/08`) and never blocks a UI thread.
- **Writes are atomic:** temp file, then rename. Set permissions explicitly (`0o600` for anything sensitive).
- **Types over strings.** Use enums for closed sets and newtypes where mixing values up is a real risk. Borrow (`&str`, `&Path`, `impl AsRef<Path>`) instead of cloning.
- **Memory budgets in RUST.md §9 are pass/fail.** Stream files, never slurp ones of unknown size. Collect a whole tree only when you must.
- **No new dependency** without saying what it replaces, its license and its maintenance status. `cargo deny` must stay green.

## Clean code

- **One concern per file.** `main.rs` only declares modules and wires: parse args, dispatch, map the result to an exit code. Commands, CLI definitions, output and prompts each get their own file (see `crates/omaphoenix/src/`).
- **Small files.** Aim for under ~250 lines. Past that, split along a real seam into a folder module (`foo/mod.rs` plus `foo/bar.rs`) and re-export so callers don't change. Data tables and long string literals (prompts, Slint markup) may run longer.
- **Small functions.** One job each. A `match` that dispatches calls functions; it doesn't contain them. If a function needs `#[allow(clippy::too_many_lines)]`, split it instead. `too_many_arguments` is allowed only for passing CLI flags straight through.
- **No UI in engines.** `phx-core`, `phx-contract` and `phx-audit` never print, prompt or draw. They take callbacks (`log`, `progress`) and return data. CLI, TUI, gum UI and GUI are thin layers over the same engine.
- **No speculative code.** No trait with one implementation, no config for a constant, no "for later" scaffolding. Delete dead code; don't comment it out.
- **Names say what, comments say why.** Write a comment only for a non-obvious reason (a trap, a parity quirk, a measured number), as one line. Never restate the code, narrate history, or leave banner comments. Public items get a one-line `///`, because `missing_docs` warns. Module `//!` headers are one line or none.
- **Tests live next to the code they test** (`#[cfg(test)] mod tests`, or a sibling `tests.rs` when large). Every bug fix ships with a test that fails without it.
- **Match the surrounding code.** Same error style, same naming, same idioms as the file you're in.

## Verification (all must pass before a commit)

`scripts/check` runs all of it in order (`--quick`: fmt, clippy and tests). The steps:

```sh
cargo fmt --all                                         # run last, after any edit
cargo clippy --workspace --all-targets -- -D warnings   # zero warnings
cargo test --workspace
cargo test --release -p omaphoenix --test pty
(cd apps/omaphoenix-gui && cargo clippy --all-targets -- -D warnings && cargo test)
(cd apps/omaphoenix-audit-gui   && cargo clippy --all-targets -- -D warnings && cargo test)
cargo deny check
scripts/peak-rss                                        # memory budgets
```

- **Before a release that changes restore, capture or secrets:** the full-restore VM test. Build the test snapshots with `scripts/vm/make-test-snapshot`, then run `scripts/vm/restore-test.sh --release <rc>`. It's a local check only (GitHub's runners have no KVM); scenarios and results are in docs/14.
- **Port or engine changes:** `scripts/audit-parity` and `scripts/omaphoenix-parity`. TUI screen changes are compared cell by cell against the original (`docs/10`).
- **Behaviour is proven by a test**, using a fake `HOME`, a fake secret or a fake command runner. "It compiles" is not done.
- **Pipe safety:** use `set -o pipefail` before piping any test command, or a failure gets hidden.
- **GUI screenshots** use a headless Hyprland output, and keys target the test window by pid. Afterwards, always `hyprctl reload` and `hyprctl output remove`.

## Findings

A bug or surprise found while testing or using the products goes in `docs/findings/` (copy `template.md`, add a row to its index): exact message, safe reproduction, cause, workaround, planned fix and its test. When it is fixed, update the status there; don't delete the file.

## Docs

Short sentences, tables for facts. Update the roadmap status when a task changes state.
