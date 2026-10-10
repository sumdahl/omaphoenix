# F-009: A restore can fail with `git archive … signal: 13 (SIGPIPE)` when the machine is busy

| | |
|---|---|
| Product | OmaPhoenix |
| Version | 0.1.0-rc.7 (and earlier) |
| Found | 2026-10-10, testing the rc.8 fixes on the reference laptop while a build ran |
| Severity | high: a restore or dry run can fail outright, at random, more often under load |
| Status | fixed in 0.1.0-rc.8; test `phx_core::store::archive::tests::reads_the_producer_to_the_end` |

## What happens

```
❯ omaphoenix restore .config/phx-demo --dry-run --no-restart
error: extract fa894da: git archive fa894da: signal: 13 (SIGPIPE)
```

Nothing is restored. The same command worked minutes earlier and 12 times in a row afterwards. It failed while a large build was keeping the CPU busy.

## Reproduce

It's a race, so a plain run rarely shows it. The test reproduces it every time without the fix: a producer writes a valid tar, pauses, then writes more padding than a pipe holds.

## Cause

`Repo::extract` (`crates/phx-core/src/store/archive.rs`) pipes `git archive` into `untar`. The tar reader stops at the end-of-archive marker, and the pipe is closed when `untar` returns.

`git archive` pads its output past that marker. If git is still writing the padding when the pipe closes, it is killed with SIGPIPE. `extract` then reports git's failure, although the tree was already unpacked in full.

## Workaround

Run the command again.

## Fix

After `untar` returns, read the rest of the stream to the end, then wait for git (`untar_from`). The test proves it: without the drain it fails with `signal: 13 (SIGPIPE)`; with it, it passes.
