# OmaPhoenix × Security Audit: integration project

<p>
  <img src="share/icons/omaphoenix.svg" width="64" alt="OmaPhoenix logo">
  &nbsp;
  <img src="share/icons/secaudit.svg" width="64" alt="Security Audit logo">
</p>

> **Status: release candidate `v0.1.0-rc.2` (2026-10-10).** Both products are ported to Rust and checked against the Go and Bash versions on a real machine. Every open question is now decided (ADR-0019 to 0023): Slint for the desktop, the repo stays private, handover after the migration. The Go OmaPhoenix is finished and waits on its freeze gate (G1–G7).
>
> **Decided:**
> - The integration is built **directly in Rust** (ADR-0011).
> - The joint product is **OmaPhoenix** (ADR-0012).
> - Security Audit also gets a **standalone Rust app on the shared engine, handed to Manish** (ADR-0013).
> Owners: **Sumiran Dahal** (OmaPhoenix) and **Manish Karki** (Security Audit). Both agreed to integrate.

## One paragraph

[OmaPhoenix](#the-two-projects) snapshots an Omarchy machine so a fresh install can be restored to exactly that state. [Security Audit](#the-two-projects) audits an Arch/Omarchy machine and explains each risk. Combined, a migration becomes **audit → clean → back up → restore → re-audit**. Nothing sensitive leaves the machine for GitHub, and nothing compromised (a backdoor, a leaked token, a malicious hook) gets copied onto the fresh install. The end state is both tools rewritten as one Rust codebase, done only after the Go version of OmaPhoenix is complete and proven.

## What's here

| Binary | What | Crates |
|---|---|---|
| `omaphoenix` | Snapshot and restore an Omarchy setup; every snapshot is audited first, and risky files stay out of it. CLI (the Go version's commands) and the terminal app (`omaphoenix tui`) with DESIGN.md's six sections | `phx-core`, `phx-contract`, `secaudit-core` |
| `omaphoenix-gui` | The desktop app (Slint), `omaphoenix gui`; optional at install. In `apps/` until Q9/Q10 are decided | `phx-core`, `secaudit-core` |
| `secaudit` | Security Audit's engine: the Bash engine's 13 sections, same messages | `secaudit-core` (→ Manish) |
| `secaudit-tui` | Exact port of Manish's Python Textual app: same screens, keys, layout and colours | `secaudit` |
| `secaudit-ui` | Exact port of Manish's gum/Bash front end, gum built in | `secaudit` |
| `secaudit-gui` | Security Audit desktop app (Slint): scan, review, accept, fix, history; optional at install | `secaudit-core` |

**Nothing is backed up until you choose what.** `init` opens the Backup screen; both apps open on it until a choice is saved, and anything found later is marked new and asked about before a snapshot. Snapshots are automatic on a schedule you pick, or manual only.

**Coding-agent skills.** `share/skills/omaphoenix` and `share/skills/secaudit` teach coding agents (Claude Code, Hermes, anything reading `~/.agents/skills`) to operate both tools safely: dry-run before any restore, never choose what to back up for the user, never run a fix without confirmation. `install.sh` links them in, as Omarchy does with its own skill.

OmaPhoenix gives a choice of **terminal or desktop app**: both launchers ("OmaPhoenix (Terminal)" and "OmaPhoenix") appear in the app menu, and Security Audit likewise has "Security Audit" (terminal) and "Security Audit (Desktop)", and `omaphoenix tui` / `omaphoenix gui` open them directly.

```sh
cargo build --release -p omaphoenix -p secaudit   # target/release/{omaphoenix,secaudit,secaudit-tui,secaudit-ui}
omaphoenix tui                                    # terminal app (bare `omaphoenix` too); or init, snapshot, list, restore, …
omaphoenix gui                                    # desktop app (needs omaphoenix-gui beside it)
omaphoenix select                                 # choose what is backed up (--all keeps everything detected)
omaphoenix timer on 1h                            # automatic snapshots (15m, 30m, 1h, 6h, daily); timer off = manual only
secaudit                                          # report
secaudit-tui                                      # Security Audit terminal app (Textual port)
secaudit-ui                                       # Security Audit menus (gum port)
(cd apps/omaphoenix-gui && cargo run --release)   # desktop apps from source (also apps/secaudit-gui)
```

**How we know it matches the old versions** (run them yourself):
- `scripts/secaudit-parity`: Bash vs Rust audit on this machine. Result: 82/82 findings and every detail list identical.
- `scripts/omaphoenix-parity`: Go vs Rust snapshot of the real home. Result: identical tree, byte-identical manifest. It also restores across implementations, including each other's encrypted vaults.
- `scripts/peak-rss`: every path's own memory, against RUST.md §9. All within budget; a snapshot peaks at ~9 MB.

## Install

**While the repo is private** (now, ADR-0022): the laptop needs `gh` logged in to an account that can see this repo. The installer then downloads the release through `gh`.

```sh
sudo pacman -S --needed github-cli
gh auth login
gh api repos/sumdahl/omaphoenix-secaudit/contents/install.sh -H "Accept: application/vnd.github.raw" | bash
```

**Once the repo is public:**

```sh
curl -fsSL https://raw.githubusercontent.com/sumdahl/omaphoenix-secaudit/v0.1.0-rc.7/install.sh | bash
```

The installer verifies the download against `SHA256SUMS` and refuses on a mismatch. It installs the static terminal binaries to `~/.local/bin`, asks whether to add the desktop apps, adds launchers with their icons, and links the agent skills. Then it offers to back this machine up or restore a snapshot. Options go after `bash -s --` (or `| bash -s --` with `gh api`): `--gui`, `--no-gui`, `--sysctl` (Manish's kernel hardening drop-in), `--uninstall`. `OMAPHOENIX_VERSION=v0.1.0-rc.7` pins a release.

**Restoring onto another laptop** (a reinstall, a new machine): install as above, then choose restore in the menu, or:

```sh
omaphoenix restore --from https://github.com/<you>/omaphoenix-backup.git --dry-run   # look first
omaphoenix restore --from https://github.com/<you>/omaphoenix-backup.git             # then apply
```

It asks for the secrets passphrase if SSH keys were backed up encrypted.

**On someone else's laptop:** `gh auth login` with your account gives that machine access to all your private repos, your backup included. Run `gh auth logout` afterwards, or add them as a collaborator on this repo only, never on the backup repo.


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
| 9 | [docs/09-gui-spike-findings.md](docs/09-gui-spike-findings.md) | GUI spikes (Tauri, Slint, native floors): measured frames, theming, accessibility, memory; recommendation for Q9 |
| 10 | [docs/10-secaudit-port-notes.md](docs/10-secaudit-port-notes.md) | Security Audit port: parity method, ids per section, Bash quirks S1–S7 (proposals for Manish) |
| 11 | [docs/11-omaphoenix-port-notes.md](docs/11-omaphoenix-port-notes.md) | OmaPhoenix port: parity method and results, deliberate differences from Go |
| 12 | [docs/12-secaudit-handover.md](docs/12-secaudit-handover.md) | Handing Security Audit to Manish: what moves, how, what's left |
| 13 | [docs/13-migration.md](docs/13-migration.md) | Moving from the Go and Bash/Python versions: what carries over, what changes, going back |
| 14 | [docs/14-vm-restore-test.md](docs/14-vm-restore-test.md) | Full restore on a fresh Omarchy VM: the plan to build the test and the scenarios that prove it (T1–T12) |
| — | [docs/findings/](docs/findings/README.md) | **Bugs and testing findings**, one file each: how to reproduce, cause, workaround, fix and its test |
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

The Rust workspace for the joint OmaPhoenix and the standalone Security Audit (docs/06) is this repo (`crates/`, `apps/`).

## Where work happens

- **Phases 1–2** (integration in Go): code goes into `~/personal/omaphoenix`. secaudit gets small contract changes, made in `~/personal/secaudit` by or with Manish.
- **Phase 4** (Rust rewrite): this repo. Built 2026-10-10.
- **Decisions:** keep [07](docs/07-decisions.md) and the ADRs current when one is made.
