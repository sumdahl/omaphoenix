# F-020: On a fresh machine, npm tools are installed with the system npm and fail

| | |
|---|---|
| Product | OmaPhoenix |
| Version | 0.1.0-rc.8 |
| Found | 2026-10-24, VM restore test T2 (#33) |
| Severity | medium: every global npm tool fails to restore on a fresh Omarchy |
| Status | open |

## What happens

```
✗ npm:cline: npm install -g cline@3.0.61: exit status 243
```

The same happens for all four npm tools. Exit 243 is npm's EACCES.

## Cause

- On the source laptop, `npm` is mise's node, with its global prefix inside the home.
- On a fresh Omarchy, `/usr/bin/npm` (the repo's node) comes first on `PATH`, and its global prefix is `/usr`, which needs root.
- `install_args` (`crates/phx-core/src/category/pkg_other/mod.rs`) runs `npm` directly when it is on `PATH`, so it picks the system one, although the restore has just installed mise's node.

## Fix

Install npm tools with the npm that captured them. When mise manages node (it's in the snapshot's mise list), run `mise exec -- npm install -g …`. Never use a root-owned prefix.

Test (phx-core, `pkg_other`): with mise's node in the item list, `install_args` for an npm item starts with `mise exec --`.
