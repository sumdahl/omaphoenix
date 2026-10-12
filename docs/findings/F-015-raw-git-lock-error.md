# F-015: When git's lock is held, a snapshot shows git's raw error

| | |
|---|---|
| Product | OmaPhoenix |
| Version | 0.1.0-rc.8 |
| Found | 2026-10-12, retesting rc.8 (also noted by Manish in his #21 reproductions) |
| Severity | low: wording |
| Status | open |

## What happens

Since F-004's fix in rc.8, a git lock from the current boot is kept, because it may belong to a live git. Correct, but the snapshot then fails with:

```
error: git add -A: exit status 128: fatal: Unable to create '/home/<user>/.local/share/omaphoenix/repo/.git/index.lock': File exists.
```

It doesn't say what is going on or what to do. The timer's failure notification shows the same text.

## Reproduce

```sh
R=~/.local/share/omaphoenix/repo
touch $R/.git/index.lock; echo a > ~/.config/phx-f015.conf
OMAPHOENIX_NO_NOTIFY=1 omaphoenix snapshot      # raw git error
rm $R/.git/index.lock ~/.config/phx-f015.conf
```

## Cause

`Repo::commit` (`crates/phx-core/src/store/history.rs`) returns git's error unchanged.

## Workaround

Wait a moment and retry. If no git is running in the repo (`pgrep -a git`), delete the lock file.

## Fix

When git's error mentions `index.lock` (or another `.lock`) and `File exists`, return a store error worded like the repo lock's:

```
another git process is using the snapshot repo; try again in a moment. If none is running, remove <path>
```

Treat it like the repo lock: a timer run doesn't notify for it (`out::notify` already skips "already running").

Test (phx-core, `store`): hold a fresh `index.lock`, commit a change, and assert the error says `another git process` and names the lock path.
