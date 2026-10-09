# ADR-0014: A private monorepo for the joint work

- **Status:** Accepted
- **Date:** 2026-10-09
- **Decided by:** Sumiran

## Context
Docs had no history, and code and decisions should change together.

## Decision
`github.com/sumdahl/omaphoenix-secaudit` (private) holds the design now and becomes the Rust Cargo workspace for `secaudit-core`, `secaudit` and `omaphoenix`. Manish's Bash repo stays a separate, read-only reference clone.

## Consequences
Partly resolves Q2: public visibility and an org are still open. At cutover the repo may be renamed to become the main OmaPhoenix repo.
