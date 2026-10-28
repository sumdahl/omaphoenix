# F-030: every restore rewrites `~/.profile` and `~/.zshrc`

| | |
|---|---|
| Product | OmaPhoenix |
| Version | 0.1.0 (VM test, docs/14) |
| Found | 2026-10-28, T3 of the full VM test on the 0.1.0 release commit |
| Severity | low |
| Status | fixed in 0.1.0 (`F-028 to F-032`) |

## What happens

A rerun reports `config applied 2` and leaves another `.profile.pre-restore.<time>` and `.zshrc.pre-restore.<time>` each time.

## Reproduce

1. Restore a snapshot whose `.profile` has `. "$HOME/.cargo/env"` onto a machine without `~/.cargo/env`.
2. `omaphoenix restore --only config` again: `.profile` counts as changed.

## Cause

F-023 guards each `source` of a missing file after writing a shell init file. The next run compared the snapshot's unguarded file with the guarded one on disk, saw a difference and wrote it again (`config/mirror.rs`).

## Workaround

None needed; the extra `.pre-restore` copies can be deleted.

## Fix

A shell init file that matches the snapshot's once guarded counts as already present. Test: `guards_missing_sources_and_keeps_everything_else`.
