# ADR-0027: Publish on crates.io as omaphoenix-*

- **Status:** Accepted
- **Date:** 2026-11-02
- **Decided by:** Sumiran

## Context
OmaPhoenix is a Rust tool, and Rust users expect `cargo install`. The workspace was `publish = false` until the handover (ADR-0013), which ADR-0024 replaced. `cargo install omaphoenix` needs every library it uses on crates.io too. The name `phx` already belongs to an unrelated crate, so `phx-*` would look like part of it.

## Decision
Publish `omaphoenix` and its libraries as `omaphoenix-core`, `omaphoenix-contract`, `omaphoenix-audit` and `omaphoenix-audit-ui`. The code keeps the short names (`phx_core` and so on) through `[lib] name` and `package =`, so nothing in the source changes. `omaphoenix-desktop` (a spike) and `apps/` stay unpublished. All crates share the workspace version and are published together after each release tag.

## Consequences
`cargo install omaphoenix` builds the command from source. The installer stays the main way in, because it also verifies checksums and adds launchers and skills. A published version can be yanked but never deleted. The crate names are claimed for good.
