# 05: Roadmap

**Status values:** `todo` · `doing` · `done` · `blocked (needs: …)`.

To find the next task: take the first `todo` whose dependencies are all `done`. Every task has acceptance criteria; it is `done` only when they are demonstrated, not when the code compiles.

## Phase 0: groundwork (owners agree)

| ID | Task | Depends | Status | Acceptance |
|---|---|---|---|---|
| P0.1 | Agree on a license for secaudit and OmaPhoenix (they must be compatible for a joint rewrite) | — | blocked (needs: both owners) | LICENSE file in both repos |
| P0.2 | Agree on the finding contract v1 ([04](04-integration-design.md)) | — | blocked (needs: Manish) | Contract written into secaudit's README |
| P0.3 | Decide repo visibility for the joint product (OmaPhoenix is private today) | — | blocked (needs: Sumiran) | Recorded in [07](07-decisions.md) |
| P0.4 | OmaPhoenix: finish its own release prerequisites (fresh-VM restore test passing). The VM harness works (unattended install); the first full restore found and fixed 3 bugs (see OmaPhoenix docs/08) | — | doing | `scripts/vm` restore test green on a fresh Omarchy 4.0.4 VM Plan: [docs/14](14-vm-restore-test.md). |

## Phases 1–3 (Go integration): cancelled

Superseded by ADR-0011: the integration is built directly in Rust. The Go OmaPhoenix finishes on its own freeze gate (OmaPhoenix `docs/06-roadmap.md`, G1–G7) and freezes. The **finding contract** from [04](04-integration-design.md) carries over unchanged, now as Rust types in `phx-audit`.

**Tracking:** every task below is a GitHub issue in `sumdahl/omaphoenix` (milestones M1–M5); open questions are issues labelled *decision needed*.

## Phase 4: Rust: shared audit engine, standalone Security Audit, joint OmaPhoenix

The design is in [06-rust-rewrite](06-rust-rewrite.md). It starts when the Go OmaPhoenix passes its freeze gate (ADR-0010) and Q1 (license) is settled.

**Products:**
- **`phx-audit`:** the audit engine and finding contract, shared.
- **`secaudit`:** the standalone Security Audit app (CLI, TUI, desktop), handed to Manish (ADR-0013).
- **`omaphoenix`:** the joint product: snapshot/restore plus the audit-before-backup gate (ADR-0012).

| ID | Task | Depends | Status | Acceptance |
|---|---|---|---|---|
| P4.0 ([#1](https://github.com/sumdahl/omaphoenix/issues/1)) | Settle Q1 (license, Manish's OK to rewrite his checks) | — | **done** (ADR-0015) | LICENSE in the new repo |
| P4.1a ([#2](https://github.com/sumdahl/omaphoenix/issues/2)) | Workspace skeleton: Cargo workspace with `phx-audit`, `secaudit`, `omaphoenix` crates (empty but building), RUST.md lints, toolchain pin, `deny.toml`, CI gate | — (no ported code) | **done** | `cargo fmt/clippy -D warnings/test/deny` green locally and in CI |
| P4.1b ([#3](https://github.com/sumdahl/omaphoenix/issues/3)) | Contract and manifest as Rust types (finding contract v1, snapshot manifest format 1, selection rules) with JSON compatibility tests against Go fixtures | P4.1a, Q4 | **done** (c0617e4, c1ff5db) | The Rust types read every fixture the Go version produced |
| P4.2 ([#5](https://github.com/sumdahl/omaphoenix/issues/5)) | `phx-audit`: port the audit engine check by check (the Bash checks become Rust modules) with stable `id`s and `targets` (contract v1, docs/04), and a parity harness against the Bash engine | P4.0, P4.1b | **done**: 13 sections, 82/82 parity (docs/10) |
| P4.2a ([#6](https://github.com/sumdahl/omaphoenix/issues/6)) | `secaudit` standalone app: CLI and TUI with feature parity to today's secaudit, plus a desktop app per DESIGN.md; ready to hand over (README, release, ownership notes) | P4.2 | **done**: CLI, exact ports `secaudit-tui`/`secaudit-ui`, desktop `apps/omaphoenix-audit-gui` (59 MB idle); handover notes (docs/12, crates/secaudit/README.md) |
| P4.2b ([#7](https://github.com/sumdahl/omaphoenix/issues/7)) | Integrate `phx-audit` into `omaphoenix`: the audit-before-backup gate, finding→item mapping, re-audit after restore (the design in docs/04, now in Rust) | P4.2, P4.3 | **done** (gate, mapping, manifest summary, `omaphoenix reaudit`) | Same `id`s and sev for every check on a reference VM |
| P4.3 ([#9](https://github.com/sumdahl/omaphoenix/issues/9)) | Port the OmaPhoenix categories one by one, verified by the existing `smoke.sh` and VM scenarios run against the Rust binary | P4.1b, Go freeze gate | **done**: `scripts/omaphoenix-smoke` PASS, real-home parity (docs/11); VM scenario not run | `smoke.sh` and VM scenario green against the Rust binary Plan: [docs/14](14-vm-restore-test.md). |
| P4.4 ([#10](https://github.com/sumdahl/omaphoenix/issues/10)) | Rust TUI (ratatui) with feature parity, using the unified keymap from DESIGN.md §7.1 | P4.2, P4.3 | **done** (both TUIs; screen tests at 80×24; pty tests under script(1): first frame < 100 ms, no terminal queries, clean exit; DESIGN.md §7.1 keymap incl. palette, `?`, `/`, `u`, `R`) | Same keys and screens; pty tests |
| P4.6 ([#12](https://github.com/sumdahl/omaphoenix/issues/12)) | GUI stack spike (Q9): Pulse and one migration step built to DESIGN.md. **Tauri done; findings in docs/09.** GPUI comparison proposed | P4.1a | **done** (Tauri, Slint, native floors; docs/09) | 120 fps on Hyprland; interruptible springs; AT-SPI pass; live theme switch |
| P4.7 ([#13](https://github.com/sumdahl/omaphoenix/issues/13)) | Joint desktop app: all DESIGN.md §4 sections | P4.6, P4.2, P4.3 | **done** in `apps/omaphoenix-gui` (Slint, ADR-0019/0020; 62 MB idle); every section checked on screen at 790 px and 1896 px wide, layout fixes made | DESIGN.md §11 checklist passes for every screen |
| P4.5 ([#14](https://github.com/sumdahl/omaphoenix/issues/14)) | Cutover: the Go OmaPhoenix and Bash secaudit are frozen; a migration note for users | P4.4, P4.7 | **done**: docs/13-migration.md; Go README marks the freeze; `go_snapshot.rs` restores a Go-made snapshot in CI. Bash secaudit stays Manish's call (ADR-0023) |
| P4.8 ([#15](https://github.com/sumdahl/omaphoenix/issues/15)) | **Handover:** move `phx-audit` and `secaudit` into Manish's repo, publish `phx-audit`, switch `omaphoenix` to the published crate | P4.2a, P4.5 | todo | Old snapshots restore with the Rust binary (format v1 compatibility) |
