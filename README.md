# OmaPhoenix

<img src="share/icons/omaphoenix.svg" width="64" alt="OmaPhoenix logo">

**Reinstall Omarchy and get your whole setup back with one command.** OmaPhoenix snapshots your packages, AUR packages, plugins, themes, config, `/etc`, services and (encrypted, if you opt in) SSH keys into a private GitHub repo you own. Every snapshot is security-audited first, so leaked tokens and risky files stay off your next machine.

[![ci](https://github.com/sumdahl/omaphoenix/actions/workflows/ci.yml/badge.svg)](https://github.com/sumdahl/omaphoenix/actions/workflows/ci.yml)
[![release](https://github.com/sumdahl/omaphoenix/actions/workflows/release.yml/badge.svg)](https://github.com/sumdahl/omaphoenix/releases)

> **v0.1.2, early.** Tested by full restores on a fresh Omarchy 4.0.6 VM before every release (below), and so far used on two people's machines. Expect rough edges on yours, and please [report them](https://github.com/sumdahl/omaphoenix/issues/new/choose).

https://github.com/user-attachments/assets/58d0a1ee-e6e2-4993-8d62-57181aa6f827

| Fresh Omarchy 4.0.6 | After `omaphoenix restore` |
|---|---|
| ![A fresh Omarchy install](docs/img/before.png) | ![The same VM after the restore](docs/img/after.png) |

The same VM. The right shot was taken when the v0.1.0 restore test finished (11:35); the VM was then reset to the clean 4.0.6 checkpoint every test starts from and shot again (11:38), so the left clock is later. The 24-hour clock becoming `11:35 AM` is the restored bar config.

**Security Audit by [Manish Karki](https://github.com/mnshchtri).** The audit engine, its 13 sections and the audit's terminal app are his design, ported to Rust from [mnshchtri/secaudit](https://github.com/mnshchtri/secaudit).

## Install

```sh
curl -fsSL https://raw.githubusercontent.com/sumdahl/omaphoenix/v0.1.2/install.sh | bash
```

The installer checks the download against the release's `SHA256SUMS` and refuses a mismatch, installs `omaphoenix` to `~/.local/bin`, and then offers to:

- **back up this machine:** creates a private repo on *your* GitHub (`<you>/omaphoenix-backup`), asks about SSH keys, takes the first snapshot and sets a schedule; or
- **restore a snapshot:** shows a dry run first, then asks before applying.

Options go after `| bash -s --`: `--sysctl` (Manish's kernel hardening drop-in), `--uninstall`. `OMAPHOENIX_VERSION=v0.1.2` pins a release.

## Use it

```sh
omaphoenix                       # the terminal app: Pulse, Timeline, Audit, Backup, Migrate, Settings
omaphoenix select                # choose what is backed up; until you do, everything detected is, except SSH keys and secret-looking files
omaphoenix snapshot              # capture now
omaphoenix timer on 1h           # automatic: 15m, 30m, 1h, 6h, daily, and after every `omarchy update`; `timer off` = manual
omaphoenix restore --dry-run     # what a restore would change, changing nothing
omaphoenix restore --only theme,plugin       # some categories
omaphoenix restore '.config/hypr' --at 2026-10-20   # one folder from an older snapshot
omaphoenix undo                  # reverse the last restore
omaphoenix doctor                # what a reinstall would lose right now
omaphoenix audit                 # the security report (sudo for full coverage); `audit tui` to browse it
```

**New laptop:** install, choose restore, or `omaphoenix restore --from https://github.com/<you>/omaphoenix-backup.git --dry-run`, then without `--dry-run`.

## How we know it works

- **A full restore on a fresh Omarchy VM before each release** ([docs/14](docs/14-vm-restore-test.md), `scripts/vm`). The v0.1.0 run, onto a clean Omarchy 4.0.6 install from a real daily-driver snapshot:

  ```
  ✓ pkg-repo  applied 53 · already present 168
  ✓ pkg-aur   applied 14 · already present 0
  ✓ plugin    applied 41 · already present 0
  ✓ theme     applied 57 · already present 0
  ✓ config    applied 1432 · already present 30
  ✓ local     applied 147 · already present 1
  ✓ bin       applied 20 · already present 3
  ✗ pkg-other applied 12 · already present 12
      ✗ npm:@pencil.dev/cli: exit status 1
  ✓ etc       applied 19 · already present 13
  ✓ service   applied 1 · already present 50
  restore took 54 min · peak memory 8.6 MB
  ```

  The one failure is upstream: `@pencil.dev/cli` is deprecated by its makers (they point to `@pen.dev/cli`) and its `sharp` dependency no longer builds, the same way by hand on Node 24 and 25. The restore kept going and listed it, as it does for any package that fails to build.
- **Every bug we found is written up** ([docs/findings](docs/findings/README.md)): how to reproduce it, the cause, the fix and the test that fails without the fix. Most were found by the VM test before anyone hit them.
- **CI on every change:** clippy with warnings as errors, unit and real-terminal tests, `cargo deny` (advisories and licenses), and memory budgets ([RUST.md](RUST.md)).
- **Checked against the versions it replaces:** the Rust audit gives the same findings as Manish's Bash engine (`scripts/audit-parity`), and snapshots match the earlier Go OmaPhoenix byte for byte (`scripts/omaphoenix-parity`).

## What it won't restore

Said up front, so a gap doesn't look like a bug:

- **Anything you didn't select,** and secrets unless you opted in (`omaphoenix secrets init`).
- **Files that look like they hold a secret** (tokens, keys in config files): the audit leaves them out of the snapshot and lists them.
- **Browser and app profiles** (Brave, Chromium, Code, Signal, …): caches and sessions, not config.
- **Files over 5 MB** (large wallpapers, for example).
- **Hardware-specific settings onto different hardware** (monitor layout, input): skipped unless you name them or pass `--all-hardware`.
- **Things built from local folders** (a binary symlinked into a project you build yourself): listed with "reinstall manually".
- **Packages that fail to build on the new machine** (an AUR package or an npm tool that breaks on a newer compiler or Node): the restore keeps going and lists each one at the end.

## Is it safe?

- **Your snapshots go only to your own private GitHub repo.** OmaPhoenix has no server and sends nothing anywhere else.
- **SSH keys are encrypted** with [age](https://age-encryption.org) and a passphrase only you know, before they leave the machine.
- **The audit only reads**; a fix runs only when you pick it and confirm the exact command.
- **Restores can be undone**: replaced files are kept, and `omaphoenix undo` puts them back.
- **Verify a download came from this repo's release workflow:**

  ```sh
  gh attestation verify omaphoenix_linux_amd64.tar.gz -R sumdahl/omaphoenix
  ```

Found a security problem? See [SECURITY.md](SECURITY.md); please don't open a public issue for it.

## Build from source

```sh
cargo build --release -p omaphoenix    # target/release/omaphoenix
```

Rust 1.95 (`rust-toolchain.toml`). How the codebase is held together: [RUST.md](RUST.md) (engineering standards), [DESIGN.md](DESIGN.md) (UI and UX), [AGENTS.md](AGENTS.md) (rules for AI agents working here). The project's history and design documents are in [docs/project.md](docs/project.md).

## Changelog

### Unreleased

- **What's backed up by default, said plainly.** Until you choose, snapshots capture everything detected except SSH keys (opt-in, encrypted) and files that look like secrets; the README used to say "nothing, until you choose", which was wrong. First-time setup now asks before keeping that default if you close the chooser without saving.

### v0.1.2 (2026-10-29)

- **The installer checks your backups really leave the machine.** It now says where snapshots go (`already backing up to <your repo>`). If the snapshot repo has no GitHub remote, so snapshots only live on this laptop, it says so and offers to connect a private GitHub repo and push to it ([F-034](docs/findings/F-034-installer-trusts-local-only-repo.md)). Nothing else changed.

### v0.1.1 (2026-10-28)

- **Upgrading no longer ends in an error.** Running the installer again on a machine that already backs up showed the setup menu, and pressing Enter ran `omaphoenix init` again and stopped with `error: already initialised`. The installer now says "already backing up" and leaves your setup alone ([F-033](docs/findings/F-033-installer-offers-setup-on-upgrade.md)). Nothing else changed.

### v0.1.0 (2026-10-28)

The first public release.

- **Snapshot and restore** your whole Omarchy setup: packages, AUR packages, plugins, themes, config, `/etc`, services, tools from npm, cargo, uv and mise, and (encrypted, if you opt in) SSH keys, into a private GitHub repo you own.
- **Security Audit built in** (`omaphoenix audit`, by Manish Karki): every snapshot is audited first, and the old `secaudit` commands are replaced (the installer removes them; your scan history moves over).
- **Fixed before release**, found by full restores on a fresh Omarchy VM: mise tools now really install ([F-028](docs/findings/F-028-mise-tools-counted-before-installed.md)); npm and uv tools stay in later snapshots ([F-029](docs/findings/F-029-npm-uv-tools-invisible-after-restore.md)); private files come back private ([F-031](docs/findings/F-031-private-files-restored-world-readable.md)); slow downloads are no longer cut off ([F-032](docs/findings/F-032-watchdog-defers-working-mise-downloads.md)); and more in [docs/findings](docs/findings/README.md).

## License

[MIT](LICENSE). Copyright (c) 2026 Sumiran Dahal and Manish Karki.
