# F-033: upgrading with the installer ends in "error: already initialised"

| | |
|---|---|
| Product | OmaPhoenix |
| Version | 0.1.0 (`install.sh`) |
| Found | 2026-10-28, upgrade test of the 0.1.0 release on the reference laptop (rc.8 installed) |
| Severity | low |
| Status | fixed in 0.1.1 |

## What happens

After installing, the upgrade still shows the "Back up this machine / Restore / Quit" menu. Enter picks "Back up", which runs `omaphoenix init` on the existing snapshot repo:

```
error: already initialised at ~/.local/share/omaphoenix/repo; to add a remote: git -C … remote add origin <url>
```

The install itself succeeded and nothing was changed, but the installer ends with an error.

## Reproduce

1. On a machine with OmaPhoenix set up, run the README's `curl … | bash`.
2. Press Enter at each question.

## Cause

`install.sh` showed the setup menu after every install, with no check for an existing snapshot repo.

## Workaround

Choose "Quit" at the menu, or install with `OMAPHOENIX_NO_MENU=1`.

## Fix

When the snapshot repo (`$OMAPHOENIX_REPO`, else `$XDG_DATA_HOME/omaphoenix/repo`) already exists, the installer says so and skips the menu. Checked by running the installer in a throwaway home with and without a repo.
