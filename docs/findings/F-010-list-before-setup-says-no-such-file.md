# F-010: `omaphoenix list` before setup says "No such file or directory"

| | |
|---|---|
| Product | OmaPhoenix (CLI) |
| Version | 0.1.0-rc.7 |
| Found | 2026-10-10, checking status on a laptop where OmaPhoenix was installed but not set up |
| Severity | low: wrong message |
| Status | fixed in 0.1.0-rc.9; test `omaphoenix::tests::before_setup::list_before_setup_says_to_run_init` |

## What happens

```
❯ omaphoenix list
error: git log --format=%h%x09%cI%x09%s: No such file or directory (os error 2)
```

It reads as if `git` is missing. The real reason is that there is no snapshot repo yet. The other commands say so:

| Command | Before setup |
|---|---|
| `omaphoenix list` | `error: git log --format=%h%x09%cI%x09%s: No such file or directory (os error 2)` |
| `omaphoenix snapshot` | `error: not initialised; run: omaphoenix init` |
| `omaphoenix select --all` | `error: not set up yet; run: omaphoenix init` |
| `omaphoenix restore --dry-run` | `error: no local snapshot repo; pass --from <git url>` |

## Reproduce

Safe: a throwaway `HOME`, read-only commands.

```sh
h=$(mktemp -d)
env -i HOME="$h" PATH=/usr/bin:/bin omaphoenix list       # BUG: "No such file or directory (os error 2)"
env -i HOME="$h" PATH=/usr/bin:/bin omaphoenix snapshot   # for contrast: "not initialised; run: omaphoenix init"
rm -r "$h"
```

## Cause

`commands::snapshot::list` (`crates/omaphoenix/src/commands/snapshot.rs`) calls `Repo::list` without checking `app.repo.exists()` first. `select.rs` and `restore.rs` do check.

`Repo::list` (`crates/phx-core/src/store/history.rs`) runs `git log` with the repo folder as its working directory. That folder doesn't exist, so starting the process fails with `ENOENT`, and the error is labelled with the git command line.

The TUI, the desktop app and `doctor` call `repo.list().unwrap_or_default()`, so they show an empty timeline and aren't affected.

## Workaround

Run `omaphoenix init` first. Until then there are no snapshots to list.

## Fix

In `list`, return `not set up yet; run: omaphoenix init` when `!app.repo.exists()`, the same wording `select` uses.

Test (omaphoenix, `commands::snapshot`): call `list` with a `Repo` whose folder doesn't exist and assert the error contains `omaphoenix init` and not `os error`. Fails today.
