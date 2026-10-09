# ADR-0010: Finish, then freeze the Go OmaPhoenix

- **Status:** Accepted
- **Date:** 2026-10-09
- **Decided by:** Sumiran

## Context
The Go version is where the hard facts are found; a moving target makes porting harder.

## Decision
The Go OmaPhoenix is finished against its freeze gate (G1–G7 in its docs/06-roadmap.md), then frozen to bug fixes only. The Rust work starts after that.

## Consequences
Resolves the old 'Rust trigger' question (Q8).
