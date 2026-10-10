# F-001: An item that disappeared before you saved is reported as new

| | |
|---|---|
| Product | OmaPhoenix |
| Version | 0.1.0-rc.7 |
| Found | 2026-10-10, daily use on the reference laptop |
| Severity | low: a wrong message; nothing is captured against the user's choice |
| Status | fixed in 0.1.0-rc.8; test `phx_core::app::review::tests::an_item_that_vanished_before_the_save_is_not_reported` |

## What happens

Right after saving in Backup, `omaphoenix doctor` (and the reminder after `omaphoenix snapshot`) says:

```
! snapshots 1 new item(s) are being backed up that you haven't reviewed. Run: omaphoenix select
```

The item is gone from the machine, so it isn't being backed up and the user can't review it. Seen with `service:user/omaphoenix-snapshot.timer` after `omaphoenix timer off`.

## Reproduce

```sh
omaphoenix snapshot                                   # start clean
mkdir -p ~/.config/phx-bugtest && echo hello > ~/.config/phx-bugtest/test.conf
omaphoenix snapshot                                   # captures config:.config/phx-bugtest
rm -r ~/.config/phx-bugtest
omaphoenix select                                     # w to save, q; phx-bugtest isn't listed
omaphoenix doctor | grep -i review                    # BUG: "1 new item(s) ..."
omaphoenix snapshot && omaphoenix doctor | grep -i review   # clears
```

## Cause

The two sides come from different moments:

- Saving (`App::save_reviewed`, `crates/phx-core/src/app/review.rs`) records only what detection finds **now**.
- `App::review_needed` checks the **last snapshot's** `manifest.json` against `reviewed.txt`, so it costs no detection.

An item in the last snapshot that vanished before the save is in the manifest but not in `reviewed.txt`.

## Workaround

Take a snapshot; the new manifest no longer has the item.

## Fix

When saving, also mark every item in the current `manifest.json` reviewed. Anything the user saw on screen, and anything already in the last snapshot, has then been dealt with; only items that truly appeared after the save are flagged.

Test (phx-core, `app/review.rs`): write a manifest holding `theme:gone`, save choices without it, and assert `review_needed()` is `None`. Fails today.

The message "are being backed up" should also read "were backed up in the last snapshot", since the check reads the last snapshot.
