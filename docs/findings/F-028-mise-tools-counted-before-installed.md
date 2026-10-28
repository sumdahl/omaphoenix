# F-028: mise tools report "already present" but are never installed

| | |
|---|---|
| Product | OmaPhoenix |
| Version | 0.1.0 (VM test, docs/14) |
| Found | 2026-10-28, T6 of the full VM test on the 0.1.0 release commit |
| Severity | high |
| Status | fixed in 0.1.0 (`F-028 to F-032`) |

## What happens

`✓ pkg-other`, yet on the restored machine 10 of the snapshot's 12 mise tools are missing (claude, copilot, gh, go, java, …). `mise ls` lists them as `(missing)`.

## Reproduce

1. Put `gh = "latest"` in `~/.config/mise/config.toml` without installing it.
2. `mise ls --global --json` lists `gh` with `"installed": false`.
3. `omaphoenix restore --only pkg-other --dry-run` on a snapshot with `mise:gh` counts it as already present.

## Cause

The config stage restores `~/.config/mise/config.toml` before `pkg-other` runs. `detect_other` read every entry of `mise ls --global --json`, installed or not, so each tool the restored config names already looked present (`crates/phx-core/src/category/pkg_other/mod.rs`).

## Workaround

`mise install` after the restore.

## Fix

Count only versions mise reports as installed (an older mise without the field still counts). Test: `a_mise_tool_the_config_names_but_nothing_installed_is_not_present`.

With the tools now installing, `elixir` failed: it needs `erlang`, which sorts after it (`failed to resolve configured install dependency 'erlang@latest'`). A mise tool that fails is now tried once more after the others; in the VM, elixir then installed in the same run.
