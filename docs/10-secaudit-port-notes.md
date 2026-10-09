# 10: Security Audit port notes (issue #5)

How the Rust port of `bin/secaudit` is checked, and what it found in the Bash version. For Manish (owner, ADR-0013): every item under "Bash quirks" is a proposal; nothing here changes the Bash repo.

## How parity is checked
- **Engine:** `crates/secaudit-core`. Each report section is a module in `src/checks/`, with Bash's messages kept word for word and a stable `id` plus `targets` on every finding (contract v1).
- **Harness:** `scripts/secaudit-parity` runs both versions on the same machine and diffs `(section, sev, msg)` for every ported section. It needs the Bash checkout and `jq`, so it runs locally, not in CI.
- **Unit tests:** each section runs against throwaway homes (`tempfile`), so behaviour is pinned without the real machine.

## Ported sections

| Section | Ids | Parity on the reference machine |
|---|---|---|
| Users & authentication | `users.uid0`, `users.passwords` (skip), `users.empty_password`, `users.root_password`, `users.legacy_hash`, `users.interactive_accounts`, `users.group_<name>`, `users.sudoers` (skip), `users.sudo_nopasswd`, `users.sudo_no_authenticate`, `users.sudoers_syntax`, `users.faillock`, `users.autologin` | ✅ as user; root paths unit-tested on a fake system |
| Filesystem permissions | `filesystem.shadow_perms`, `.sudoers_perms`, `.home_perms`, `.world_writable_files`, `.world_writable_dirs`, `.suid_scan`, `.suid_unowned`, `.path_writable`, `.path_cwd` | ✅ |
| Services & persistence | `services.failed_units`, `.system_units`, `.user_units`, `.autostart`, `.cron`, `.rc_remote_exec`, `.journal`, `.auditd` | ✅ |
| Secrets & credentials in home | `secrets.credential_perms`, `secrets.git_credentials_plaintext`, `secrets.docker_auth_plaintext`, `secrets.shell_history`, `secrets.loose_files_scan`, `secrets.loose_files` | ✅ 7/7 findings, and the 30 loose files listed are identical (2026-10-10) |

Parity run 2026-10-10: 29/29 findings identical across the four sections, and every detail list identical.

## Bash quirks found

| # | Where | What happens | Rust | Proposal |
|---|---|---|---|---|
| S1 | every `${1/#$target_home/~}` | Bash 5.2+ has `patsub_replacement` on, which tilde-expands the replacement, so messages show `/home/<user>/…` instead of `~/…` | prints `~/…`; the harness normalises this one difference | quote it: `${1/#"$target_home"/\~}` |
| S2 | `check_perm` | `stat -c %a` reads a symlink's own mode (always 777), so a symlinked credential file always fails as "readable by others" | same, for parity | `stat -L` |
| S3 | history scan | an unreadable history file reports "no obvious secrets" | same, for parity | report `skip` when unreadable |

| S4 | autologin | `${autologin// /}` strips every space after `" getty"` is appended, giving e.g. `User=ugetty` | same, for parity | join with `, ` before printing |
| S5 | shadow empty-password | `awk '$2==""'` also matches blank lines, adding an empty name | blank lines ignored | `NF>1 && $2==""` |

## Resource use
- The SUID/SGID scan dominates: `find` over `/usr /opt /etc /home /var /srv` takes ~5.6 s here, mostly `/home`, in both versions. A single shared walk for the loose-file and SUID scans would halve the home walk.
The ported section runs in about 0.5 s on the reference machine, at a peak under the 9 MB measurement floor (`scripts/peak-rss`). History files are streamed line by line, with lines capped at 64 KiB, and the home walk stops at 30 results.
