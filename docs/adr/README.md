# Architecture decision records

One file per decision. Records are never edited after acceptance, only **superseded** by a newer record. New decision: copy `template.md`, take the next number, and link it here.

| ADR | Decision | Status |
|---|---|---|
| [0001](0001-audit-before-every-backup.md) | Audit before every backup | Accepted |
| [0002](0002-integrate-in-go-and-bash-first.md) | Integrate in Go and Bash first | Superseded by ADR-0011 |
| [0003](0003-go-phase-integration-through-cli-json.md) | Go-phase integration through CLI + JSON | Superseded by ADR-0011 |
| [0004](0004-the-audit-never-applies-fixes-on-its-own.md) | The audit never applies fixes on its own | Accepted |
| [0005](0005-no-go-wails-desktop-app.md) | No Go/Wails desktop app | Accepted |
| [0006](0006-design-md-is-the-ui-ux-source-of-truth.md) | DESIGN.md is the UI/UX source of truth | Accepted |
| [0007](0007-both-a-desktop-app-and-a-tui.md) | Both a desktop app and a TUI | Accepted |
| [0008](0008-slow-downloads-never-block-a-restore.md) | Slow downloads never block a restore | Accepted |
| [0009](0009-rust-md-engineering-standards.md) | RUST.md engineering standards | Accepted |
| [0010](0010-finish-then-freeze-the-go-omaphoenix.md) | Finish, then freeze the Go OmaPhoenix | Accepted |
| [0011](0011-integrate-directly-in-rust.md) | Integrate directly in Rust | Accepted |
| [0012](0012-the-joint-product-is-named-omaphoenix.md) | The joint product is named OmaPhoenix | Accepted |
| [0013](0013-a-standalone-security-audit-handed-to-manish.md) | A standalone Security Audit, handed to Manish | Superseded by 0024 |
| [0014](0014-a-private-monorepo-for-the-joint-work.md) | A private monorepo for the joint work | Accepted |
| [0015](0015-manish-agrees-license-mit-or-apache.md) | Manish agrees to the rewrite; dual MIT OR Apache-2.0 | Accepted; license superseded by 0026 |
| [0016](0016-finding-contract-v1-adopted.md) | Finding contract v1 adopted as proposed | Accepted |
| [0017](0017-memory-budgets-are-binding.md) | Memory budgets are binding | Accepted |
| [0018](0018-one-engine-crate.md) | One engine crate, modules per Go package | Accepted |
| [0019](0019-the-desktop-stack-is-slint.md) | The desktop stack is Slint (memory-gated) | Accepted |
| [0020](0020-slint-under-its-royalty-free-license.md) | Slint under its Royalty-free license | Accepted |
| [0021](0021-gate-excludes-failures-and-warns-on-warnings.md) | The gate excludes failures and warns on warnings | Accepted |
| [0022](0022-the-repo-stays-private.md) | The repo stays private | Superseded by 0024 |
| [0023](0023-handover-after-the-migration.md) | Handover after the migration is finished | Superseded by 0024 |
| [0024](0024-security-audit-folds-in-and-the-repo-goes-public.md) | Security Audit folds into OmaPhoenix, and the repo goes public | Accepted |
| [0025](0025-terminal-first.md) | The terminal app first; the desktop apps are frozen | Proposed |
| [0026](0026-mit-only.md) | MIT only | Accepted |
