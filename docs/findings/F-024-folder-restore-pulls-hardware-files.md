# F-024: Restoring a folder pulls in the hardware-specific files inside it

| | |
|---|---|
| Product | OmaPhoenix |
| Version | 0.1.0-rc.8 |
| Found | 2026-10-24, VM restore test T8 (#33) |
| Severity | medium: on different hardware it can apply another machine's monitor layout or touchpad settings without being asked |
| Status | open |

## What happens

On a VM restored from a Dell laptop's snapshot, the full restore correctly skipped `.config/hypr/monitors.lua` as "other hardware". Then:

```
omaphoenix restore .config/hypr --no-restart
✓ config    applied 1 · already present 10
```

The one file applied was `monitors.lua`: the laptop's monitor layout, now on the VM (`monitors.lua.pre-restore.…` kept the VM's own).

## Cause

`Env::want` (`crates/phx-core/src/category/env.rs`) treats an item as **explicit** when it or a parent directory is named. Hardware-specific items skip only when not explicit. Naming the folder counts as naming every file in it.

## Workaround

Name the files you want (`omaphoenix restore .config/hypr/input.lua`), or `omaphoenix undo`.

## Fix

A hardware-specific item is explicit only when **it** is named (exact id match). A parent folder doesn't count. `--all-hardware` still includes them all.

Test (phx-core, `category::env`): with `items = [".config/hypr"]` on foreign hardware, `want(".config/hypr/monitors.lua")` is false, `want(".config/hypr/input.lua")` is true, and with `items = [".config/hypr/monitors.lua"]` it's true.
