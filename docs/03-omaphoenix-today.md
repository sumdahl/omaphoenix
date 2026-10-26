# 03: OmaPhoenix as it is today (2026-10-09)

The source is `~/personal/omaphoenix`, private repo `github.com/sumdahl/omaphoenix-go` (renamed from `omaphoenix` at 0.1.0). CI is green. The authoritative docs are in that repo's `docs/` folder; this is the summary the integration needs.

## Shape

- **Language:** Go. One static binary (`CGO_ENABLED=0`).
- **Layout:** `internal/core` holds the logic with no UI code. `cmd/omaphoenix` is the CLI. `internal/tui` is the TUI (Bubble Tea v2).
- **Snapshots** are commits in the user's own private git repo, by default `~/.local/share/omaphoenix/repo` (override with `OMAPHOENIX_REPO`). The commit date is the timestamp.
- **Categories,** in restore order: `pkg-repo`, `pkg-aur`, `plugin`, `theme`, `config`, `local`, `bin`, `pkg-other`, `secret` (opt-in, age-encrypted), `etc`, `service`.
- **Commands:**
  - `init`, `snapshot`, `list`
  - `restore [item…] --at --dry-run --only --from`
  - `doctor --json`
  - `secrets init`
  - `timer on|off|status` (a systemd user timer, plus an Omarchy post-update hook)
  - the TUI, when run with no arguments
- **Selection:** `selection.conf` holds `category:glob` rules with `!` to exclude, last match wins. It's edited in the TUI's Backup tab.

## Security behaviour already built in

| Already filtered | Where |
|---|---|
| Secret-named files (`*token*`, `*cred*`, `*.env`, `id_*`, `.claude.json`…) | `internal/category/files.go`: `secretNames` |
| Files whose content matches token or private-key shapes | `files.go`: `secretShape` regex (GitHub, OpenAI, Anthropic, AWS, Slack, Google, GitLab, PEM keys) |
| Browser and Electron profiles | `config.go`: `profileMarkers` |
| `/etc` files that would break boot or auth (fstab, passwd, pam.d, mkinitcpio…) | `etc.go`: `etcNever` |
| Git bundles whose local diff contains a secret shape | `gitwork.go`: refuses, falls back to a scanned copy |
| SSH private keys | only through the opt-in `secret` category, encrypted with age |

## Integration seams

These are the places integration code plugs in:

- **`internal/core.App.Snapshot()`:** the gate goes here, before capture, or between detect and capture so it can filter items.
- **`internal/category.Item{ID, Note}`:** items are identified by an ID per category. Path-like IDs are relative to `~` for config and absolute for etc and bin.
- **`internal/core.Doctor()` and `Finding{Level, Area, Msg}`:** audit findings can surface here, and therefore in the TUI's Doctor tab and in `doctor --json`.
- **`internal/selection`:** a finding can become a generated exclude (`!config:.config/foo/creds.ini`) that the user sees and can override.
- **TUI:** add an "Audit" tab next to Doctor, or merge audit findings into Doctor.
