# OmaPhoenix × Security Audit: integration project

> **Status: design.** No Rust code yet. The Go OmaPhoenix is being finished and frozen (its freeze gate G1–G7); then the Rust work starts.
>
> **Decided:**
> - The integration is built **directly in Rust** (ADR-0011).
> - The joint product is **OmaPhoenix** (ADR-0012).
> - Security Audit also gets a **standalone Rust app on the shared engine, handed to Manish** (ADR-0013).
> Owners: **Sumiran Dahal** (OmaPhoenix) and **Manish Karki** (Security Audit). Both agreed to integrate.

## One paragraph

[OmaPhoenix](#the-two-projects) snapshots an Omarchy machine so a fresh install can be restored to exactly that state. [Security Audit](#the-two-projects) audits an Arch/Omarchy machine and explains each risk. Combined, a migration becomes **audit → clean → back up → restore → re-audit**. Nothing sensitive leaves the machine for GitHub, and nothing compromised (a backdoor, a leaked token, a malicious hook) gets copied onto the fresh install. The end state is both tools rewritten as one Rust codebase, done only after the Go version of OmaPhoenix is complete and proven.

## The two projects

| | OmaPhoenix | Security Audit (`secaudit`) |
|---|---|---|
| Owner | Sumiran Dahal | Manish Karki |
| Repo | `github.com/sumdahl/omaphoenix` (**private**) | `github.com/mnshchtri/secaudit` (**public, no LICENSE file yet**) |
| Local path | `~/personal/omaphoenix` | `~/personal/secaudit` |
| Language today | Go (single static binary) | Bash engine + Python Textual TUI + gum UI |
| What it does | Timestamped snapshots of packages, plugins, themes, config, `/etc`, services, encrypted SSH keys, stored in the user's private git repo; one-command restore | Read-only audit of ~14 areas (packages, boot, sysctl, firewall, network, SSH, users, lock, filesystem, persistence, containers, secrets, rootkit); score + grade; fix commands run only on confirmation |
| Machine-readable output | `omaphoenix doctor --json`, snapshot manifests | `secaudit --json FILE` (one JSON object per finding) |

## Documents (read in order)

| # | Doc | For |
|---|---|---|
| 1 | [docs/01-problem-and-vision.md](docs/01-problem-and-vision.md) | Why integrate; what goes wrong without it |
| 2 | [docs/02-secaudit-today.md](docs/02-secaudit-today.md) | Exact facts about secaudit as of 2026-10-09 (checks, JSON, exit codes, state) |
| 3 | [docs/03-omaphoenix-today.md](docs/03-omaphoenix-today.md) | Exact facts about OmaPhoenix as of 2026-10-09 |
| 4 | [docs/04-integration-design.md](docs/04-integration-design.md) | The audit gate, the finding contract, the mapping from findings to backup items |
| 5 | [docs/05-roadmap.md](docs/05-roadmap.md) | Phased plan with task IDs and acceptance criteria |
| 6 | [docs/06-rust-rewrite.md](docs/06-rust-rewrite.md) | Target architecture and crate choices for the joint Rust rewrite |
| 7 | [docs/adr/](docs/adr/README.md) · [docs/07-decisions.md](docs/07-decisions.md) | Decisions as ADRs (one file each), and **open questions that need both owners** |
| 8 | [docs/08-slow-download-watchdog.md](docs/08-slow-download-watchdog.md) | Generic rule: no slow or stalled download may delay or fail a restore (spec + required tests) |
| — | [RUST.md](RUST.md) | **Engineering standards for the Rust codebase:** safe, idiomatic, modern; lints, errors, testing, deps, release |
| — | [DESIGN.md](DESIGN.md) | **Single source of truth for UI/UX:** feel, voice, theme tokens from Omarchy, spring physics, keymap, consent patterns, definition of done |
| — | [AGENTS.md](AGENTS.md) | Rules for AI agents working on this project |

## Workspace layout

```
~/personal/joint-omaphoenix-secaudit/     the joint workspace (home of this project)
├── omaphoenix-secaudit/                  this design folder: README, AGENTS, DESIGN, RUST, docs/
└── secaudit/                             clone of github.com/mnshchtri/secaudit (Manish's repo)
~/personal/omaphoenix/                    the Go OmaPhoenix (private repo sumdahl/omaphoenix)
~/personal/project-ideas/omaphoenix-secaudit → symlink to the design folder (old path)
```

The Rust workspace for the joint OmaPhoenix and the standalone Security Audit (docs/06) will also live under `~/personal/joint-omaphoenix-secaudit/`.

## Where work happens

- **Phases 1–2** (integration in Go): code goes into `~/personal/omaphoenix`. secaudit gets small contract changes, made in `~/personal/secaudit` by or with Manish.
- **Phase 4** (Rust rewrite): a new repo, decided in [07-decisions](docs/07-decisions.md).
- **This folder** holds only the design. Keep it updated when a decision is made.
