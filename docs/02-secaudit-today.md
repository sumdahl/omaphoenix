# 02: Security Audit as it is today (2026-10-09)

The source is `~/personal/secaudit` at commit `8a20e26`, remote `github.com/mnshchtri/secaudit` (public). Re-check this file against the repo before relying on it: the project is young and moving.

## Shape

| Part | File | Lines | Tech |
|---|---|---|---|
| Engine | `bin/secaudit` | 699 | Bash; `jq` for JSON |
| TUI | `bin/secaudit-tui` | 1588 | Python + Textual, run through `uv` (no system install) |
| Light UI | `bin/secaudit-ui` | 558 | gum |
| Installer | `install.sh` | 93 | Symlinks `bin/*` into `~/.local/bin`; `--sysctl` installs a hardening drop-in |
| Extras | `extras/sysctl/99-zz-hardening.conf` | 21 | Kernel hardening sysctl |

It is installed on Sumiran's machine as symlinks in `~/.local/bin`. Note that OmaPhoenix's `bin` category records these as symlinks ("reinstall manually").

## Sections (the order in `bin/secaudit`)

Packages & updates · Boot & disk · Kernel hardening (sysctl) · Firewall · Network exposure · SSH · Users & authentication · Screen lock · Filesystem permissions · Services & persistence · Containers & VMs · Secrets & credentials in home · Rootkit & malware indicators · Summary

## Behaviour that the integration relies on

- **Read-only.** The engine never changes the system. Fixes run only from the TUI, after confirmation.
- **Root:** `secaudit` as a normal user is a quick pass, and checks that need root are recorded as `skip`. `sudo secaudit` gives full coverage, and `--deep` adds package checksums and a whole-disk SUID scan (slow). `need_root` is the helper that skips. Under sudo, the target home is `SUDO_USER`'s.
- **Exit code:** `1` if any check fails, `0` otherwise. `2` means `--json` was given without `jq`.
- **JSON:** `secaudit --json FILE` writes one object per finding:
  ```json
  {"sev":"warn","section":"SSH","msg":"~/.ssh/id_ed25519 has no passphrase","detail":"","fix":"ssh-keygen -p -f ~/.ssh/id_ed25519","hint":""}
  ```
  `sev` is one of `fail|warn|pass|info|skip`. `fix` is a shell command; `hint` is prose.
- **Score:** `100 − 10×fail − 2×warn`, floored at 0. Grades are A≥90, B≥80, C≥70, D≥60, otherwise F. Accepted risks don't count.
- **State:** `${XDG_STATE_HOME:-~/.local/state}/secaudit/`:
  - `runs/<timestamp>/` holds `findings.jsonl`, `output.txt`, `meta.json` and `applied`. The newest 30 are kept.
  - `accepted.json` holds accepted risks, matched by section and message with numbers masked.
- **Never prints secret values;** findings name files only.

## Gaps that matter for integration

These are **proposals to Manish**, not decisions. See [07-decisions](07-decisions.md).

1. **No stable check ID.** Findings are identified by section plus message text, and the message contains paths and numbers. An integration needs a stable `id` such as `ssh.key_no_passphrase`.
2. **No structured path or target.** OmaPhoenix needs to know *which file, unit or package* a finding is about, so it can exclude exactly that item. Today the path appears only inside `msg` or `detail`.
3. **No schema version** in the JSON, so a format change would silently break consumers.
4. **No way to run one section or a subset** (e.g. only Secrets + Persistence for a fast pre-snapshot gate).
5. **No LICENSE file.** This blocks any code sharing or rewrite until the owners agree on one.
6. **Fixed home directory.** The target home comes from `$HOME`/`SUDO_USER`; there's no flag to audit a different directory, such as a restored sandbox home or a VM mount.
