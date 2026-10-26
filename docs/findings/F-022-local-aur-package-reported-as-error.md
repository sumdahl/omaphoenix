# F-022: A locally built package that isn't on the AUR shows as a raw failure

| | |
|---|---|
| Product | OmaPhoenix |
| Version | 0.1.0-rc.8 |
| Found | 2026-10-24, VM restore test T2 (#33) |
| Severity | low: wording, but it makes the restore exit as failed |
| Status | fixed in 0.1.0-rc.9; test `phx_core::category::pkg::tests::reads_the_packages_yay_cannot_find; verify in the VM rerun` |

## What happens

```
✗ pkg-aur   applied 14 · already present 0
    ✗ omaphoto: omarchy-pkg-aur-add omaphoto: exit status 1
```

`omaphoto` was built locally on the source laptop. `yay` already says `Packages not in AUR: omaphoto` during updates there.

## Fix

- **At capture:** mark foreign packages that aren't on the AUR (one `yay -Si` batch, or the AUR RPC) with the note "local build".
- **At restore:** report them as `· not on the AUR (built locally): reinstall it by hand`, not as a failure.

Test (phx-core, `category::pkg`): an AUR item whose note says "local build" is listed as skipped, not failed.
