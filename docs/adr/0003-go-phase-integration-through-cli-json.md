# ADR-0003: Go-phase integration through CLI + JSON

- **Status:** Superseded by ADR-0011
- **Date:** 2026-10-09
- **Decided by:** design

## Context
Two owners, two repos; neither should import the other's code.

## Decision
OmaPhoenix calls `secaudit --json` and parses its output.

## Consequences
Superseded with ADR-0002. The JSON contract survives as Rust types in `secaudit-core`.
