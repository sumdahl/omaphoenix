# ADR-0009: RUST.md engineering standards

- **Status:** Accepted
- **Date:** 2026-10-09
- **Decided by:** Sumiran

## Context
The rewrite should be safe, idiomatic, modern Rust at a production-company standard.

## Decision
All Rust follows RUST.md: forbid unsafe, no unwrap/panic in libraries, pedantic clippy, thiserror/anyhow, tracing, secrecy, property/snapshot tests, fuzzing, cargo-deny.

## Consequences
The CI gate in RUST.md §1 is required for every merge.
