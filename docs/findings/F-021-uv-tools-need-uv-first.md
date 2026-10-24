# F-021: uv tools fail on a fresh machine because uv itself isn't restored

| | |
|---|---|
| Product | OmaPhoenix |
| Version | 0.1.0-rc.8 |
| Found | 2026-10-24, VM restore test T2 (#33) |
| Severity | medium: every uv tool fails to restore on a fresh Omarchy |
| Status | open |

## What happens

```
✗ uv:spotdl: mise exec -- uv tool install spotdl==4.5.2: exit status 1
```

The same happens for all four uv tools. `which uv` finds nothing in the VM.

## Cause

- On the source laptop, `uv` is a 57 MB binary in `~/.local/bin`, so the `bin` stage records it as "reinstall manually" without copying it.
- mise doesn't manage uv there, so `mise exec -- uv` has no uv to run either.

## Fix

Before installing uv tools, make sure uv exists: install it with mise (`mise exec uv@latest -- uv tool install …`) or with pacman (`uv` is in the Arch repos). Prefer mise, since it needs no sudo.

Test (phx-core, `pkg_other`): `install_args` for a uv item when `uv` isn't on `PATH` uses `mise exec uv@latest --`.
