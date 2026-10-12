# F-013: Files programs rewrite at startup make a snapshot after every boot

| | |
|---|---|
| Product | OmaPhoenix |
| Version | 0.1.0-rc.8 |
| Found | 2026-10-12, reading what the automatic snapshot after a boot changed |
| Severity | low: extra snapshots and noise, no data at risk |
| Status | open |

## What happens

The automatic snapshot after a boot (`omarchy update` hook / timer) shows `files changed`, but nothing the user changed is in it:

```
278591c snapshot 2026-10-12T09:38:57+05:45 · files changed
 files/config/.config/ibus/bus/<machine-id>-unix-0          | 4 ++--
 files/config/.config/ibus/bus/<machine-id>-unix-wayland-1  | 4 ++--
 files/config/.config/vercel-plugin/active-session.json     | 2 +-
```

- `~/.config/ibus/bus/*` holds the address of ibus's running daemon, rewritten each time ibus starts.
- `active-session.json` is a session record.

Neither is a setting, and restoring them onto a new machine is useless or harmful. It's the same class as Spotify's `*.bnk` files (docs/11).

## Reproduce

```sh
git -C ~/.local/share/omaphoenix/repo log --stat --format='%h %s' | grep -B1 -A3 'ibus/bus'
```

On a machine with ibus, reboot, then snapshot. The commit holds only the ibus bus files (and any session files).

## Cause

`files::JUNK` (`crates/phx-core/src/category/files/names.rs`) has no entry for runtime-state files that live under `~/.config`.

## Workaround

None needed. Optionally, untick `.config/ibus` in Backup.

## Fix

1. Add the ibus bus folder to the never-captured list. It's a directory, so it needs a path rule (`.config/ibus/bus`), not a name glob.
2. Add `active-session.json` and `*session*.json` only if a check of real configs shows no setting would be lost. Otherwise list it by path.
3. Note the class in docs/11 next to `*.bnk`.

Test (phx-core, `category/config`): a fake home with `.config/ibus/bus/x-unix-0` and `.config/ibus/settings.conf`. The capture holds the settings file and not the bus file.
