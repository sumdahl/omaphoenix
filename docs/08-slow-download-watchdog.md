# 08: Slow-download watchdog (spec)

**The rule:** no single slow or stalled download may delay or fail the rest of a restore.

**Status:** built and tested in Go (`omaphoenix/internal/sys/watchdog.go`, `watched.go`). The Rust port must keep this exact behaviour, with the parity tests listed at the end.

## Why

The first fresh-VM rehearsal stalled on one JDK download running at **10 kB/s**: 193 MB, about 5 hours, while everything else waited. This is not a Java problem. Any mirror, any tool, any package can do this on any day. The handling is generic.

## Scope: every external install

| Installer | Unit watched |
|---|---|
| mise | **one tool per install** (never "install everything" in one call) |
| npm -g, cargo install, uv tool | one package |
| AUR (via `omarchy-pkg-aur-add`, yay) | the batch first; if it fails or crawls, one package at a time |
| Repo packages (pacman under sudo) | not watched: pacman runs as root (its I/O isn't readable without root) and already drops slow mirrors and tries the next one |
| Future installers | anything that downloads or builds goes through the same runner |

## Measuring progress

- Sample every **5 s**.
- Progress = the sum of `rchar` (bytes read by syscalls, sockets included) from `/proc/<pid>/io` over the installer's **whole process tree**, found through parent links in `/proc/*/stat`.
- This counts network *and* disk reads, so a long **compile is progress**, not a stall.
- Processes we may not read (root, under sudo) are skipped. The count is then conservative, never inflated.
- The child stays in our process group so it can still prompt (sudo from yay).

## Decision policy

| Parameter | Default | Meaning |
|---|---|---|
| Grace | 45 s | No verdict at all (connecting, resolving) |
| Window | 2 min | The rate is measured over this trailing window |
| MinRate | 32 KiB/s | A window below this counts as slow |
| SlowFor | 5 min | Continuously slow this long → **too slow** |
| StallFor | 3 min | Zero progress this long → **stalled** |

```
observe(t, bytes):
  record sample; drop samples older than the window (keep one at its start)
  if bytes grew: last_move = t
  if t < Grace:                       continue
  if t - last_move >= StallFor:       STALLED
  if window not yet full:             continue
  rate = Δbytes / Δt over the window
  if rate >= MinRate: slow_since = none; continue
  slow_since = slow_since or t
  if t - slow_since >= SlowFor:       TOO_SLOW
  continue
```

- **Forgiving:** brief dips and a slow start that recovers never trigger.
- **Bounded:** a 10 kB/s download is stopped within about 7–8 minutes instead of 5 hours.

## Graceful stop
1. SIGTERM to the whole tree, children first.
2. Wait 10 s, then SIGKILL whatever is left.
3. No orphans; tests check this.

## Outcome: deferred, never failed
- The item goes to `Result.Deferred` with the reason ("download too slow (10 kB/s)" / "no progress for several minutes"). The stage shows `⏸`; deferral is not a failure and doesn't fail the restore.
- Everything else continues.
- The report ends with: *"N item(s) were skipped for now because their download was too slow. Everything else is restored. Run `omaphoenix restore` again later to finish them; it continues where this run stopped."*
- The list is saved to `$XDG_STATE_HOME/omaphoenix/deferred.json`, and `doctor` warns until a later restore finishes them. A restore with nothing deferred clears the file.
- **Retry is just running restore again.** Every stage is idempotent, so finished items are skipped and only deferred ones are attempted.

## Tests that must pass (Go today, Rust after the port)
- **Policy on synthetic timelines:**
  - fast: untouched
  - 10 kB/s: too slow within 8 min
  - dead after 1 min: stalled
  - slow start then fast: untouched
  - bursty compile: untouched
  - silent during grace: untouched
- **Real process tree:** a crawling grandchild is stopped within seconds and no process survives. A fast command is untouched.
- **Restore path:** of two tools, the crawling one is deferred and the other applied, and the restore doesn't wait on the crawler.

## Not yet (ideas)
- Retrying a deferred item automatically once at the end of the same run, if the network has recovered.
- Letting the user tune the thresholds in config, for very slow connections where 32 KiB/s is normal.
