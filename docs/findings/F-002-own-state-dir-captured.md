# F-002: OmaPhoenix backs up its own state folder

| | |
|---|---|
| Product | OmaPhoenix |
| Version | 0.1.0-rc.7 |
| Found | 2026-10-10, the first snapshot after a clean start on the reference laptop |
| Severity | low: one unwanted item and a false "new item" warning; no data at risk |
| Status | open |

## What happens

The first snapshot after choosing what to back up ended with:

```
✓ snapshot 8a67d3e  2026-10-10T15:25:25+05:45
  ! 1 new item(s) are being backed up that you haven't reviewed. Run: omaphoenix select
```

The item is `local:state/omaphoenix`, which is `~/.local/state/omaphoenix`, OmaPhoenix's own state:

- `gate-daily.json`: the audit cache.
- `notified.json`: when it last notified.
- `migrate.json`: progress in the Migrate section.

- `last-restore*.json` and `undo-trash/`: undo's records, and the files a restore created and an undo moved aside.

All of it is specific to this machine, and it is recaptured whenever the cache changes.

The undo trash can hold files a restore decrypted, such as SSH keys restored onto a machine and then undone. The `local` capture still runs the secret scan (`files/scan.rs` matches `BEGIN … PRIVATE KEY`, with a test). So those are skipped today, but only because the scan catches them. Skipping the folder removes the reliance.

## Reproduce

```sh
rm -f ~/.local/state/omaphoenix/*.json      # only OmaPhoenix's own cache and progress
omaphoenix select                           # w, q; state/omaphoenix isn't listed (empty folders are skipped)
omaphoenix snapshot                         # the audit writes gate-daily.json, then capture finds the folder
omaphoenix doctor | grep -i review          # BUG: "1 new item(s) ...", the item is local:state/omaphoenix
```

## Cause

- The `local` category captures folders under `~/.local/state` (`crates/phx-core/src/category/local.rs`), and nothing excludes OmaPhoenix's own.
- The audit runs before capture and writes `gate-daily.json` into that folder, so even an empty folder at save time is non-empty by capture time.

## Workaround

Run `omaphoenix select`, open `local`, untick `state/omaphoenix`, and press `w`.

## Fix

Never detect or capture `$XDG_STATE_HOME/omaphoenix`, whatever the selection says, the same way `files::JUNK` is never captured. Check whether Security Audit's state (`~/.local/state/secaudit`, its history) should be skipped too, or kept on purpose so the history travels.

Test (phx-core, `category/local.rs`): a fake home with `.local/state/omaphoenix/gate-daily.json` and another state folder; `detect` lists the other folder and not `state/omaphoenix`. Fails today.
