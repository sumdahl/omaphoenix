# F-017: A dry run's "planned" counts plan lines for packages, not packages

| | |
|---|---|
| Product | OmaPhoenix |
| Version | 0.1.0-rc.8 |
| Found | 2026-10-24, VM restore test T1 (#33) |
| Severity | low: a misleading number; nothing is skipped |
| Status | open |

## What happens

A dry run of a 221-package snapshot on a fresh Omarchy 4.0.6 VM:

```
✓ pkg-repo  applied 0 · already present 168 · planned 2
✓ pkg-aur   applied 0 · already present 0 · planned 1
```

It reads as "2 packages to install", out of 221. In fact 53 are planned: `planned` counts plan lines, and package categories write one `bring the system up to date first` line plus one `install <all 53 names>` line. The AUR line is the same: 15 packages, counted as 1.

Every other category plans one line per item, so their counts are right.

## Reproduce

```sh
omaphoenix restore --dry-run      # on a machine missing several packages from the snapshot
```

The `pkg-repo` "planned" number is the number of lines under it, not the number of packages.

## Cause

`PkgList::restore` (`crates/phx-core/src/category/pkg.rs`) pushes `format!("install {}", missing.join(" "))` as a single `planned` entry. The report prints `planned.len()`.

## Fix

Plan one line per package (`install <name>`). Keep the update line separate. The report's existing "… N more" cut-off (15 lines) keeps it readable.

Test (phx-core, `category::pkg`): a dry run with three missing packages reports `planned` = 3 (+1 for the update line on an unsynced system) and names each package.
