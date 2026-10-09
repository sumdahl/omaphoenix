# ADR-0005: No Go/Wails desktop app

- **Status:** Accepted
- **Date:** 2026-10-09
- **Decided by:** Sumiran

## Context
A Go GUI would be thrown away by the Rust rewrite.

## Decision
The Go version ships CLI + TUI only. The only GUI is the joint Rust desktop app.

## Consequences
Wails is removed from the OmaPhoenix plan (its docs/03, docs/06).
