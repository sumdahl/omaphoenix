# F-034: the installer says "already backing up" when snapshots never leave the machine

| | |
|---|---|
| Product | OmaPhoenix |
| Version | 0.1.1 (`install.sh`) |
| Found | 2026-10-29, Manish's install of 0.1.1 on his laptop, which had a snapshot repo from earlier testing |
| Severity | medium |
| Status | fixed in 0.1.2 |

## What happens

The installer prints `already backing up (~/.local/share/omaphoenix/repo); nothing to set up` and skips setup, though that repo has no GitHub remote: every snapshot stays on the laptop, and a reinstall would lose them all.

## Reproduce

1. `omaphoenix init` (no `--github` or `--remote`), so the repo is local only.
2. Run the README's `curl … | bash`: it says "already backing up" and offers nothing.

## Cause

F-033's fix skipped setup whenever the snapshot repo existed, without checking that it had a remote. `omaphoenix init` refuses an existing repo, so the setup menu couldn't have connected it either.

## Workaround

`gh repo create <you>/omaphoenix-backup --private`, then `git -C ~/.local/share/omaphoenix/repo remote add origin <its url>`, then `omaphoenix snapshot`. Or move the repo aside and rerun the installer.

## Fix

The installer names where snapshots go (`already backing up to <url>`). A repo without a remote is reported as "only on this machine", and the installer offers to create or pick a private GitHub repo, adds it as `origin` and takes a snapshot, which pushes. Checked by running the installer in throwaway homes (no repo, a repo with a remote, a local-only repo), and the connect step against a real OmaPhoenix repo with `gh` stubbed and GitHub redirected to a local bare repo: the snapshot reached it.
