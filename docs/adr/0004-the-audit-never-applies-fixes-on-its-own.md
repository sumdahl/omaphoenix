# ADR-0004: The audit never applies fixes on its own

- **Status:** Accepted
- **Date:** 2026-10-09
- **Decided by:** design (matches secaudit)

## Context
Fixes change the system; secaudit already requires confirmation.

## Decision
Integration code only excludes or blocks backup items. Any fix runs after explicit human confirmation.

## Consequences
No silent system changes from a timer run; fix flows live in the UIs (DESIGN.md §7.3).
