# Findings

Bugs and surprises found while testing or using the products, one file each. A finding stays here after it is fixed, as the record of what went wrong and how it was proven fixed.

**Add one:** copy [template.md](template.md) to `F-NNN-short-name.md` (next free number), fill it in, and add a row below. Write the reproduction so someone else can follow it on their own machine.

**Fix one:** the fix ships with a test that fails without it. Set the status to `fixed in <version>` and name the commit and the test.

| # | Finding | Product | Severity | Status |
|---|---|---|---|---|
| [F-001](F-001-vanished-item-flagged-new.md) | An item that disappeared before you saved is reported as new | OmaPhoenix | low | open |
| [F-002](F-002-own-state-dir-captured.md) | OmaPhoenix backs up its own state folder | OmaPhoenix | low | open |
| [F-003](F-003-dry-run-plans-decrypting-identical-keys.md) | A dry run says it would decrypt keys that are already identical | OmaPhoenix | low | open |
| [F-004](F-004-stale-git-lock-after-power-loss.md) | After a power cut during a snapshot, every later snapshot can fail | OmaPhoenix | medium | fixed in rc.8 |
| [F-005](F-005-item-restore-asks-for-passphrase.md) | Restoring one item asks for the secrets passphrase anyway | OmaPhoenix | medium | open |
| [F-006](F-006-stale-wording-in-status-and-backup.md) | Wording slips: "automatic every daily", stale hints in Backup and Settings | OmaPhoenix | low | open |
| [F-007](F-007-timeline-says-only-the-time.md) | The timeline says when each snapshot was taken, not what changed | OmaPhoenix | low | open |
| [F-008](F-008-doctor-says-keys-not-backed-up.md) | `doctor` says the SSH keys aren't backed up after saving in Backup | OmaPhoenix | medium | open |
| [F-010](F-010-list-before-setup-says-no-such-file.md) | `omaphoenix list` before setup says "No such file or directory" | OmaPhoenix | low | open |

**Severity:** `high` loses or leaks data, or breaks a restore. `medium` is wrong behaviour with no data at risk. `low` is a wrong message or a cosmetic problem.
