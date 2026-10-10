# F-004: After a power cut during a snapshot, every later snapshot can fail

| | |
|---|---|
| Product | OmaPhoenix |
| Version | 0.1.0-rc.7 |
| Found | 2026-10-10, reviewing what a power cut does (code review, not yet seen happen) |
| Severity | medium: no snapshot is lost, but backups stop until someone removes a file by hand |
| Status | fixed in 0.1.0-rc.8; tests `phx_core::store::tests::taking_the_lock_clears_git_locks_left_by_a_crash`, `taking_the_lock_keeps_git_locks_from_this_boot` |

## What happens

If power dies while `git add` or `git commit` runs inside the snapshot repo, git can leave `.git/index.lock` behind. Every later snapshot then fails with git's:

```
fatal: Unable to create '.../omaphoenix/repo/.git/index.lock': File exists.
```

- **The timer:** its runs keep failing. Each failure raises a "snapshot failed" notification, which goes away after 15 s, and `doctor` eventually reports the snapshot as stale.
- **Manual snapshots:** fail with the same message.

## Reproduce

Safe: it only simulates the leftover file.

```sh
touch ~/.local/share/omaphoenix/repo/.git/index.lock
omaphoenix snapshot              # fails: index.lock exists (only if something changed since the last snapshot)
rm ~/.local/share/omaphoenix/repo/.git/index.lock
```

## Cause

`Repo::commit` (`crates/phx-core/src/store/history.rs`) runs `git add -A` and `git commit` with no handling for a stale `index.lock`.

## Workaround

```sh
rm ~/.local/share/omaphoenix/repo/.git/index.lock
```

Do this only while no OmaPhoenix run is going: check `systemctl --user is-active omaphoenix-snapshot.service` says `inactive`.

## Fix

OmaPhoenix holds its own repo lock (`Repo::lock`) for the whole snapshot, restore or undo. So any `index.lock` it finds while holding that lock is a leftover from a crash. Right after taking the lock, remove `.git/index.lock` if it exists, and log a warning: `removed a leftover git lock from an interrupted run`.

Check the other git lock files the same way (`HEAD.lock`, `refs/heads/*.lock`).

Only a lock file older than the last boot (`btime` in `/proc/stat`) is removed. A newer one may belong to a live process: git's detached `gc --auto` can outlive the run that started it, and a user may run git by hand in the repo. A power cut always means a reboot, so this still covers the finding. When the boot time can't be read, nothing is removed.

Test (phx-core, `store`): create a repo, write `.git/index.lock`, change a file, and commit through the snapshot path. Assert it succeeds and the lock is gone. A second test writes a lock from this boot and asserts it is kept.

Related: an interrupted capture is already safe. The next run recaptures from the machine and only loses the reuse of unchanged files, as a speed-up.
