# RUST.md: engineering standards for the joint Rust codebase

**How we write Rust here:** idiomatic, safe, modern, and to the bar a top-tier product company would hold. It applies to every crate in the workspace ([docs/06-rust-rewrite.md](docs/06-rust-rewrite.md)) and to the desktop app's Rust side.

- If code disagrees with this file, fix the code or change the file deliberately, with both owners.
- Agents: read this before writing any Rust. `AGENTS.md` rules still apply.

---

## 1. Toolchain and workspace

- **Edition 2024**, stable toolchain pinned in `rust-toolchain.toml`. Declare an MSRV (`rust-version`) and test it in CI.
- **One workspace.** Shared `[workspace.package]` (version, edition, license, rust-version) and `[workspace.dependencies]`; member crates use `dep.workspace = true`, so no version drift.
- **Lints are set once,** in `[workspace.lints]`:
  ```toml
  [workspace.lints.rust]
  unsafe_code = "forbid"
  missing_docs = "warn"
  unused_must_use = "deny"
  rust_2024_compatibility = "warn"

  [workspace.lints.clippy]
  pedantic = { level = "warn", priority = -1 }
  unwrap_used = "deny"
  expect_used = "warn"        # allowed with a message that proves the invariant
  panic = "deny"
  todo = "deny"
  dbg_macro = "deny"
  print_stdout = "deny"       # binaries print through their UI layer only
  module_name_repetitions = "allow"
  ```
- **CI gate**, all of which must pass:
  - `cargo fmt --check`
  - `cargo clippy --workspace --all-targets -- -D warnings`
  - `cargo test --workspace` (with `cargo-nextest`)
  - `cargo deny check` (licenses, advisories, bans, sources)
  - `cargo doc --no-deps -D warnings`
  - an MSRV build
  - `cargo machete` (unused dependencies)

## 2. Safety
- **No `unsafe`.** `#![forbid(unsafe_code)]` in every crate. If a dependency must use unsafe, prefer audited, widely used crates.
- **No panics in library code.** No `unwrap`, `panic!`, `todo!`, or indexing that can go out of bounds on untrusted input. Use `get()`, `?`, checked arithmetic. `expect("…")` only where an invariant is proven locally, and the message says why it holds.
- **Treat every outside input as hostile.** That covers snapshot repos, manifests, audit JSON, tar entries, `/proc` and command output.
  - Parse into validated types at the edge ("parse, don't validate").
  - Reject path traversal (`..`, absolute paths) the way the Go `untar` does.
  - Bound the size of what you read.
- **Never interpolate into a shell.** Run external commands with `std::process::Command` and argument vectors only; never `sh -c` with joined strings.
- **Secrets:** wrap them in `secrecy::SecretString`/`SecretBox`, zeroize on drop, and keep them out of `Debug` and `Display`. They never reach logs, errors or panics. Keep plaintext in memory only, as the Go age vault does.
- **Root:** the binary never runs as root. Privileged steps go through `sudo`/`pkexec` per file or command, as today.

## 3. Types and design
- **Model the domain in types:**
  - newtypes for IDs and paths (`SnapshotId`, `PluginId`, `HomeRelPath`)
  - enums for closed sets (`Severity`, `Category`, `Verdict`)
  - no stringly typed APIs
