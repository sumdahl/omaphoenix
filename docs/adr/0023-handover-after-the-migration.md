# ADR-0023: Handover after the migration is finished

- **Status:** Accepted
- **Date:** 2026-10-10
- **Decided by:** Sumiran Dahal ("I will invite him later after finishing migration"). Answers Q7 (#16).

## Context
ADR-0013 hands the Security Audit crates to Manish; docs/12 describes how.

## Decision
The handover (P4.8, #15) happens after the cutover (P4.5, #14) is done. Until then both products live in this repo. The owner invites Manish and they agree on publishing `secaudit-core` at that point, following docs/12.

## Consequences
No crates.io publishing or repo split before then; nothing is pushed to `mnshchtri/secaudit` meanwhile.
