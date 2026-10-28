# F-027: one blocked mise tool makes every npm tool fail to restore

| | |
|---|---|
| Product | OmaPhoenix |
| Version | 0.1.0 (VM test, docs/14) |
| Found | 2026-10-27, VM restore test of the 0.1.0 build |
| Severity | medium |
| Status | fixed in 0.1.0 (`F-027: name the tool in mise exec`) |

## What happens

`✗ pkg-other`, with each npm tool failing: `mise exec -- npm install -g cline@3.0.61: exit status 1`. The log shows mise, not npm, failing:

```
mise ERROR Failed to install npm:@opencode/cli@latest: aube install failed: refusing to add @opencode/cli: only 0 weekly downloads (threshold: 1000)
```

## Reproduce

1. With mise 2026.10.19 or later, put `opencode = "latest"` in `~/.config/mise/config.toml` without installing it.
2. `mise exec -- node --version` fails with the error above; `mise exec node -- node --version` works.

## Cause

A bare `mise exec --` first installs every missing tool in the config. Since 2026.10.19 mise refuses npm packages with under 1,000 weekly downloads, so one such tool in the restored config fails every command run through it. The npm path (and cargo when it isn't on PATH) used the bare form (`crates/phx-core/src/category/pkg_other/mod.rs`).

## Workaround

`mise exec node -- npm install -g <pkg>@<version>` by hand.

## Fix

Name the tool: `mise exec node --` for npm, `mise exec rust --` for cargo, as uv already did (`uv@latest`). Tests: `on_a_fresh_machine_npm_and_uv_go_through_mise`. In the VM, 4 of the 5 npm tools then restore; `@pencil.dev/cli` still fails building `sharp` on Node 25, a bug in that package.
