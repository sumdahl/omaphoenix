# ADR-0021: The gate excludes failures and warns on warnings

- **Status:** Accepted
- **Date:** 2026-10-10
- **Decided by:** Sumiran Dahal ("do what is the most recommended"). Answers Q5 (#8).

## Context
The audit gate (ADR-0001) runs before every snapshot. A blocking gate stops unattended timer snapshots on any failure, so a machine can go days without a backup; a silent gate lets secrets through.

## Decision
Default policy: items tied to a failing finding are **excluded** from the snapshot; warnings are **reported** and the item is kept. The snapshot still happens, the manifest records what was left out and why, and `doctor` surfaces it. This is what `phx-core::gate` already does.

## Consequences
Backups keep running; nothing a failing check flags reaches GitHub. A stricter `block` mode can be added later as an option without changing the default.
