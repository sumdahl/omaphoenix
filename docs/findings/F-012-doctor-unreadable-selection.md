# F-012: `doctor` doesn't say when `selection.conf` can't be read

| | |
|---|---|
| Product | OmaPhoenix |
| Version | 0.1.0-rc.8 as planned |
| Found | 2026-10-10, Manish's review of #21 |
| Severity | low: a less precise message |
| Status | open |

## What happens

If `selection.conf` exists but can't be read (wrong permissions, a broken file), `doctor`'s secrets check falls back to the default rules. It then reports on keys from those defaults instead of saying the file is unreadable, so the real problem isn't shown.

## Reproduce

```sh
chmod 000 ~/.local/share/omaphoenix/repo/selection.conf
omaphoenix doctor | grep secrets   # no word about the unreadable file
chmod 644 ~/.local/share/omaphoenix/repo/selection.conf
```

## Cause

`check_secrets` (`crates/phx-core/src/doctor/mod.rs`) uses `self.rules().unwrap_or_default()`.

## Fix

On `Err`, add `warn  snapshots  can't read selection.conf (<error>); snapshots use it to choose what to back up` and skip the per-key check.

Test (phx-core, `doctor`): make `selection.conf` a directory, then assert a warning that names `selection.conf`.
