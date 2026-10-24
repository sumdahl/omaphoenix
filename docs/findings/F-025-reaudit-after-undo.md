# F-025: After an undo, `reaudit` says "nothing to undo"

| | |
|---|---|
| Product | OmaPhoenix |
| Version | 0.1.0-rc.8 |
| Found | 2026-10-24, VM restore test T10 (#33) |
| Severity | low: wrong wording; the check can't run |
| Status | open |

## What happens

After a restore, then an `omaphoenix undo` of a later partial restore:

```
❯ omaphoenix reaudit
error: nothing to undo: no restore has been recorded on this machine
```

## Cause

`App::reaudit` (`crates/phx-core/src/app/reaudit.rs`) finds the restored snapshot through the undo journal (`last-restore.json`). Undo renames that journal to `last-restore.undone-<time>.json`, and reaudit reuses undo's error.

## Fix

1. Record the restored snapshot id separately (e.g. `last-restored-snapshot` in the state dir), and leave it alone on undo.
2. When there's none, say `nothing to re-audit: no restore has been recorded on this machine`.

Test (phx-core, `app`): restore, undo, then reaudit still finds the restored snapshot.
