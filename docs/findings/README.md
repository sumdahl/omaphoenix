# Findings

Bugs and surprises found while testing or using the products, one file each. A finding stays here after it is fixed, as the record of what went wrong and how it was proven fixed.

**Add one:** copy [template.md](template.md) to `F-NNN-short-name.md` (next free number), fill it in, and add a row below. Write the reproduction so someone else can follow it on their own machine.

**Fix one:** the fix ships with a test that fails without it. Set the status to `fixed in <version>` and name the commit and the test.

| # | Finding | Product | Severity | Status |
|---|---|---|---|---|
| [F-001](F-001-vanished-item-flagged-new.md) | An item that disappeared before you saved is reported as new | OmaPhoenix | low | fixed in rc.8 |
| [F-002](F-002-own-state-dir-captured.md) | OmaPhoenix backs up its own state folder | OmaPhoenix | low | fixed in rc.8 |
| [F-003](F-003-dry-run-plans-decrypting-identical-keys.md) | A dry run says it would decrypt keys that are already identical | OmaPhoenix | low | fixed in rc.8 |
| [F-004](F-004-stale-git-lock-after-power-loss.md) | After a power cut during a snapshot, every later snapshot can fail | OmaPhoenix | medium | fixed in rc.8 |
| [F-005](F-005-item-restore-asks-for-passphrase.md) | Restoring one item asks for the secrets passphrase anyway | OmaPhoenix | medium | fixed in rc.8 |
| [F-006](F-006-stale-wording-in-status-and-backup.md) | Wording slips: "automatic every daily", stale hints in Backup and Settings | OmaPhoenix | low | fixed in rc.8 |
| [F-007](F-007-timeline-says-only-the-time.md) | The timeline says when each snapshot was taken, not what changed | OmaPhoenix | low | fixed in rc.8 |
| [F-008](F-008-doctor-says-keys-not-backed-up.md) | `doctor` says the SSH keys aren't backed up after saving in Backup | OmaPhoenix | medium | fixed in rc.8 |
| [F-009](F-009-restore-fails-with-sigpipe.md) | A restore can fail with `git archive … signal: 13 (SIGPIPE)` when the machine is busy | OmaPhoenix | high | fixed in rc.8 |
| [F-010](F-010-list-before-setup-says-no-such-file.md) | `omaphoenix list` before setup says "No such file or directory" | OmaPhoenix | low | fixed in rc.9 |
| [F-011](F-011-secrets-init-reincludes-excluded-key.md) | `secrets init` can include a key you excluded | OmaPhoenix | low | open |
| [F-012](F-012-doctor-unreadable-selection.md) | `doctor` doesn't say when `selection.conf` can't be read | OmaPhoenix | low | open |
| [F-013](F-013-runtime-files-make-daily-snapshots.md) | Files programs rewrite at startup make a snapshot after every boot | OmaPhoenix | low | open |
| [F-014](F-014-summary-after-failed-snapshot.md) | After a failed snapshot, the next one's summary compares against the wrong "before" | OmaPhoenix | low | open |
| [F-015](F-015-raw-git-lock-error.md) | When git's lock is held, a snapshot shows git's raw error | OmaPhoenix | low | open |
| [F-016](F-016-temp-copy-of-secret-captured.md) | A program's temp file (e.g. a copy of `.claude.json`) is backed up as config | OmaPhoenix | medium | fixed in rc.9 |
| [F-017](F-017-dry-run-counts-lines-not-packages.md) | A dry run's "planned" counts plan lines for packages, not packages | OmaPhoenix | low | open |
| [F-018](F-018-watchdog-kills-pacman-under-sudo.md) | The watchdog kills pacman mid-install when an install under sudo runs past 3 minutes | OmaPhoenix | **high** | fixed in rc.9 |
| [F-019](F-019-skipped-service-reported-failed.md) | A service skipped as "other hardware" is also reported as failed | OmaPhoenix | medium | fixed in rc.9 |
| [F-020](F-020-npm-restore-uses-system-npm.md) | On a fresh machine, npm tools are installed with the system npm and fail | OmaPhoenix | medium | fixed in rc.9 |
| [F-021](F-021-uv-tools-need-uv-first.md) | uv tools fail on a fresh machine because uv itself isn't restored | OmaPhoenix | medium | fixed in rc.9 |
| [F-022](F-022-local-aur-package-reported-as-error.md) | A locally built package that isn't on the AUR shows as a raw failure | OmaPhoenix | low | fixed in rc.9 |
| [F-023](F-023-restored-profile-locks-out-desktop.md) | A restored shell profile that sources a missing file stops the desktop from starting | OmaPhoenix | **high** | fixed in rc.9 |
| [F-024](F-024-folder-restore-pulls-hardware-files.md) | Restoring a folder pulls in the hardware-specific files inside it | OmaPhoenix | medium | fixed in rc.9 |
| [F-025](F-025-reaudit-after-undo.md) | After an undo, `reaudit` says "nothing to undo" | OmaPhoenix | low | fixed in rc.9 |
| [F-026](F-026-uv-shim-skips-mise.md) | uv tools fail to restore when a uv shim is on PATH | OmaPhoenix | medium | fixed in 0.1.0 |
| [F-027](F-027-mise-exec-installs-whole-config.md) | One blocked mise tool makes every npm tool fail to restore | OmaPhoenix | medium | fixed in 0.1.0 |
| [F-028](F-028-mise-tools-counted-before-installed.md) | mise tools report "already present" but are never installed | OmaPhoenix | high | fixed in 0.1.0 |
| [F-029](F-029-npm-uv-tools-invisible-after-restore.md) | npm and uv tools restored through mise look missing, and drop out of the next snapshot | OmaPhoenix | high | fixed in 0.1.0 |
| [F-030](F-030-shell-init-rewritten-every-restore.md) | Every restore rewrites `~/.profile` and `~/.zshrc` | OmaPhoenix | low | fixed in 0.1.0 |
| [F-031](F-031-private-files-restored-world-readable.md) | Owner-only files come back readable by everyone | OmaPhoenix | medium | fixed in 0.1.0 |
| [F-032](F-032-watchdog-defers-working-mise-downloads.md) | A slow but working mise download is deferred as "no progress" | OmaPhoenix | medium | fixed in 0.1.0 |

**Severity:** `high` loses or leaks data, or breaks a restore. `medium` is wrong behaviour with no data at risk. `low` is a wrong message or a cosmetic problem.
