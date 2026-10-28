# ADR-0015: Manish agrees to the rewrite; dual MIT OR Apache-2.0

- **Status:** Accepted; the license part is superseded by ADR-0026 (MIT only)
- **Date:** 2026-10-09
- **Decided by:** Sumiran Dahal and Manish Karki (agreement given to Sumiran in conversation)

## Context
Porting Security Audit's checks into Rust (`secaudit-core`) rewrites Manish's work, and its Bash repo had no LICENSE file. Q1 (issue #1) blocked the port until both the permission and a compatible license were settled.

## Decision
- Manish agrees to the rewrite of Security Audit's checks into `secaudit-core` and the standalone `secaudit` app, under the plan in ADR-0013 (handed over to him when finished).
- All crates in this workspace are licensed **MIT OR Apache-2.0**, the Rust-ecosystem standard. That covers `secaudit-core`, `secaudit`, `omaphoenix` and `omaphoenix-desktop`.

## Consequences
- **Unblocks** the engine port (P4.2, issue #5).
- **License files:** `LICENSE-MIT` and `LICENSE-APACHE` are added, and `license = "MIT OR Apache-2.0"` is set in the workspace. Crates stay `publish = false` until the handover.
- **Dependencies:** `cargo-deny` licence policy stays compatible (permissive, plus MPL-2.0 for unmodified dependencies).
