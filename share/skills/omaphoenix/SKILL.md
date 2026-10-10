---
name: omaphoenix
description: >
  REQUIRED when operating OmaPhoenix, the Omarchy snapshot and restore tool, on an installed system.
  Use when the user asks to back up or snapshot their Omarchy setup, choose what is backed up,
  set automatic or manual snapshots, list or restore a snapshot, undo a restore, move to a new
  machine, back up SSH keys, or asks why a backup failed or what a reinstall would lose.
  Triggers: omaphoenix, snapshot, backup, restore, reinstall, migrate machine, selection.conf,
  backup timer. Excludes developing the omaphoenix source code (follow the repo's AGENTS.md).
---

# OmaPhoenix Skill

OmaPhoenix takes timestamped snapshots of an Omarchy setup (packages, plugins, themes, config,
`/etc`, services, encrypted SSH keys) into the user's own git repo, and restores any of them on
this or a fresh machine. Every snapshot is audited first; risky files stay out.

This skill is for using the installed tool. It is not for changing its source.

## When This Skill MUST Be Used

- Taking, listing or restoring snapshots; undoing a restore.
- Choosing what is backed up, or editing `selection.conf`.
- Turning automatic snapshots on, off, or changing the interval.
- Setting up a new machine from a backup.
- "What would I lose if I reinstalled?", backup health, backup errors.

## Topic Guides

Read the matching guide before acting:

- [`snapshot.md`](snapshot.md): taking snapshots, choosing what is captured, the schedule
- [`restore.md`](restore.md): restoring, partial restores, undo, re-audit, new machines
- [`secrets.md`](secrets.md): encrypted SSH keys and the passphrase
- [`troubleshooting.md`](troubleshooting.md): doctor, the lock, the timer, logs, env vars

## Critical Safety Rules

1. **Never run a real `restore` or `undo` without the user's explicit OK.** Always run it with
   `--dry-run` first, show the result, and ask. A restore overwrites files and installs packages.
2. **Never choose what to back up for the user.** Send them to `omaphoenix select` (or the
   Backup section of the app). Use `select --all` only when they ask to keep everything.
3. **Never print secret values.** No keys, tokens or passphrases in output, files or commands.
   Never put a passphrase on a command line; `secrets init` asks for it itself.
4. **Never push, add or change the backup remote without asking.** The remote is the user's
   private repo; `init --github` creates a GitHub repo.
5. **Never delete the snapshot repo or its history.** It is the only copy of old snapshots.
6. **Read state as JSON where it exists:** `omaphoenix doctor --json`.

## Commands

| Command | What it does |
|---|---|
| `omaphoenix` / `omaphoenix tui` | Terminal app (sections 1-6: Pulse, Timeline, Audit, Backup, Migrate, Settings) |
| `omaphoenix gui` | Desktop app (if installed) |
| `omaphoenix init [--github owner/name \| --remote URL]` | Create the snapshot repo, then choose what to back up |
| `omaphoenix select [--all]` | Choose what snapshots capture |
| `omaphoenix snapshot` | Capture now; commits only if something changed |
| `omaphoenix list` | Snapshot ids and times, newest first |
| `omaphoenix restore [ITEMS] [--at ID] [--dry-run] [--only CATS]` | Apply a snapshot (see restore.md) |
| `omaphoenix undo [--dry-run]` | Reverse the last restore |
| `omaphoenix reaudit` | After a restore: compare this machine's audit with the snapshot's |
| `omaphoenix doctor [--json]` | What a reinstall would lose right now |
| `omaphoenix secrets init` | Include `~/.ssh`, private keys encrypted |
| `omaphoenix timer on [15m\|30m\|1h\|6h\|daily]`, `timer off`, `timer status` | Automatic or manual snapshots |

Every command has `--help`; read it before using a flag you haven't seen here.

## Where Things Live

| Path | What |
|---|---|
| `~/.local/share/omaphoenix/repo` | The snapshot repo (`OMAPHOENIX_REPO` overrides) |
| `repo/selection.conf` | What is captured (see snapshot.md) |
| `repo/reviewed.txt` | Items the user has reviewed; anything else is "new" |
| `repo/manifest.json` | The latest snapshot's items and audit summary |
| `~/.local/state/omaphoenix/` | Local state: notifications, the daily audit cache, undo trash |
| `~/.config/systemd/user/omaphoenix-snapshot.{timer,service}` | The automatic snapshot units |

## Categories

`pkg-repo`, `pkg-aur`, `pkg-other` (npm, cargo, uv, …), `plugin`, `theme`, `config`, `local`,
`bin`, `etc`, `service`, `secret` (opt-in). Category names are used by `selection.conf`,
`restore --only` and the manifest.
