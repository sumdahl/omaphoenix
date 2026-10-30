# Snapshots and What They Capture

## Taking one

`omaphoenix snapshot` audits the machine, recaptures every selected item, and commits only if
something changed ("no changes since the last snapshot" otherwise). It then pushes to the remote;
a failed push is kept locally and retried next run. Unchanged files are reused, so a run with
nothing new takes a few seconds.

`omaphoenix list` shows the timeline: `<id>  <time>`, newest first.

## Choosing what is captured

Nothing is decided for the user. Until they save a choice, the apps open on Backup and every
snapshot prints a reminder.

- `omaphoenix select` opens the Backup screen: categories, then items. `space` ticks, `w` saves.
- `omaphoenix select --all` keeps everything detected (secrets stay opt-in). Only when asked.
- Until the user chooses, a snapshot captures everything detected except `secret` (opt-in, encrypted) and what the secret scan or the audit leaves out; it warns "you haven't chosen what to back up yet". First-time setup asks before keeping that default.
- Items found after the last save are marked **new**. A manual snapshot asks about them first;
  the timer follows the rules and `doctor` reports what waits for review.

Saving writes `selection.conf` and `reviewed.txt` in the repo.

### `selection.conf`

One rule per line, `[!]category:glob`; the **last** matching rule wins. `#` starts a comment.

```
config:*                  # every detected config
!config:.config/chromium  # except this one
theme:*
!theme:old-*              # globs work
bin:/usr/local/bin/spotx-ctl
secret:.ssh/*             # added by `secrets init`
machine:config:.config/hypr/monitors.conf   # hardware-specific: not restored onto other hardware
```

Item ids: config/local are paths relative to `~` (`.config/hypr`), `etc` is the absolute path,
`service` is `system/<unit>` or `user/<unit>`, packages are names. The Backup screen and
`manifest.json` show the exact ids. Saving from the Backup screen rewrites the file, so hand
edits last only until the next save.

## Automatic or manual

| Command | Schedule |
|---|---|
| `omaphoenix timer on 15m` / `30m` / `1h` / `6h` / `daily` | Automatic, at that interval |
| `omaphoenix timer off` | Manual only: snapshots happen when the user takes one |
| `omaphoenix timer status` | Current state and next run |

The timer runs at the lowest CPU and I/O priority. With it on, Omarchy hooks also snapshot after
`omarchy update` and check backup health at login. Ask before changing the schedule.

## Notifications

Only unattended runs notify (timer, hooks): a failed snapshot, or backup health once a day.
Runs from a terminal or the apps don't. A run that finds another one in progress stays quiet.

## The audit before each snapshot

Failing findings leave the files and units they name out of the snapshot; warnings are captured
and noted in the manifest. The whole-filesystem permission scan and the package update check are
reused for a day (`~/.local/state/omaphoenix/gate-daily.json`); everything else runs every time.
