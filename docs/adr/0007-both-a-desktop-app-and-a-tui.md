# ADR-0007: Both a desktop app and a TUI

- **Status:** Accepted
- **Date:** 2026-10-09
- **Decided by:** Sumiran

## Context
Restores happen on fresh installs, over SSH and on bare consoles where there is no desktop yet.

## Decision
The product ships a desktop app and a TUI as equal, first-class front ends over one Rust core.

## Consequences
DESIGN.md §12 maps every rule to the terminal; the TUI must start < 100 ms and never block on terminal queries.
