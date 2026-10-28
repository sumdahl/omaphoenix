# ADR-0013: A standalone Security Audit, handed to Manish

- **Status:** Superseded by ADR-0024
- **Date:** 2026-10-09
- **Decided by:** Sumiran

## Context
OmaPhoenix absorbs the audit, but Security Audit should keep living as its own product.

## Decision
Security Audit is rewritten as a standalone Rust app on the shared `secaudit-core` engine and handed over to Manish Karki when finished. One workspace until the handover; then `secaudit-core` and `secaudit` move to his repo and the core is published.

## Consequences
`secaudit-core` must never depend on OmaPhoenix crates. License agreement (Q1) is required before porting his checks.
