# F-023: A restored shell profile that sources a missing file stops the desktop from starting

| | |
|---|---|
| Product | OmaPhoenix |
| Version | 0.1.0-rc.8 |
| Found | 2026-10-24, VM restore test T4 (#33), fresh Omarchy 4.0.6 |
| Severity | **high**: after a restore and a reboot, the desktop session never starts. The password is accepted, the screen flashes, and it's back at the login screen. A text console still works. |
| Status | open |

## What happens

The source laptop's `~/.profile` starts with:

```sh
. "$HOME/.cargo/env"                 # added by rustup
. "$HOME/.local/share/../bin/env"    # added by uv's installer
```

On a fresh machine neither file exists:
- Rust comes from the `rust` package, not rustup;
- uv isn't restored (F-021);
- the `bin` stage only records `~/.local/bin` binaries.

Omarchy's session starter `uwsm` loads `~/.profile` with `sh`, where sourcing a missing file is fatal:

```
uwsm_env-preloader: /home/phx/.profile: line 1: /home/phx/.cargo/env: No such file or directory
wayland-wm-env@hyprland.desktop.service: Main process exited, code=exited, status=1/FAILURE
```

Once both files exist (test workaround: two empty files), the full restored desktop starts: theme, bar, plugins.

The same profile also hardcodes `/home/sumiran/…` in `PATH` lines. Those are harmless on another username, but they belong to the same class of problem.

## Reproduce

VM restore test T2 then T4 (docs/14) with a snapshot whose `~/.profile` sources `~/.cargo/env` or `~/.local/bin/env`. Log in at the greeter.

## Cause

Config files are restored as copied. Nothing checks that what they source or exec exists on the target machine, and nothing checks that a login shell still starts after the restore.

## Workaround

From a text console (Ctrl+Alt+F2), `omaphoenix undo`, or create the missing files (`mkdir -p ~/.cargo && touch ~/.cargo/env`).

## Fix

1. **Check after restore (the safety net):**
   - After the config stage, run each restored login shell as the user (`sh -lc true`, `bash -lc true`, and `zsh -lc true` when zsh is installed).
   - If one fails, put the previous version of the file it names back (`.pre-restore`).
   - Report `✗ ~/.profile: line 1 sources ~/.cargo/env, which doesn't exist on this machine (kept the original)`.
2. **Capture the small env scripts:** `~/.cargo/env` and `~/.local/bin/env` are a few lines of shell from rustup and uv. Capture them as config (they're tiny and secret-free), so the sourced files travel with the profile.
3. **At capture, warn:** like the services dependency check, list files that shell init files source from `$HOME` that the snapshot doesn't include.

Test (phx-core):
- Restore a fake home whose `.profile` sources a file not in the snapshot.
- Assert the login-shell check fails, the original `.profile` is kept, and the report names the missing file.
