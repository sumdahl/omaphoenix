# ADR-0018: One engine crate, modules per Go package

- **Status:** Accepted
- **Date:** 2026-10-10
- **Decided by:** Sumiran Dahal (agent proposal while porting, overnight)

## Context
docs/06 planned five OmaPhoenix crates (`phx-core`, `phx-category`, `phx-store`, `phx-tui`, `phx-cli`). While porting, every one of them depended on the others' types, so splitting them would mean many public APIs that exist only to cross crate lines.

## Decision
- **`phx-core`** holds the engine as modules that mirror the Go packages: `sys`, `store`, `category/*`, `app`, `doctor`, `undo`, `schedule`, plus `gate` (the audit gate).
- **`phx-contract`** stays separate: the on-disk formats, shared by tools that only read snapshots.
- **`omaphoenix`** is the one binary: CLI and TUI.
- `secaudit-core` and `secaudit` stay apart from all of these, for the handover (ADR-0013). `phx-core` depends on `secaudit-core`, never the reverse.

## Consequences
- Fewer crates, fewer public APIs. A module can become a crate later if a real second user appears.
- Compile times are fine at this size.
