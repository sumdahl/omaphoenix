# ADR-0025: The terminal app first; the desktop apps are frozen

- **Status:** Proposed (needs Manish's OK for step 2)
- **Date:** 2026-10-28
- **Decided by:** Sumiran Dahal

## Context
OmaPhoenix ships five front ends: the CLI, `omaphoenix tui`, `audit tui`, `audit menu`, and two Slint desktop apps. Omarchy users live in the terminal, every feature is built twice, and the desktop apps carry their own workspaces, license exception (ADR-0020) and the slowest CI job.

## Decision
The terminal app is the product; every user-facing feature must be reachable in it (docs/15).

1. **0.1.x:** the desktop apps are frozen: they keep building, get no features, and the installer stops offering them by default.
2. **0.2:** one terminal app. `audit tui` folds into `omaphoenix tui` as its Audit section, with full and deep scans, history, export and the AI explanations; `audit menu` is dropped. This ends the rule that the audit TUI matches Manish's Textual app screen for screen, so it needs his agreement.
3. **0.3:** the desktop apps are removed, with a note in docs/13.

## Consequences
- One place to build each feature; docs/15's gaps (the restore picker first) become the TUI roadmap.
- Users who prefer a window lose the desktop app in 0.3; the terminal app opens from the app launcher as before.
- Supersedes the desktop parts of ADR-0019 and ADR-0020 once step 3 lands.
