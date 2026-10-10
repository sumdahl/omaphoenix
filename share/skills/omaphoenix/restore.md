# Restore, Undo and New Machines

**Rule: dry run first, show it, get an explicit OK, then run for real.**

## Restoring

```bash
omaphoenix list                                   # pick a snapshot
omaphoenix restore --at <id-or-time> --dry-run    # what would change; changes nothing
omaphoenix restore --at <id-or-time>              # only after the user says yes
```

- `--at` takes a snapshot id or a timestamp prefix (`2026-10-09T14`). Without it: the latest.
- Items narrow it: `omaphoenix restore .config/hypr --at 2026-10-06 --only config`.
  Items are globs over item ids; a directory selects everything under it.
- `--only pkg-repo,theme,…` restores only those categories.
- `--all-hardware` also applies hardware-specific settings onto different hardware. Default: skipped.
- `--no-restart` skips restarting the Omarchy shell (TTY or SSH restores).

A restore goes through Omarchy's own commands, stage by stage, and reports each one. Replaced
files are kept as `<file>.pre-restore.<stamp>`. Slow downloads are deferred, not fatal: run the
same restore again later and it continues. Package installs may ask for `sudo`, so run it in a
terminal the user can see.

## Undo

```bash
omaphoenix undo --dry-run    # what would be reversed
omaphoenix undo              # only after the user says yes
```

Replaced files get their previous version back. Files the restore created move to
`~/.local/state/omaphoenix/undo-trash/<stamp>/`, never deleted. A file edited after the restore
is left alone. Installed packages are listed with the command to remove them, not removed.

## Re-audit

`omaphoenix reaudit` compares this machine's audit with the one the restored snapshot recorded:
new risks and resolved ones. Suggest it after every restore.

## A fresh machine

```bash
omaphoenix restore --from https://github.com/<user>/omaphoenix-backup.git --dry-run
```

`--from` clones the snapshot repo first (needs `gh auth login` for a private repo). Then restore
as above, with the user's OK. Encrypted SSH keys ask for the secrets passphrase.
