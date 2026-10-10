# Troubleshooting

## Start with doctor

```bash
omaphoenix doctor          # ✗ risk, ! warning, ✓ fine; exits non-zero when a risk exists
omaphoenix doctor --json   # [{"level": "risk|warn|ok", "area": "...", "msg": "..."}]
```

It covers: snapshot age, the timer, git repos with local-only or unpushed work, services,
secrets, items waiting for review, and the last audit.

## Common messages

| Message | Meaning |
|---|---|
| `a snapshot or restore is already running (maybe the automatic one); try again when it finishes` | Another run holds the lock. Wait; don't delete lock files. |
| `not initialised; run: omaphoenix init` | No snapshot repo yet. Ask before creating one (it may create a GitHub repo). |
| `you haven't chosen what to back up yet …` | Send the user to `omaphoenix select`. |
| `N item(s) in the last snapshot haven't been reviewed yet …` | Same: `omaphoenix select` shows them marked new. |
| `push failed, snapshot kept locally and pushed next run` | Network or auth. Check `gh auth status`. |
| `… skipped: contains something shaped like a secret` | Left out on purpose. Don't force it in. |

## The timer

```bash
omaphoenix timer status
systemctl --user list-timers omaphoenix-snapshot.timer
journalctl --user -u omaphoenix-snapshot.service -n 50
```

## Environment variables

| Variable | Effect |
|---|---|
| `OMAPHOENIX_REPO` | Use another snapshot repo (tests, a second backup) |
| `OMAPHOENIX_AUDIT=off` | Skip the audit before a snapshot. Only with the user's OK. |
| `OMAPHOENIX_NO_NOTIFY=1` | No desktop notification from this run |
| `OMAPHOENIX_PASSPHRASE` | Secrets passphrase for scripts only (see secrets.md) |
| `OMAPHOENIX_HARDWARE` | Pretend to be another machine model (testing hardware-specific restore) |
