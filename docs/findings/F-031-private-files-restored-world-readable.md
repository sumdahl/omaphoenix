# F-031: owner-only files come back readable by everyone

| | |
|---|---|
| Product | OmaPhoenix |
| Version | 0.1.0 (VM test, docs/14) |
| Found | 2026-10-28, T10 of the full VM test on the 0.1.0 release commit (`reaudit`: `secrets.credential_perms` on `.config/gh/hosts.yml`) |
| Severity | medium |
| Status | fixed in 0.1.0 (`F-028 to F-032`) |

## What happens

Files that were `600` on the source machine are `644` after a restore: 133 in the reference snapshot, `~/.config/gh/hosts.yml` among them. `omaphoenix reaudit` reports a new risk. The home directory is `700` on Omarchy, so other users still can't reach them, but the files themselves are no longer private.

## Reproduce

1. `chmod 600 ~/.config/gh/hosts.yml`, snapshot.
2. Restore that snapshot from its git repo on another machine: `stat -c %a ~/.config/gh/hosts.yml` prints `644`.

## Cause

Capture copied files with their mode, but git records only whether a file is executable, so every file checks out as `644` or `755` and the restore copied that mode.

## Workaround

`chmod 600` the files `reaudit` names.

## Fix

A snapshot lists its owner-only files in `private.txt`; a restore makes them owner-only again before placing them, and a restored file that is more open than the snapshot's counts as changed. Older snapshots have no list and restore as before. Test: `owner_only_files_survive_a_round_trip_through_git`.
