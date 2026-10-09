# ADR-0016: Finding contract v1 adopted as proposed

- **Status:** Accepted
- **Date:** 2026-10-09
- **Decided by:** Sumiran Dahal (with Manish's agreement to the rewrite, ADR-0015)

## Context
OmaPhoenix's audit gate needs findings with a stable identity and the exact items they concern (Q4, issue #4). The proposal is in docs/04: keep secaudit's existing fields and add `schema`, `id` and `targets`.

## Decision
- Adopt contract v1 as proposed: `{schema, id, sev, section, msg, detail, fix, hint, targets[]}`.
  - `id` is stable, `section.check`, lowercase.
  - `targets[]` entries are `{kind: file|unit|package|none, path|name}`.
- Readers also accept **legacy lines** from today's Bash secaudit (no `schema`/`id`/`targets`) as schema 0.
- Readers refuse any `schema` newer than they know.

## Consequences
- **Unblocks** the contract types (P4.1b, issue #3).
- **Revisions:** Manish may revise ids and targets before the handover, via a new ADR, with a `schema` bump if fields change.
