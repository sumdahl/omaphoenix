# F-026: uv tools fail to restore when the npm step left a uv shim behind

| | |
|---|---|
| Product | OmaPhoenix |
| Version | 0.1.0-rc.9 (VM test, docs/14) |
| Found | 2026-10-26, VM restore test T2 |
| Severity | medium |
| Status | fixed in 0.1.0 (`F-026: uv restores through mise…`) |

## What happens

`✗ pkg-other` with `uv:<tool>: uv tool install …: exit status 127`: every uv tool fails on a fresh Omarchy.

## Reproduce

1. On a machine with mise and no uv, run `mise reshim` after any mise install that brings a `uv` shim (the npm step does).
2. `command -v uv` finds `~/.local/share/mise/shims/uv`, but `uv --version` fails: no uv version is installed behind it.

## Cause

`install_args` (`crates/phx-core/src/category/pkg_other/mod.rs`) only went through `mise exec uv@latest` when `uv` was not on PATH, and a shim counts as on PATH.

## Workaround

`mise use -g uv@latest`, then rerun the restore.

## Fix

uv always goes through `mise exec uv@latest` when mise is present. Test: `on_a_fresh_machine_npm_and_uv_go_through_mise` ("a uv shim on PATH still goes through mise").
