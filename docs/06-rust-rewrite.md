# 06: Joint Rust rewrite (Phase 4)

## Why Rust, and why last

**Why Rust:**
- Memory safety and strong types suit a tool that runs as root and parses untrusted files.
- It produces one static binary, like Go today.
- The CLI/TUI ecosystem is excellent (clap, ratatui).
- Both owners want it, and coding agents work well with Rust's compiler feedback.

**Why last:**
- The Go and Bash versions are where the hard facts are being found (see OmaPhoenix `docs/08-implementation-notes.md`: pacman exit codes, deterministic bundles, terminal queries, `/etc` never-lists).
- A rewrite before those are found would rediscover them painfully.
- Porting proven behaviour with parity tests is far cheaper.

**Risk to watch:** a rewrite stalls feature work for weeks. Keep the Go version shippable until cutover (P4.5).

## Products and ownership (ADR-0012, ADR-0013)

| Crate / app | What | Owner |
|---|---|---|
| `secaudit-core` | Audit engine (checks as modules), finding contract types, scoring, accepted-risk store | Manish Karki after the handover |
| `secaudit` | Standalone Security Audit: CLI, TUI, desktop app | Manish Karki after the handover |
| `omaphoenix` | Joint product: snapshot/restore, slow-download watchdog, hardware-aware restore, undo, **audit-before-backup** via `secaudit-core` | Sumiran Dahal |

Build both in **one workspace until the handover**, so parity work and the shared contract evolve together. Then move the two `secaudit*` crates to Manish's repo, publish `secaudit-core`, and have OmaPhoenix depend on the published version (RUST.md forbids git dependencies in releases). Keep the crate boundary clean from day one: `secaudit-core` must not depend on any OmaPhoenix crate.

Both apps follow [DESIGN.md](../DESIGN.md) and [RUST.md](../RUST.md). The standalone Security Audit keeps its own name and identity but shares the design language and keymap, so a person using both feels one family.

## Workspace

```
omaphoenix/                    (new repo, or a rewrite branch: decision in 07)
├─ Cargo.toml                  workspace
├─ crates/
│  ├─ phx-core/                snapshot/restore orchestration, no UI   ← internal/core
│  ├─ phx-category/            one module per category                 ← internal/category
│  ├─ phx-store/               git-backed snapshot store               ← internal/store
│  ├─ secaudit-core/           audit engine + contract (→ Manish)      ← secaudit bin/secaudit
│  ├─ secaudit/                standalone Security Audit app (→ Manish)
│  ├─ phx-contract/            Finding, Manifest, Selection types      ← the JSON contracts
│  ├─ phx-tui/                 ratatui front end                       ← internal/tui + secaudit-tui
│  └─ phx-cli/                 clap binary: `omaphoenix`, `omaphoenix audit`
├─ tests/fixtures/             JSON fixtures produced by the Go/Bash versions (parity)
└─ scripts/vm/                 same VM harness, pointed at the Rust binary
```

## Crate choices

| Need | Crate | Note |
|---|---|---|
| CLI | `clap` (derive) | Shell completion through `clap_complete` |
| TUI | `ratatui` + `crossterm` | Mature, widely used. **Learn from the Go trap:** never block on terminal queries at startup (OmaPhoenix had a 5 s stall on a bare Linux console). |
| JSON / TOML | `serde`, `serde_json`, `toml` | The contract types live in `phx-contract` and are shared |
| Errors | `thiserror` (libraries), `anyhow` (binary) | |
| Encryption | `age` (the rage project's library crate) | Same format as Go's `filippo.io/age`, so existing vaults stay readable. **Verify this with a cross-implementation test.** |
| Git | **Run the system `git`** (`std::process::Command`) | Same reasoning as Go: full auth, signing and bundle support for free. `gix` only if a need appears. |
| Parallelism | `rayon` for CPU work; plain threads for subprocesses | No async runtime needed: the work is filesystem and subprocess bound |
| Globs | `globset` | Must reproduce the Go selection semantics, with a test |
| systemd / D-Bus | `zbus` only if needed; otherwise run `systemctl` | |
| Tests | built-in, `insta` (snapshot tests), `assert_cmd` | |
| Release | `cargo-dist` or a hand-written GitHub Actions job | Static musl builds for amd64 and arm64, checksums, attestations |
| Lint | `clippy -D warnings`, `rustfmt`, `cargo-deny` (licenses, advisories) | `cargo-deny` matters: the product is a security tool |

## Audit engine port

The Bash checks are short and plentiful. Port them as data where possible:
- **Each check is a struct** implementing a `Check` trait with `id()`, `section()`, `needs_root()` and `run(&Ctx) -> Vec<Finding>`.
- **`Ctx`** abstracts the home dir, root status and command runner, so checks run against fixture trees in tests. This makes `--home DIR` and parity testing natural.
- **Keep shelling out** to `ss`, `ufw`, `systemctl`, `pacman`, `arch-audit` where they are the authority. Re-implementing them gains nothing.
- **The parity harness** runs the Bash engine and the Rust engine on the same VM and diffs (`id`, `sev`, `targets`).

## Compatibility promises

- Snapshot format v1 (made by Go OmaPhoenix) must restore with the Rust binary.
- Finding contract v1 stays readable. A v2 is allowed only with a `schema` bump.
- `selection.conf` semantics stay identical: last match wins, escaped ids, `*` is the whole category.

## Desktop app

There is **no Go/Wails app** (ADR-0005). The desktop app is part of the Rust rewrite, built to [DESIGN.md](../DESIGN.md). The Rust core crates are shared with the CLI and TUI, and the app is a front end only.

**Requirements DESIGN.md puts on the UI stack:**
- Interruptible springs that carry velocity.
- 120 fps transform/opacity animation.
- Shared-element transitions.
- Live re-theming from Omarchy's `colors.toml`.
- The user's font through fontconfig.
- **Screen-reader access over AT-SPI.**
- Native Wayland.
- Respect for reduced motion.

| Option | For | Against |
|---|---|---|
| **Tauri 2** (Rust core plus a web UI rendered by WebKitGTK) | The most mature motion and accessibility tooling; WebKitGTK exposes AT-SPI; fast iteration on fine visual polish; the Rust core is called directly through commands | The UI is in TS, not Rust; WebKitGTK performance must be checked against the 120 fps budget on Hyprland |
| **GPUI** (Zed's GPU UI framework) | Pure Rust, GPU-rendered, very fast, proven by Zed on Linux/Wayland | Young API with breaking changes; accessibility is immature; spring physics would be written by hand |
| iced / Slint / egui | Pure Rust | Motion and polish fall short of DESIGN.md §6 without heavy custom work |

**Proposal:** Tauri 2. Do a two-week spike first: build Pulse and one migration step to the DESIGN.md bar on Hyprland, measure frame times and run a screen-reader pass, then confirm Q9. If the spike fails the budget, fall back to GPUI.
