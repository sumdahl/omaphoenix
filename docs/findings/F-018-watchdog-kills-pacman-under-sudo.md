# F-018: The watchdog kills pacman mid-install when an install under sudo runs past 3 minutes

| | |
|---|---|
| Product | OmaPhoenix |
| Version | 0.1.0-rc.8 (and since the watchdog, docs/08) |
| Found | 2026-10-24, VM restore test T2 (#33), fresh Omarchy 4.0.6 |
| Severity | **high**: pacman is killed mid-transaction, which can leave a half-installed system; a stale `db.lck` then fails every later install, and the restore hangs |
| Status | open |

## What happens

On a fresh machine, the first restore installs the missing repo packages in one batch (`omarchy-pkg-add …` → `sudo pacman -S …`). In T2 that was 53 packages, 2 GB.

1. **10:38:53:** `sudo pacman -S --noconfirm --needed -- 1password …` starts.
2. **10:41:58:** pacman stops right after `installed proton-vpn-daemon`, 3 min 5 s later. There is no `transaction completed` in `/var/log/pacman.log`, and the post-transaction hooks never ran.
3. `/var/lib/pacman/db.lck` is left behind. The one-by-one fallback then fails every package with `could not lock database: File exists`.
4. The AUR stage builds 14 packages, then yay waits on the stale lock **forever**: `db.lck is present. There may be another Pacman instance running. Waiting...`

## Reproduce

Run the VM restore test (docs/14, T2) on a fresh image. Any batch install under sudo that takes longer than 3 minutes shows it.

## Cause

`InteractiveWatched` (`crates/phx-core/src/sys/watched.rs`) measures progress as the sum of `rchar` from `/proc/<pid>/io` over the install's process tree (`tree_read_bytes`).

- `/proc/<pid>/io` of a **root** process can't be read by the user OmaPhoenix runs as.
- pacman runs as root under `sudo`, so its reads, downloads and installs count as **zero**.
- Only the user-level wrappers are measured, and they do almost nothing. After 3 minutes the watchdog judges the tree stalled and stops it with SIGTERM, then SIGKILL, killing pacman mid-transaction.

The Go-era VM run didn't hit this: its batches were smaller, or its downloads went through user-level tools.

## Workaround

None that is safe for users. Don't restore onto a fresh machine with rc.8 until this is fixed. In the VM, `sudo rm /var/lib/pacman/db.lck` lets the rest continue; the interrupted transaction may still need `sudo pacman -S` again.

## Fix

1. **Never kill a process tree that holds the pacman lock.** While any process in the tree runs as root, or `/var/lib/pacman/db.lck` exists and its owner is in the tree, the watchdog must not stop it. pacman has its own download timeouts.
2. **Measure what can be measured.**
   - `/proc/<pid>/stat` (utime + stime) is world-readable, even for root processes. Use the tree's CPU time as a progress signal next to `rchar`.
   - For download rate, use the system's `/proc/net/dev` receive bytes when the tree has unreadable members.
3. **A deferred install must never leave `db.lck`.** If a stop ever happens, wait for pacman to exit cleanly (SIGINT, as Ctrl-C would) before anything else runs.

Tests (phx-core, `sys::watched`):

- A tree whose child is unreadable (simulate with a process that does no reads but burns CPU for longer than the stall limit) is **not** stopped.
- A tree that does nothing at all is still stopped.
- docs/08's spec is updated to match.
