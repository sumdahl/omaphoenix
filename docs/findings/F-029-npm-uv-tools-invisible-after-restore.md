# F-029: npm and uv tools restored through mise look missing, and drop out of the next snapshot

| | |
|---|---|
| Product | OmaPhoenix |
| Version | 0.1.0 (VM test, docs/14) |
| Found | 2026-10-28, T3, T6 and T10 of the full VM test on the 0.1.0 release commit |
| Severity | high |
| Status | fixed in 0.1.0 (`F-028 to F-032`) |

## What happens

A second `omaphoenix restore` reinstalls every npm and uv tool (`applied 7` instead of 0), and a snapshot taken on the restored machine leaves all of them out, so later backups lose them.

## Reproduce

On a fresh Omarchy (system npm in `/usr/bin` ahead of mise's shims on `PATH`, no uv version set in mise):

1. Restore a snapshot with npm and uv tools; they install through `mise exec node --` and `mise exec uv@latest --` (F-020, F-021, F-026).
2. `npm ls -g` shows only the system packages; `uv tool list` fails with `mise ERROR No version is set for shim: uv`.
3. `omaphoenix snapshot`: `packages/other.json` has no npm or uv tools.

## Cause

Restores install npm tools under mise's node, but detection read the global packages of the `npm` on `PATH`, the system one. For uv, detection ran `uv tool list` through the shim, which fails without a mise uv version, and read nothing (`pkg_other/npm.rs`, `pkg_other/mod.rs`).

## Workaround

Put mise's shims first on `PATH` (Omarchy's shell config does, once restored and a new shell is open) before rerunning or snapshotting.

## Fix

npm: read the `npm` on `PATH` and every node mise installed. uv: when `uv tool list` fails and mise is there, run it as `mise exec uv@latest -- uv tool list`, as the install does. Test: `finds_packages_under_mises_node_not_only_the_npm_on_path`.
