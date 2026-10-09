# ADR-0017: Memory budgets are binding

- **Status:** Accepted
- **Date:** 2026-10-09
- **Decided by:** Sumiran Dahal

## Context
The owner requires the product to be very memory-efficient. Measured on the reference machine:
- the Go OmaPhoenix peaks at 126 MB (`snapshot`) and 142 MB (`restore --dry-run`), mostly from whole-file reads and loading pacman's full file list
- the Tauri desktop spike idles at ≈ 400 MB across its WebKit processes

## Decision
The budgets in RUST.md §9 (CLI/TUI) and DESIGN.md §6.5 (desktop) are **pass/fail requirements**, enforced by a peak-RSS check in CI. The headline numbers:
- `snapshot` / `restore --dry-run` ≤ 48 MB
- TUI ≤ 24 MB
- desktop ≤ 150 MB target, 250 MB ceiling

A GUI stack that can't meet the desktop ceiling doesn't ship.

## Consequences
- **Streaming I/O and lazy iteration** are mandatory on the large paths (RUST.md §9 rules).
- **The GUI decision (Q9) gains a hard criterion.** Tauri as measured fails it, so a GPUI comparison spike, plus a Tauri memory-tuning attempt, is required before the decision.

## Correction (2026-10-10)
The Go figures in Context were wrong about the cause. 126 MB and 142 MB were the peak RSS of **child processes** (`npm ls` alone peaks at ~126 MB), counted because `wait4` includes waited-for children. The Go process's own peak was never measured. The Tauri figure was later corrected to 234 MB (docs/09). The decision stands; the Rust binaries measure their own `VmHWM` (RUST.md §9) and meet every budget.
