# ADR-0001: Audit before every backup

- **Status:** Accepted
- **Date:** 2026-10-09
- **Decided by:** Sumiran + Manish

## Context
A long-used machine accumulates secrets in config, persistence/backdoors and weak settings. A naive backup pushes secrets to GitHub and re-installs compromise on the fresh machine. OmaPhoenix filters some of this but isn't a security tool; Security Audit is (docs/01).

## Decision
Integrate Security Audit into OmaPhoenix: run the audit before a snapshot, exclude or block flagged items, re-audit after restore.

## Consequences
Snapshots carry an audit summary; the gate needs findings with stable ids and targets (contract v1, docs/04).
