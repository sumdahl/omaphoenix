# 14: Full restore on a fresh Omarchy VM

**The goal:** prove that the Rust OmaPhoenix brings a real machine back on a freshly installed Omarchy, which is the one promise the product exists for. It has never been tested end to end. This doc covers how to build the test and how to run it.

**Status (2026-10-23):** planned. Starts after rc.9 (F-010 to F-016), because F-016 changes what a restore brings back. It is roadmap items P0.4 and P4.3's "VM scenario".

## What already exists

The Go version built a VM harness on 2026-10-09 (`~/personal/omaphoenix/scripts/vm`, notes in its `docs/08` and `docs/09`).

| Piece | State |
|---|---|
| `vm.sh`: QEMU inside Docker with `/dev/kvm`; no host packages and no sudo | works |
| `make-cidata.sh`: unattended Omarchy install (test user `phx`, throwaway `vm_key`) | works with Omarchy 4.0.4 |
| `restore-test.sh`: stage the binary and a snapshot, revert to `fresh`, restore, print the report | builds the **Go** binary |
| `~/.cache/omaphoenix-vm`: 4.0.4 ISO (6 GB), disk with the `fresh` checkpoint, old test snapshot | 15 GB on disk |

**What the Go run proved:** the unattended install, the fresh-install update path (`omarchy-update -y`), repo and AUR packages, plugins, themes, config, local files, scripts, and mise/cargo with the watchdog.

**What it found:** wrangler's OAuth tokens in a snapshot, Omarchy refusing a bare `pacman -Syu`, and a JDK download at 10 kB/s that held up the whole restore. All three are fixed.

**Why it stopped:** the VM's network ran at **55 kB/s against the host's 1.4 MB/s**. That is QEMU's user-mode network (slirp) inside rootless Docker, a property of the test setup and not of the product.

**Never verified:**
- `/etc`, services, npm/uv, the reboot and a visual check;
- everything only the Rust version does: the audit gate, the user's own choice of what to back up, the encrypted secrets vault, `undo`, `reaudit`, F-001 to F-009.

## Safety rules (binding)

- **The real laptop is never restored to.** Everything happens on the VM's own disk. The host only runs QEMU in Docker.
- **No real secret enters the VM.**
  - The test snapshot is taken with `secret` excluded.
  - The secrets scenario (T7) uses a separate snapshot made in a throwaway `HOME` with generated keys and a test passphrase.
  - Private repos go in as file mirrors (`git config --system url.<file>.insteadOf <https url>`). No GitHub token ever enters the VM.
- **The test user's NOPASSWD sudo exists only inside the VM** (`/etc/sudoers.d/99-omaphoenix-test`).
- **Never `pkill -f`** with a pattern in the same command: it kills the calling shell. Kill by PID from `pgrep` with a bracketed pattern.
- **Before reading results, check the binary hash in the VM** against the staged one: a stale binary once made a whole run meaningless.

## Implementation plan

Each phase ends with something checkable. Estimates are for the first time through.

### Phase 1: move the harness into this repo (≈30 min)

- Copy `vm.sh`, `Dockerfile`, `make-cidata.sh` and `restore-test.sh` to `scripts/vm/` here. The Go repo is frozen; this repo owns the Rust product.
- `restore-test.sh` stages the Rust binary:
  - default: `cargo build --release -p omaphoenix`, the current tree;
  - `--release vX.Y.Z`: the published tarball, checked against `SHA256SUMS`, which tests exactly what users install.
- Keep the hash check, and add `omaphoenix --version` to the report.
- Follow AGENTS.md: small files, a one-line `//!`-style header, `bash -n`, and `shellcheck` if available.

**Done when:** `scripts/vm/vm.sh boot` and `vm.sh ssh true` work from this repo against the existing disk.

### Phase 2: fix the VM network (≈30–60 min)

1. **Package cache.** Share the host's `/var/cache/pacman/pkg` read-only into the VM with `-virtfs …,mount_tag=pkgcache,readonly=on`. Mount it in the VM and add it as a second `CacheDir`. Most packages then come from disk. The yay build cache is shared the same way, read-only.
2. **Network.** Replace `-netdev user` with `-netdev passt` (add `passt` to the Dockerfile image). Keep the SSH port forward.
3. **Measure:** download one 100 MB file inside the VM, then compare a package install with and without the cache.

**Done when:** in-VM download speed is at least 50% of the host's, or every repo package in the test snapshot comes from the shared cache.

