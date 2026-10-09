# 05: Roadmap

**Status values:** `todo` · `doing` · `done` · `blocked (needs: …)`.

To find the next task: take the first `todo` whose dependencies are all `done`. Every task has acceptance criteria; it is `done` only when they are demonstrated, not when the code compiles.

## Phase 0: groundwork (owners agree)

| ID | Task | Depends | Status | Acceptance |
|---|---|---|---|---|
| P0.1 | Agree on a license for secaudit and OmaPhoenix (they must be compatible for a joint rewrite) | — | blocked (needs: both owners) | LICENSE file in both repos |
| P0.2 | Agree on the finding contract v1 ([04](04-integration-design.md)) | — | blocked (needs: Manish) | Contract written into secaudit's README |
| P0.3 | Decide repo visibility for the joint product (OmaPhoenix is private today) | — | blocked (needs: Sumiran) | Recorded in [07](07-decisions.md) |
| P0.4 | OmaPhoenix: finish its own release prerequisites (fresh-VM restore test passing). The VM harness works (unattended install); the first full restore found and fixed 3 bugs (see OmaPhoenix docs/08) | — | doing | `scripts/vm` restore test green on a fresh Omarchy 4.0.4 VM |

## Phases 1–3 (Go integration): cancelled

Superseded by D11: the integration is built directly in Rust. The Go OmaPhoenix finishes on its own freeze gate (OmaPhoenix `docs/06-roadmap.md`, G1–G7) and freezes. The **finding contract** from [04](04-integration-design.md) carries over unchanged, now as Rust types in `secaudit-core`.

## Phase 4: Rust: shared audit engine, standalone Security Audit, joint OmaPhoenix

The design is in [06-rust-rewrite](06-rust-rewrite.md). It starts when the Go OmaPhoenix passes its freeze gate (ADR-0010) and Q1 (license) is settled.

**Products:**
- **`secaudit-core`:** the audit engine and finding contract, shared.
- **`secaudit`:** the standalone Security Audit app (CLI, TUI, desktop), handed to Manish (ADR-0013).
- **`omaphoenix`:** the joint product: snapshot/restore plus the audit-before-backup gate (ADR-0012).

| ID | Task | Depends | Status | Acceptance |
|---|---|---|---|---|
| P4.0 | Settle Q1 (license, Manish's written OK to rewrite his checks) | — | blocked (needs: both owners) | LICENSE in the new repo |
| P4.1 | Workspace skeleton, CI, release pipeline; the contract and manifest as Rust types with JSON compatibility tests against the Go fixtures | P4.0, Go freeze gate | todo | The Rust parser reads every fixture the Go version produced |
| P4.2 | `secaudit-core`: port the audit engine check by check (the Bash checks become Rust modules) with stable `id`s and `targets` (contract v1, docs/04), and a parity harness against the Bash engine | P4.1 | todo |
| P4.2a | `secaudit` standalone app: CLI and TUI with feature parity to today's secaudit, plus a desktop app per DESIGN.md; ready to hand over (README, release, ownership notes) | P4.2 | todo |
| P4.2b | Integrate `secaudit-core` into `omaphoenix`: the audit-before-backup gate, finding→item mapping, re-audit after restore (the design in docs/04, now in Rust) | P4.2, P4.3 | todo | Same `id`s and sev for every check on a reference VM |
| P4.3 | Port the OmaPhoenix categories one by one, verified by the existing `smoke.sh` and VM scenarios run against the Rust binary | P4.1 | todo | `smoke.sh` and VM scenario green against the Rust binary |
| P4.4 | Rust TUI (ratatui) with feature parity, using the unified keymap from DESIGN.md §7.1 | P4.2, P4.3 | todo | Same keys and screens; pty tests |
| P4.6 | GUI stack spike (Q9): Pulse and one migration step built to DESIGN.md | P4.1 | todo | 120 fps on Hyprland; interruptible springs; AT-SPI pass; live theme switch |
| P4.7 | Joint desktop app: all DESIGN.md §4 sections | P4.6, P4.2, P4.3 | todo | DESIGN.md §11 checklist passes for every screen |
| P4.5 | Cutover: the Go OmaPhoenix and Bash secaudit are frozen; a migration note for users | P4.4, P4.7 | todo |
| P4.8 | **Handover:** move `secaudit-core` and `secaudit` into Manish's repo, publish `secaudit-core`, switch `omaphoenix` to the published crate | P4.2a, P4.5 | todo | Old snapshots restore with the Rust binary (format v1 compatibility) |