- **Make illegal states unrepresentable.** Use typestate or builders where order matters (e.g. a `Restore` that can't run apply before preview).
- **Small traits at real seams only:** `Category` (detect/capture/restore), `Check` (audit), `Store`, `CommandRunner` (so tests can fake the system). No trait with a single implementation "for later".
- **Pure core, effects at the edges.** Policy (watchdog verdicts, selection rules, finding→item mapping, scoring) is pure and needs no I/O to test, like Go's `Watchdog.Observe`.
- **Borrow, don't clone,** across hot paths. `Cow` where it helps. `Arc` only for real sharing across threads.
- **Error types:**
  - libraries use `thiserror` enums with a variant per failure the caller can act on, and keep the source chain (`#[source]`)
  - binaries use `anyhow`/`color-eyre` with `.context("what we were doing")`
  - error messages follow DESIGN.md §3 voice when they reach a user
- **Public API:** `#[must_use]` on results and builders; `#[non_exhaustive]` on public enums and structs that will grow.

## 4. Concurrency and I/O
- **No async runtime by default.** The work is subprocess- and filesystem-bound:
  - std threads plus `crossbeam-channel`, or a scoped thread pool
  - `rayon` for CPU-parallel scans
  - add `tokio` only if a real need appears (many concurrent network streams), and confine it to one crate
- **Every external process call:**
  - has a timeout or the watchdog ([docs/08](docs/08-slow-download-watchdog.md))
  - kills its whole tree on cancel
  - never blocks the UI thread
- **Cancellation is explicit:** a `CancellationToken`-style flag checked between steps, and Ctrl-C leaves the system consistent.
- **File writes are atomic:** write a temporary file, `fsync`, then rename. Resulting permissions are explicit (`0o600` for anything sensitive).

## 5. Observability
- **`tracing`** everywhere, with structured fields (`snapshot_id`, `category`, `item`), never secret values. `tracing-subscriber` is configured only in binaries.
- **Logs** go to `$XDG_STATE_HOME/<product>/logs`, rotated. User-facing output is separate from logs.

## 6. Testing
- **Unit tests** next to the code. Pure policy gets **table-driven** tests and **property tests** (`proptest`), for example: selection Build∘Parse round-trips for any id; the watchdog never defers a fast series.
- **Snapshot tests** (`insta`) for CLI output, JSON contracts and TUI frames.
- **Integration tests** (`assert_cmd`, `tempfile`) against fake homes and fake command runners.
- **Fuzzing** (`cargo-fuzz`) for every parser of outside input: manifests, audit JSONL, tar, `/proc` parsing.
- **Parity:** the Rust version must pass the Go version's fixtures, `scripts/smoke.sh` and the VM rehearsal (`scripts/vm/restore-test.sh`) before cutover.
- **Coverage** reported (`cargo-llvm-cov`). No target number, but every bug fix ships with a test that fails without the fix.

## 7. Dependencies
- **Prefer mature, maintained, widely used crates.** Every new dependency is justified in its PR: what it replaces, maintenance status, license, size.
- **Policy is enforced by `cargo-deny`:**
  - license allowlist compatible with the project license (Q1)
  - no git dependencies in releases
  - advisories fail the build
- **Default stack** (see docs/06): `clap`, `ratatui`/`crossterm`, `serde`/`serde_json`/`toml`, `thiserror`/`anyhow`, `tracing`, `age`, `globset`, `secrecy`/`zeroize`, `rayon`, `insta`, `proptest`, `assert_cmd`.

## 8. Documentation and API
- **Docs and comments:**
  - every public item has a one-line rustdoc comment (`missing_docs` warns)
  - module `//!` headers are one line, or none
  - other comments only for a non-obvious *why*, one line; never restate code or narrate history
- **Structure:** one concern per file, files under ~250 lines, `main.rs` only wires (see AGENTS.md "Clean code").
- **Naming** follows the Rust API Guidelines: `as_`/`to_`/`into_` conventions, iterators over `Vec` returns, `impl AsRef<Path>` inputs.
- **Versioning:**
  - semver, changelog via `git-cliff` or similar
  - the snapshot format and finding contract carry their own versions (see docs/06 compatibility promises)

## 9. Memory: a hard requirement

The user's standing requirement: **"optimize memory usage, very efficient."** Budgets are pass/fail, like tests, and are measured on a real Omarchy machine (the reference: Dell Latitude 7430, ~235 packages, 41 plugins, 57 themes).

| Path | Budget (own peak RSS) | Rust, measured 2026-10-10 |
|---|---|---|
| `snapshot` (full machine) | **≤ 48 MB** | 8.7 MB (real home, 149 MB captured) |
| `restore --dry-run` | **≤ 48 MB** | 5.2 MB |
| `doctor`, `list` | **≤ 16 MB** | 4.8 MB, 4.3 MB |
| `secaudit` full quick audit | **≤ 48 MB** | 7.7 MB |
| TUI, idle | **≤ 24 MB** | `secaudit tui` 4.6 MB |
| Timer snapshot (background) | **≤ 48 MB**, `Nice=19`, idle I/O | as `snapshot` |
| Desktop app, idle (all processes, PSS) | **target ≤ 150 MB, ceiling 250 MB** | Tauri spike 234 MB; Slint spike 46 MB (docs/09) |

**Measure the process, not its children.** An earlier version of this table gave a "Go baseline" of 126 MB for `snapshot`. That was `npm ls`, a child process: `wait4`'s peak RSS counts every child the process waited for, and `npm ls` alone peaks at ~126 MB. Both binaries print their own `VmHWM` when `OMAPHOENIX_REPORT_HWM` or `OMAPHOENIX_AUDIT_REPORT_HWM` is set, and `scripts/peak-rss` reads that.

**Rules that get there:**
- **Stream, never slurp.** Files are hashed and compared in fixed-size chunks (`BufReader`, 64 KiB), never `fs::read` for content of unknown size. The secret scanner works line by line with a bounded line length. JSONL is parsed one line at a time.
- **Compare by hash and size first.** Two files differ if sizes differ; otherwise compare streamed hashes. Never hold both contents.
- **Iterate trees lazily** (`walkdir`-style iterators), and don't collect whole trees into a `Vec`.
- **Ask, don't dump.** Prefer `pacman -Qo <path>` batches over loading `pacman -Qlq` (hundreds of thousands of paths) into a set. If a full set is needed, hold paths as interned, sorted `Box<str>` and binary-search them, not `HashSet<String>`.
- **Bounded concurrency.** Parallel work uses a fixed pool; no unbounded spawning that multiplies buffers.
- **Borrow over clone; `Cow`** where a value is usually unchanged; `Box<str>` / `Arc<str>` for long-lived immutable strings.
- **No hidden caches.** Any cache has a size bound and a reason in a comment.
- **Measured in CI on every push:** `scripts/peak-rss` runs each CLI path against a throwaway home and fails over budget. A path is checked from the moment its subcommand exists. `dhat` heap profiling for investigations.

## 10. Performance
- **Measure before optimising,** with `criterion` benches for scans that touch thousands of files.
- **Budgets:**
  - TUI starts in < 100 ms and never blocks on a terminal query (DESIGN.md §12)
  - snapshot of a typical machine in < 20 s (Go today: ~20 s)
- **Release profile:** `lto = "thin"`, `codegen-units = 1`, `strip = true`, `panic = "abort"` only in binaries.

## 11. Release and supply chain
- **Builds:** static `musl` for amd64/arm64 (`cargo-dist` or the CI matrix), reproducible build flags, SHA256SUMS, and GitHub artifact attestations, as the Go release does.
- **Dependency hygiene:** `cargo-audit` in CI on a schedule, not only on push.
- **Installers:** `install.sh` keeps its rules: everything in `main()`, verify before install, refuse on mismatch.

## 12. Review checklist (every PR)
- [ ] fmt, clippy (pedantic, `-D warnings`), tests, deny and docs pass.
- [ ] No `unsafe`, `unwrap`, `panic!` or shell strings. Secrets wrapped and never logged.
- [ ] Outside input parsed into types at the edge, with bounds and traversal checks.
- [ ] Every external command has a timeout or the watchdog, and tree-kill on cancel.
- [ ] Behaviour proven by a test (unit/property/snapshot/integration), and the VM rehearsal for restore changes.
- [ ] Public API documented; errors actionable and in DESIGN.md voice.
- [ ] No new dependency without the justification in §7.
- [ ] Memory budget (§9) still met for any touched path; no whole-file reads of unknown-size content.