### Phase 3: refresh the VM image to the current Omarchy (≈45 min)

- Download the current Omarchy ISO (4.0.6 or later) and check its sha256.
- Check `make-cidata.sh`'s `user_configuration.json` against the new ISO's `full_disk` template (the script says where it lives).
- `vm.sh install`, then `vm.sh save fresh-<version>`. Keep the old `fresh` until the new one works, then delete the 4.0.4 ISO and checkpoint.

**Done when:** the VM boots to the desktop on the new version, `vm.sh shot` shows it, and `omarchy version` matches.

### Phase 4: build the test snapshots (≈30 min)

New script: `scripts/vm/make-test-snapshot`.
- **Main snapshot:**
  - Snapshot the reference laptop's real home into a throwaway repo (`OMAPHOENIX_REPO=$dir/snap`), with the user's own `selection.conf` plus `!secret:*`.
  - Keep the audit gate on, so the gate's exclusions are tested too.
  - Bundle a mirror of each private repo the snapshot references (from `plugins.json`/`themes.json`): `gh repo clone --mirror` on the host, handed in as a file.
- **Secrets snapshot:**
  - A throwaway `HOME` with two generated `ssh-keygen` keys, `secrets init` with a test passphrase (`OMAPHOENIX_PASSPHRASE`), one config file, then snapshot.
  - Manish's `f005-repro.sh` on #21 already does this; reuse it.
- **Check on the host before anything enters the VM:**
  - `grep -r` finds no `PRIVATE KEY`, no token shapes and no `.ssh/id_` file in the main snapshot;
  - the secrets snapshot holds only `vault.age`.

**Done when:** both snapshots exist and the leak check prints nothing.

### Phase 5: run the scenarios (below), record, fix (≈2–3 h, mostly waiting)

- A failure gets a finding in `docs/findings/` with the exact output, and its fix ships with a test as usual.
- Record each run in the **Results** table at the end of this doc: date, version, scenario, pass/fail, restore time.

**Done when:** T1 to T10 pass on the release candidate. P0.4 and P4.3 then move to `done`.

### Phase 6: keep it running (ongoing)

- Run `scripts/vm/restore-test.sh --release <rc>` before every release that changes restore, capture or secrets. Add this to the AGENTS.md verification list.
- GitHub's standard runners can't run nested KVM reliably, so this stays a local check. Revisit if a larger runner with KVM becomes available.

## Test scenarios

Each scenario starts from the clean checkpoint (`vm.sh revert fresh-<version>`) unless it says it continues the previous one. **Expect** is what passing looks like; anything else is a finding.

| # | Scenario | Steps (in the VM, as `phx`) | Expect |
|---|---|---|---|
| T1 | Dry run on a fresh machine | `omaphoenix restore --from ~/phx-test/snap.git --dry-run --no-restart` | Says the system is brought up to date first, then lists a plan per category; changes nothing (`git status` of `~` and `/etc` mtimes unchanged) |
| T2 | Full restore | Same command without `--dry-run` | `omarchy update` runs, then every stage reports `✓`. Slow items show `⏸ deferred`, never hang. Exit 0. Time recorded |
| T3 | Rerun is a no-op | `omaphoenix restore --no-restart` again | Every category `applied 0 · already present N`; deferred items from T2 are finished |
| T4 | Reboot and visual check | `sudo reboot`, wait for SSH, then `vm.sh shot after.png`; `hyprctl` checks over SSH | The desktop comes up with the snapshot's theme, bar layout, plugins and keybindings. The host's hardware items (monitor layout, touchpad, battery) are skipped as "other hardware", and the VM keeps its own screen layout |
| T5 | Services and `/etc` | `systemctl --user list-unit-files --state=enabled` and the system units; diff `/etc` items against the snapshot | Every service in the snapshot is enabled. Every captured `/etc` file matches, except `sudoers.d` (needs root; reported as skipped at capture) |
| T6 | Tools from other managers | `mise ls`, `cargo install --list`, `npm ls -g`, `uv tool list` | Every `pkg-other` item in the snapshot is installed, or deferred with a reason |
| T7 | Secrets (secrets snapshot) | Restore the secrets snapshot with `OMAPHOENIX_PASSPHRASE`, then with a wrong one, then a single config item with no passphrase available | Right passphrase: keys restored as `0600` in a `0700` `~/.ssh`, byte-identical. Wrong one: a clear error, nothing written. The single config item never asks for the passphrase (F-005) |
| T8 | Partial restore | Change two restored config files, then `omaphoenix restore .config/hypr --at <id> --no-restart` and `--only theme` | Only the named item or category changes; the replaced file is kept as `.pre-restore.<time>` |
| T9 | Undo | `omaphoenix undo --dry-run`, then `omaphoenix undo` after T8 | Exactly T8's changes are reversed. Files it created go to the undo trash; a file edited after the restore is left alone; installed packages are listed, not removed |
| T10 | After-restore health | `omaphoenix reaudit`, `omaphoenix doctor`, `omaphoenix snapshot` into a new local repo | `reaudit` reports no new risk the snapshot didn't have. `doctor` shows no deferred items. A new snapshot of the VM matches the restored items (minus hardware) |
| T11 | Interrupted restore (optional) | Start T2 and kill it by PID during the AUR stage, then run `omaphoenix restore` again and `omaphoenix undo --dry-run` | The rerun continues and completes. Undo covers what the first run did up to the kill. No stale git lock blocks the rerun (F-004 path) |
| T12 | The apps (optional) | `omaphoenix tui` over SSH in `script(1)`; `omaphoenix-gui` on the VM display + `vm.sh shot` | Both open on the restored machine; the TUI's Backup search finds items; the GUI renders with the restored theme |

