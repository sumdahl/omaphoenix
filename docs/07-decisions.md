# 07: Decisions and open questions

## Decided

| # | Decision | By | Date |
|---|---|---|---|
| D1 | Integrate secaudit into OmaPhoenix: audit before backup, so sensitive data never goes to the internet and compromise isn't copied to a fresh machine | Sumiran + Manish | 2026-10-09 |
| D2 | Do the integration in the current languages first (Go + Bash), then rewrite both in Rust | Sumiran + Manish | 2026-10-09 |
| D3 | The Go phase integrates through the CLI and JSON contract, not code imports | design, pending owners' review | 2026-10-09 |
| D4 | The audit never auto-applies fixes; it only excludes or blocks backup items. Fixes need human confirmation | design, matches secaudit today | 2026-10-09 |
| D5 | **No Go/Wails desktop app.** The only GUI will be the joint Rust desktop app. The Go phase ships CLI + TUI only | Sumiran | 2026-10-09 |
| D6 | `DESIGN.md` is the single source of truth for UI/UX; changes need both owners | Sumiran | 2026-10-09 |
| D8 | **Slow or stalled downloads never block a restore.** Every external install runs under the generic watchdog (docs/08) and is deferred, not failed; rerunning restore finishes it | Sumiran | 2026-10-09 |
| D9 | The Rust codebase follows **RUST.md**: safe (`forbid(unsafe_code)`), idiomatic, modern Rust at production-company standard | Sumiran | 2026-10-09 |
| D11 | **The integration is built directly in Rust**, not in Go: the Go OmaPhoenix freezes without it. **Supersedes D2/D3** (Go-first integration) | Sumiran | 2026-10-09 |
| D12 | **The joint product is named OmaPhoenix** (resolves Q3) | Sumiran | 2026-10-09 |
| D13 | **Security Audit is also rewritten as a standalone Rust app**, built on the same audit engine crate as OmaPhoenix, and **handed over to Manish Karki** when finished. Two products, one shared engine | Sumiran | 2026-10-09 |
| D10 | **The Go OmaPhoenix is finished, then frozen** once it passes the freeze gate (OmaPhoenix `docs/06-roadmap.md`, G1–G7). Only then does the joint Rust project start, per this plan. Go gets bug fixes only after that | Sumiran | 2026-10-09 |
| D7 | The joint product ships **both a desktop app and a TUI**, as equal, first-class front ends over one shared Rust core. The TUI is required: it is the only interface over SSH, on a bare console, and during a fresh-install restore before the desktop is back | Sumiran | 2026-10-09 |

## Open: needs the owners

Agents must not decide these. Ask the user.

| # | Question | Who | Notes |
|---|---|---|---|
| Q1 | **License** for both projects. A shared engine and a rewrite of Manish's checks need his explicit agreement and compatible licenses. **Blocks the start of the Rust work** | both | secaudit has no LICENSE; OmaPhoenix docs planned MIT. Proposal: MIT or Apache-2.0 for all three crates |
| Q2 | **Repo and visibility** for the joint product: a new org? a new repo? public or private? | both | OmaPhoenix is private, secaudit is public |
| ~~Q3~~ | ~~Product name~~: **resolved by D12: OmaPhoenix** | — | |
| Q4 | **Finding contract v1** fields and the ID naming scheme ([04](04-integration-design.md)) | Manish | Proposal ready |
| Q5 | **Default gate policy:** `exclude` failures and `warn` on warnings, or `block` on any failure? | both | Proposal: exclude/warn, so a timer run never silently stops backing up |
| Q6 | Should secaudit's Python TUI stay as a separate app in the Go phase, or be replaced by the OmaPhoenix TUI Audit tab? | both | Proposal: keep it, and hand off to it for fixes until the Rust TUI replaces both |
| Q7 | **Ownership and repos for the shared engine** after the handover: Manish owns `secaudit-core` and the standalone app; OmaPhoenix depends on a published version (crates.io) | both | Proposal: build everything in one workspace until the handover, then move `secaudit-core` and `secaudit` into Manish's repo and publish the core crate |
| Q8 | **Rust rewrite trigger:** after the first joint release (P3.3), or earlier? | both | Proposal: after P3.3 |
| Q9 | **Rust GUI stack** for the desktop app (see [06](06-rust-rewrite.md#desktop-app)) | both | Proposal: Tauri 2 (Rust core, web UI). Alternative: GPUI (pure Rust). Must satisfy DESIGN.md §6 and §9 |
