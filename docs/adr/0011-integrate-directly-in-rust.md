# ADR-0011: Integrate directly in Rust

- **Status:** Accepted
- **Date:** 2026-10-09
- **Decided by:** Sumiran

## Context
Building the integration in Go and again in Rust doubles the work.

## Decision
The integration is built directly in Rust. The Go OmaPhoenix freezes without it. Supersedes ADR-0002 and ADR-0003.

## Consequences
Go roadmap phases P1–P3 are cancelled; the audit gate arrives with the Rust version. The Python-TUI question (Q6) is moot.
