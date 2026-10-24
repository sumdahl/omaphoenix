# F-019: A service skipped as "other hardware" is also reported as failed

| | |
|---|---|
| Product | OmaPhoenix |
| Version | 0.1.0-rc.8 |
| Found | 2026-10-24, VM restore test T2 (#33) |
| Severity | medium: a false failure makes a good restore exit as failed (`error: 3 stage(s) failed`) |
| Status | open |

## What happens

```
✓ etc       applied 19 · already present 13
    · skipped, other hardware: /etc/systemd/system/battery-threshold.service
✗ service   applied 3 · already present 50
    ✗ system battery-threshold.service (unit file missing? its package or /etc file may not be restored)
    · skipped, other hardware: system/battery-threshold.service
```

The `etc` stage rightly skips the laptop's battery service file on different hardware, and the `service` stage says it's skipped too, yet it still tries to enable it and fails.

## Reproduce

VM restore test T2 (docs/14) with a snapshot from a laptop that has `battery-threshold.service` in `/etc/systemd/system`.

## Cause

The `service` category (`crates/phx-core/src/category/service.rs`) records the hardware skip but doesn't drop the unit from the list it enables, or it checks `env.want` only for the skip note.

## Fix

A unit that `env.want` rejects is skipped only: no enable attempt, no failure.

Test (phx-core, `category::service`): a hardware rule for `system/x.service` on foreign hardware gives `skipped` = [x] and `failed` = [].
