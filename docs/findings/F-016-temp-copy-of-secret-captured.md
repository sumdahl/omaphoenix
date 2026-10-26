# F-016: A program's temp file (e.g. a copy of `.claude.json`) is backed up as config

| | |
|---|---|
| Product | OmaPhoenix |
| Version | 0.1.0-rc.8 |
| Found | 2026-10-23, the snapshot during `omarchy update` on the reference laptop |
| Severity | medium: a copy of an opt-in secret can bypass the opt-in; here it held no keys |
| Status | fixed in 0.1.0-rc.9; test `phx_core::category::files::names::tests::names` |

## What happens

```
✓ snapshot bb70797  2026-10-23T14:49:16+05:45
  ! 14 item(s) in the last snapshot haven't been reviewed yet.
```

One of the 14 was `config:.claude.json.tmp.159014.3290931ff618`, a 163 KB leftover from Claude Code saving `~/.claude.json`. Claude Code writes a temp copy, then renames it; this copy was left behind.

`.claude.json` itself is an opt-in secret, unticked on the reference laptop. The temp copy has a different name, so it went in as ordinary config and into the private GitHub backup. A scan found no token-shaped values in it, but the same path could carry a real secret.

## Reproduce

```sh
cp ~/.claude.json ~/.claude.json.tmp.1.test
omaphoenix select        # it appears under config, ticked
rm ~/.claude.json.tmp.1.test
```

## Cause

- Home dotfiles are detected by name.
- `files::JUNK` has no temp-file patterns.
- The secret check `is_secret` matches the real name (`.claude.json`), not its temp copies.

## Workaround

Untick it in Backup (`/` search: `claude tmp`), then delete the leftover file.

## Fix

1. Never capture temp files: add `*.tmp`, `*.tmp.*`, `*~` and `.#*` to `files::JUNK`.
2. Treat a name that starts with a secret's name followed by `.` (`.claude.json.tmp.…`, `.netrc.bak`) as that secret, so a copy can't bypass the opt-in.

Test (phx-core, `category`): a fake home with `.claude.json.tmp.1` and `.netrc.bak`. Neither is offered as config.
