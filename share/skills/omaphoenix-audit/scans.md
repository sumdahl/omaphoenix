# Scans and Findings

## Which scan

| Scan | How | Covers |
|---|---|---|
| Quick | `omaphoenix audit` as the user | Everything readable without root; root-only checks report `skip` |
| Full | `sudo omaphoenix audit` (user runs it in a terminal) | Adds shadow, sudoers, root-only files |
| Deep | `sudo omaphoenix audit --deep` | Adds every package file checksum and a whole-disk SUID/SGID scan (slow) |

Run the quick scan yourself. For full or deep, give the user the command, or tell them to press
`s` in `omaphoenix audit tui` and pick the mode.

## Machine-readable output

```bash
omaphoenix audit --json /tmp/findings.jsonl --no-color >/dev/null
```

One JSON object per line:

| Field | Meaning |
|---|---|
| `sev` | `fail`, `warn`, `pass`, `info` or `skip` |
| `section`, `msg` | Report section and message, word for word |
| `id` | Stable check id, `section.check` (e.g. `ssh.sshd_enabled`) |
| `detail` | Extra lines (lists of files, units) |
| `fix` | Suggested shell command; may be empty |
| `hint` | Advice when there is no single command |
| `targets` | Files, units or packages the finding is about |
| `schema` | Contract version (1) |

Each scan from the apps is also saved under `~/.local/state/omaphoenix-audit/runs/<stamp>/findings.jsonl`.

## Reading findings

- Lead with `fail`, then `warn`. `skip` usually means "needs root", not a problem.
- Explain in plain words what the risk is and what the fix changes before suggesting it.
- The `secrets` section names files that look like they hold credentials. Say which files; never
  open them.
