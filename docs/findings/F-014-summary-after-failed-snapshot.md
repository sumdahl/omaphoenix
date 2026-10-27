# F-014: After a failed snapshot, the next one's summary compares against the wrong "before"

| | |
|---|---|
| Product | OmaPhoenix |
| Version | 0.1.0-rc.8 |
| Found | 2026-10-12, retesting rc.8 on the reference laptop |
| Severity | low: a misleading summary; the snapshots themselves are correct |
| Status | open |

## What happens

1. A snapshot fails at `git add` (a live `index.lock`).
2. The next one runs. It summarises against the failed run's **uncommitted** manifest instead of the last snapshot.

In the retest:

```
caf20bb  +1 config        # phx-demo added
(failed)                  # phx-lock-test.conf added; manifest written, nothing committed
4289197  files changed    # should say +1 config (phx-lock-test.conf)
a0ff960  -2 config        # relative to caf20bb it was -1 (phx-demo); relative to 4289197 it was -2
```

## Reproduce

```sh
R=~/.local/share/omaphoenix/repo
touch $R/.git/index.lock; echo a > ~/.config/phx-f014.conf
OMAPHOENIX_NO_NOTIFY=1 omaphoenix snapshot          # fails: index.lock File exists
rm $R/.git/index.lock
omaphoenix snapshot && omaphoenix list | head -1    # BUG: "files changed", expected "+1 config"
rm ~/.config/phx-f014.conf && omaphoenix snapshot
```

## Cause

`App::snapshot` (`crates/phx-core/src/app/snapshot.rs`):

- It reads `before` from the moved-aside working tree (`prev/manifest.json`).
- It writes the new `manifest.json` before committing.

When the commit fails, the working tree keeps that uncommitted manifest. The next run treats it as the previous snapshot.

## Workaround

None needed: the snapshot contents are right.

## Fix

Read `before` from the last commit, not the working tree: `git show HEAD:manifest.json`, or `None` when there is no commit yet. That's what "previous snapshot" means.

Test (phx-core, `app`):

1. Snapshot a fake home.
2. Add a file and write the manifest without committing, to simulate the failed run.
3. Snapshot again.
4. Assert the subject says `+1 config`.

Fails today.
