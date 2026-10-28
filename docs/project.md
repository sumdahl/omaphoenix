# OmaPhoenix: project notes

How this project came to be and where its decisions live: the integration of OmaPhoenix and Manish Karki's Security Audit, the Rust port, and the design documents. For using OmaPhoenix, see the [README](../README.md).

> **Status: `v0.1.0`.** Both products are ported to Rust, checked against the Go and Bash versions on a real machine, and tested by full restores on a fresh Omarchy 4.0.6 VM (docs/14). Security Audit now ships inside OmaPhoenix as `omaphoenix audit`. The Go OmaPhoenix is frozen at `v0.3.0-rc.1` (`sumdahl/omaphoenix-go`).
>
> **Decided:**
> - The integration is built **directly in Rust** (ADR-0011).
> - The joint product is **OmaPhoenix** (ADR-0012).
> - ~~Security Audit gets a standalone Rust app, handed to Manish~~ (ADR-0013): superseded by the fold.
> - **Security Audit is folded into OmaPhoenix** as `omaphoenix audit` (0.1.0; Manish agreed). The `secaudit` commands are gone; `install.sh` removes them.
>
> Owners: **Sumiran Dahal** (OmaPhoenix) and **Manish Karki** (Security Audit). Both agreed to integrate.

**Security Audit by [Manish Karki](https://github.com/mnshchtri).** The audit engine, its 13 sections, the terminal app and the menus are his design, ported to Rust from [mnshchtri/secaudit](https://github.com/mnshchtri/secaudit).

## The two projects

| | OmaPhoenix | Security Audit (`secaudit`) |
|---|---|---|
| Owner | Sumiran Dahal | Manish Karki |
| Repo | `github.com/sumdahl/omaphoenix-go` (**private**, the Go version) | `github.com/mnshchtri/secaudit` (**public, no LICENSE file yet**) |
| Local path | `~/personal/omaphoenix` | `~/personal/secaudit` |
| Language today | Go (single static binary) | Bash engine + Python Textual TUI + gum UI |
| What it does | Timestamped snapshots of packages, plugins, themes, config, `/etc`, services, encrypted SSH keys, stored in the user's private git repo; one-command restore | Read-only audit of ~14 areas (packages, boot, sysctl, firewall, network, SSH, users, lock, filesystem, persistence, containers, secrets, rootkit); score + grade; fix commands run only on confirmation |
| Machine-readable output | `omaphoenix doctor --json`, snapshot manifests | `secaudit --json FILE` (one JSON object per finding) |

## Documents (read in order)

| # | Doc | For |
|---|---|---|
| 1 | [01-problem-and-vision.md](01-problem-and-vision.md) | Why integrate; what goes wrong without it |
| 2 | [02-secaudit-today.md](02-secaudit-today.md) | Exact facts about secaudit as of 2026-10-09 (checks, JSON, exit codes, state) |
| 3 | [03-omaphoenix-today.md](03-omaphoenix-today.md) | Exact facts about OmaPhoenix as of 2026-10-09 |
| 4 | [04-integration-design.md](04-integration-design.md) | The audit gate, the finding contract, the mapping from findings to backup items |
| 5 | [05-roadmap.md](05-roadmap.md) | Phased plan with task IDs and acceptance criteria |
| 6 | [06-rust-rewrite.md](06-rust-rewrite.md) | Target architecture and crate choices for the joint Rust rewrite |
| 7 | [adr/](adr/README.md) · [07-decisions.md](07-decisions.md) | Decisions as ADRs (one file each), and **open questions that need both owners** |
| 8 | [08-slow-download-watchdog.md](08-slow-download-watchdog.md) | Generic rule: no slow or stalled download may delay or fail a restore (spec + required tests) |
| 9 | [09-gui-spike-findings.md](09-gui-spike-findings.md) | GUI spikes (Tauri, Slint, native floors): measured frames, theming, accessibility, memory; recommendation for Q9 |
| 10 | [10-secaudit-port-notes.md](10-secaudit-port-notes.md) | Security Audit port: parity method, ids per section, Bash quirks S1–S7 (proposals for Manish) |
| 11 | [11-omaphoenix-port-notes.md](11-omaphoenix-port-notes.md) | OmaPhoenix port: parity method and results, deliberate differences from Go |
| 12 | [12-secaudit-handover.md](12-secaudit-handover.md) | Handing Security Audit to Manish: what moves, how, what's left |
| 13 | [13-migration.md](13-migration.md) | Moving from the Go and Bash/Python versions: what carries over, what changes, going back |
| 14 | [14-vm-restore-test.md](14-vm-restore-test.md) | Full restore on a fresh Omarchy VM: the plan to build the test and the scenarios that prove it (T1–T12) |
| — | [findings/](findings/README.md) | **Bugs and testing findings**, one file each: how to reproduce, cause, workaround, fix and its test |
| — | [RUST.md](../RUST.md) | **Engineering standards for the Rust codebase:** safe, idiomatic, modern; lints, errors, testing, deps, release |
| — | [DESIGN.md](../DESIGN.md) | **Single source of truth for UI/UX:** feel, voice, theme tokens from Omarchy, spring physics, keymap, consent patterns, definition of done |
| — | [AGENTS.md](../AGENTS.md) | Rules for AI agents working on this project |

## Workspace layout

```
~/personal/joint-omaphoenix-secaudit/     the joint workspace (home of this project)
├── omaphoenix-secaudit/                  this design folder: README, AGENTS, DESIGN, RUST, docs/
└── secaudit/                             clone of github.com/mnshchtri/secaudit (Manish's repo)
~/personal/omaphoenix/                    the Go OmaPhoenix (private repo sumdahl/omaphoenix-go)
~/personal/project-ideas/omaphoenix-secaudit → symlink to the design folder (old path)
```

The Rust workspace for the joint OmaPhoenix and the standalone Security Audit (docs/06) is this repo (`crates/`, `apps/`).

## Where work happens

- **Phases 1–2** (integration in Go): code goes into `~/personal/omaphoenix`. secaudit gets small contract changes, made in `~/personal/secaudit` by or with Manish.
- **Phase 4** (Rust rewrite): this repo. Built 2026-10-10.
- **Decisions:** keep [07](07-decisions.md) and the ADRs current when one is made.
