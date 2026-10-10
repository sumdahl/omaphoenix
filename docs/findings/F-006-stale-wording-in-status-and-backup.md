# F-006: Wording slips: "automatic every daily", stale hints in Backup and Settings

| | |
|---|---|
| Product | OmaPhoenix (TUI and desktop app) |
| Version | 0.1.0-rc.7 |
| Found | 2026-10-10, the Backup screen on the reference laptop |
| Severity | low: wording only |
| Status | fixed in 0.1.0-rc.8; test `omaphoenix::tui::tests::status_and_backup_wording_match_the_setup` |

## What happens

```
✓ idle · ● 2 new item(s) to review (4) · last snapshot 1m ago · automatic every daily
[-] secret     9 of 14  encrypted; 6 Settings → e first
```

1. **"automatic every daily":** the interval name is pasted after "every". That reads well for `15m`, `30m`, `1h` and `6h`, but not for `daily`.
2. **"6 Settings → e first":** the secret row tells the user to set up encryption even after `omaphoenix secrets init` has been done.
3. **Settings footer `t timer on/off`:** `t` now steps through manual, 15m, 30m, 1h, 6h and daily. It no longer just switches on or off.
4. **Settings says "Manual: press s whenever you want a snapshot"**, but `s` takes a snapshot only on Pulse and Timeline. On Settings it does nothing.

## Reproduce

```sh
omaphoenix timer on daily
omaphoenix secrets init      # once
omaphoenix tui               # the status line and 4 Backup show both
```

## Cause

1. The interval string is built in two places:
   - `crates/omaphoenix/src/tui/view/status.rs`: `format!("automatic every {i}")`
   - `apps/omaphoenix-gui/src/activity.rs`: the same format.
2. The hint is fixed text: `crates/omaphoenix/src/tui/view/backup.rs`, `"  encrypted; 6 Settings → e first"`. It never checks `secret::secrets_ready`.

## Workaround

None needed.

## Fix

1. Give `phx_core::schedule` one short label per interval (`every 15 min`, `every 30 min`, `hourly`, `every 6 hours`, `daily`) next to `describe`, and use it in both apps.
2. Show `encrypted` once `secrets_ready(&repo.dir)` is true, and keep `encrypted; 6 Settings → e first` only before that.
3. Change the Settings footer to `t next schedule · e set up encrypted secrets · …` (`crates/omaphoenix/src/tui/view/status.rs`, `hint`).
4. Chosen: the Settings text now says "take one with s on Pulse or Timeline". Making `s` global would clash with Audit's `s` (scan), and snapshotting from Backup with unsaved ticks would be confusing.

Tests: `phx_core::schedule::tests::every_interval_has_a_short_label`, and the TUI screen test `status_and_backup_wording_match_the_setup` (the hint shows before setup and is gone after). The `?` keys panel said "timer on/off" too and is fixed with them.
