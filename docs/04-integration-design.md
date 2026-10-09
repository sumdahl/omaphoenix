# 04: Integration design

## Principle

**Two programs, one contract.** In the Go phase OmaPhoenix *calls* `secaudit` as an external command and reads its JSON. Neither codebase imports the other. This keeps ownership clean and both tools usable alone, and it gives the Rust rewrite a tested contract to port.

## The finding contract (v1, proposed)

The current JSON line is kept and **only extended** with new fields, so secaudit's own UIs keep working:

```json
{
  "schema": 1,
  "id": "secrets.env_readable",
  "sev": "fail",
  "section": "Secrets & credentials in home",
  "msg": "~/.config/app/.env is world-readable",
  "detail": "",
  "fix": "chmod 600 ~/.config/app/.env",
  "hint": "",
  "targets": [ { "kind": "file", "path": "/home/u/.config/app/.env" } ]
}
```

| New field | Meaning |
|---|---|
| `schema` | Contract version. Consumers refuse a `schema` newer than they know. |
| `id` | Stable check ID, `section.check`, lowercase. It never changes once released, even if `msg` wording does. |
| `targets[]` | What the finding is about. `kind` is `file` (absolute path), `unit` (`user/foo.service` or `system/foo.service`), `package`, or `none`. |

Changes needed in secaudit, one per check:
- a `cid "secrets.env_readable"` helper call next to `bad`/`wn`
- a `target file "$path"` helper call

These are small additions to its existing helper style. Optional and later: `--only SECTION[,SECTION]`, and `--home DIR` to audit a sandbox or restored home.

## Mapping findings to snapshot items

| Finding target | OmaPhoenix item | Default action in the gate |
|---|---|---|
| `file` under `~/.config/…` or a dotfile in `~` | `config:<path relative to ~>` | **exclude** that file from capture |
| `file` under `~/.local/…` | `local:<…>` | exclude |
| `file` in `/etc` | `etc:<abs path>` | exclude, and never restore it |
| `file` in a bin directory (SUID or unowned script) | `bin:<path>` | exclude |
| `unit` (persistence: unknown user or system unit) | `service:<scope/unit>` plus the unit file in `config`/`etc` | exclude **both** the enable and the unit file |
| `package` (e.g. a known-CVE package) | `pkg-*:<name>` | warn only; packages restore at the latest version anyway |
| `none` (machine-level: firewall off, sysctl, autologin) | — | not excludable; show in the report and re-check after restore |

Sections in **gate scope:**
- Secrets & credentials
- Services & persistence
- Filesystem permissions (SUID, writable PATH)
- Packages & updates (only the "pacman hook pipes download into root shell" check)
- Users & authentication (NOPASSWD drop-ins)

Everything else is **report-only.**

## The gate

```
omaphoenix snapshot
  ├─ run  secaudit --json <tmp> [--only <gate sections>]   (as user; never prompts)
  ├─ parse; drop findings the user accepted in secaudit's accepted.json
  ├─ for each fail/warn finding with targets → mapped item
  │     policy=block   : abort the snapshot, notify, exit 1
  │     policy=exclude : add a temporary exclude for this run, record why in the manifest
  │     policy=warn    : capture anyway, record the warning in the manifest
  ├─ capture (existing code path, with the extra excludes)
  └─ manifest.json gains  "audit": { tool, version, score, grade, run_id, excluded: [ {item, id, sev} ] }
```

- **Default policy:** `exclude` for `fail`, `warn` for `warn`, configurable per category in `selection.conf` or a new `audit.conf`.
- **If secaudit is not installed:**
  - `doctor` says so with an install hint.
  - Snapshots proceed with OmaPhoenix's built-in filters, which already exist.
  - The manifest records `audit: null`, so a restore can tell an unaudited snapshot.
- **Timer runs** use the user-level quick pass; root checks show as `skip`. A manual `omaphoenix snapshot --audit=full` runs `sudo secaudit` interactively for a migration snapshot.
- **Timing budget:** the gate must add less than about 10 s to a timer snapshot. If the full engine is slower, the gate uses `--only`.

## Restore side

- `restore --dry-run` lists the items the snapshot excluded because of the audit, with the finding IDs, so the user knows what was deliberately left behind.
- After a real restore, `omaphoenix doctor` (or the TUI) offers **re-audit**:
  - run secaudit on the new machine
  - compare it with the snapshot's recorded score
  - report new failures that the restore introduced

## UI

- **TUI:** a new **Audit** tab showing the score, grade, findings grouped by section, and which backup items each one affects. `x` runs a fix only after confirmation, by handing the terminal to `secaudit-tui` or running the fix command shown. `a` accepts a risk by writing secaudit's `accepted.json` format.
- **`doctor`** gains an "audit" area: last audit time, score, gate-relevant failures.

## Security of the integration itself

- Never put a secret value in the manifest, logs or TUI. Findings name paths only.
- The audit JSON is written to `$XDG_RUNTIME_DIR` with mode 600 and deleted after parsing. It is never committed to the snapshot repo, because finding *paths* can still be sensitive. Only the summary (counts, IDs, excluded item IDs) goes in the manifest.
- Running `sudo secaudit` is only ever interactive and explicit, never from a timer.
