# ADR-0022: The repo stays private

- **Status:** Superseded by ADR-0024
- **Date:** 2026-10-10
- **Decided by:** Sumiran Dahal ("make it private, I will later invite Manish"). Answers Q2 (#17).

## Context
ADR-0014 created `sumdahl/omaphoenix-secaudit` as private pending this question.

## Decision
The repo stays private under `sumdahl`. Manish is invited as a collaborator later, by the owner.

## Consequences
- GitHub build attestations stay off (user-owned private repos can't have them); `SHA256SUMS` still guards every install.
- `install.sh` downloads through `gh` for anyone with access.
