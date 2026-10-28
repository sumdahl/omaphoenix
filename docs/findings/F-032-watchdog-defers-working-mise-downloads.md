# F-032: a slow but working mise download is deferred as "no progress"

| | |
|---|---|
| Product | OmaPhoenix |
| Version | 0.1.0 (VM test, docs/14) |
| Found | 2026-10-28, the full VM test after the F-028 fix, which made mise tools install for the first time |
| Severity | medium |
| Status | fixed in 0.1.0 (`F-028 to F-032`) |

## What happens

`⏸ mise:maven: no progress for several minutes`, on two runs in a row, while mise's own output shows the download moving (`4.1/10.0 MB · 31 kB/s` … `6.5/10.0 MB · 37 kB/s`). Any mise download longer than three minutes is cut off, however steadily it moves.

## Reproduce

1. Start `mise install --yes maven@3.10.0` on a slow link.
2. Read `/proc/<pid>/io` every 10 s: `rchar` stays at 1112061 while `wchar` grows (126669, 919115, 1191683).

## Cause

The install watchdog (F-012) measured progress as bytes read (`rchar`) by the install's process tree. mise, like other Rust programs, reads the network with `recv`, which `rchar` doesn't count, so the download looked stalled (`crates/phx-core/src/sys/watched.rs`).

## Workaround

Rerun `omaphoenix restore`; each run gets three more minutes. Or `mise install` by hand.

## Fix

Progress is bytes read plus bytes written; the download's growing file counts. Test: `a_download_that_only_writes_still_counts_as_progress`. In the VM, maven then installed at the same speed without being deferred.