**Measured for every run:** total restore time, the time per stage (from the log), every deferred item and its speed, and the peak memory of the `omaphoenix` process (`OMAPHOENIX_REPORT_HWM=1`, budget 48 MB, RUST.md §9).

## What can't be tested here

- **Real hardware:** suspend, real GPU, real Wi-Fi, a monitor layout restored onto the same laptop model. That's the physical-laptop test (G3 in the Go repo's `docs/09`), on a friend's machine.
- **GitHub as the remote:** the VM uses a file mirror so no token enters it. Cloning over HTTPS from a private repo with `gh auth login` is covered by the install docs and the physical test.

## Results

| Date | Version | Scenarios | Result | Restore time | Notes |
|---|---|---|---|---|---|
| 2026-10-09 | Go v0.3.0 (pre-rc) | T1, T2 partial | stopped: VM network 55 kB/s | — | packages, plugins, themes, config, local, scripts verified; `/etc`, services, reboot not reached |
| 2026-10-24 | 0.1.0-rc.8 | T1 | pass | 0 min | Nothing changed; peak 5.8 MB. Package "planned" counts plan lines (F-017) |
| 2026-10-24 | 0.1.0-rc.8 | T2 | **fail** | (killed at 3 min) | Watchdog killed pacman mid-install under sudo, leaving `db.lck` and hanging AUR (**F-018, high**) |
| 2026-10-24 | rc.9 branch (F-018 fix) | T2 | pass with known failures | 31 min | 8 of 11 stages ✓. ✗: npm via system npm (F-020), uv missing (F-021), hardware-skipped service reported failed (F-019), local AUR package (F-022). Peak 8 MB |
| 2026-10-24 | rc.9 branch | T3 | pass | 74 s | Every category `applied 0`; only the known failures repeat |
| 2026-10-24 | rc.9 branch | T4 | **fail**, then pass with workaround | — | Desktop session never started: restored `.profile` sources `~/.cargo/env` and `~/.local/bin/env`, missing on a fresh machine (**F-023, high**). With them created: theme, bar, plugins, ncal date as on the laptop; hardware settings kept the VM's own. Harness: the restored ufw denies SSH; fixed in `restore-test.sh` |
| 2026-10-24 | rc.9 branch | T5 | pass | — | 53/54 services enabled (the 54th skipped as other hardware); all 32 `/etc` files byte-identical |
| 2026-10-24 | rc.9 branch | T6 | partial | — | mise 12/12, cargo 5/5; npm 0/5 (F-020), uv 0/4 (F-021) |
| 2026-10-24 | rc.9 branch | T7 | pass | — | Fake keys byte-identical, 700/600/644; wrong passphrase writes nothing; config item never asks (F-005 confirmed) |
| 2026-10-24 | rc.9 branch | T8 | pass, one bug | — | Single file and `--only theme` restore exactly that. Naming a folder pulled in hardware-specific `monitors.lua` (F-024) |
| 2026-10-24 | rc.9 branch | T9 | pass | — | Undo puts the edited file back; restored theme goes to the undo trash |
| 2026-10-24 | rc.9 branch | T10 | partial | — | `doctor` clean; a new snapshot matches (gaps explained). `reaudit` after undo can't find the restore (F-025) |
