# 15: Everything the product does, in the terminal app

**The rule:** the terminal app (`omaphoenix tui`) is the main way to use OmaPhoenix, so every user-facing feature must be reachable from it. A feature that only exists on the command line is a gap, listed below until it is closed. Scripting-only flags (`--json`, `--all`) are the exception.

**Keep it current:** a PR that adds or changes a command, flag or TUI action updates this table in the same PR.

Status: ✅ in the TUI · 🟡 partly · ❌ CLI only · ➖ CLI only on purpose

## OmaPhoenix

| Feature | CLI | TUI | Status |
|---|---|---|---|
| First-time setup (local repo) | `init` | Pulse / Settings `i` hands over to `init` | 🟡 no way to pass `--github owner/name` or `--remote URL` |
| Choose what is backed up | `select` | Backup: `space` toggles, `a` all, `w` saves, `/` search | ✅ |
| Keep everything detected | `select --all` | Backup `a`, then `w` | ✅ |
| Take a snapshot | `snapshot` | Pulse / Timeline `s` | ✅ |
| Snapshot timeline | `list` | Timeline | ✅ |
| Preview a restore (dry run) | `restore --dry-run --at ID` | Timeline `p` / Enter | ✅ |
| Restore a whole snapshot | `restore --at ID` | Timeline `r` (asks first) | ✅ |
| Restore only some categories | `restore --only theme,plugin` | | ❌ |
| Restore only some items | `restore 'config:.config/hypr*'` | | ❌ |
| Hardware-specific files onto other hardware | `restore --all-hardware` | | ❌ |
| Restore onto a fresh machine from a URL | `restore --from URL` | | ❌ |
| Restore without restarting the shell | `restore --no-restart` | | ➖ TTY/SSH only |
| Undo the last restore | `undo` | palette "Undo last restore" | ✅ |
| Preview the undo | `undo --dry-run` | | ❌ |
| What a reinstall would lose | `doctor` | Pulse (`d` refreshes) | ✅ |
| Doctor as JSON | `doctor --json` | | ➖ scripts and bug reports |
| Backup-health notification | `doctor --notify` | | ➖ login hook |
| Re-audit after a restore | `reaudit` | Migrate, last step | ✅ |
| Migration checklist | | Migrate | ✅ TUI only |
| Encrypted SSH keys | `secrets init` | Settings `e` hands over to `secrets init` | ✅ |
| Automatic snapshots on/off, interval | `timer on 1h` / `timer off` | Settings `t` cycles the interval | ✅ |
| Timer state and next run | `timer status` | Settings | ✅ |
| Open the desktop app | `gui` | | ➖ desktop apps are frozen (terminal focus) |

## Audit

| Feature | CLI | TUI | Status |
|---|---|---|---|
| Quick scan | `audit` | Audit section, scan | ✅ |
| Full scan (sudo) | `sudo audit` | `audit tui` only | 🟡 not in `omaphoenix tui` |
| Deep scan | `audit --deep` | `audit tui` only | 🟡 |
| Findings, accept a risk, run a fix | | Audit: `a` accepts, `x` runs the fix (asks first), `o` opens | ✅ |
| Scan history and trend | | `audit tui` only | 🟡 |
| Export a report | `audit --report FILE` | `audit tui` only (`e`) | 🟡 |
| Findings as JSON | `audit --json FILE` | | ➖ scripts |
| Audit another home | `audit --home DIR` / `--user NAME` | | ➖ restore checks |
| AI explanations | | `audit tui` only (`i` / `I`) | 🟡 |
| Menu front end | `audit menu` | | ➖ to be dropped with the one-TUI plan |

## Gaps to close

In priority order. The audit rows close by folding `audit tui` into `omaphoenix tui` (the terminal-focus plan), which needs Manish's OK to stop matching his app screen for screen.

1. **Restore picker:** tick categories and items, preview, restore (`--only`, item globs, `--all-hardware`), reusing the Backup checklist.
2. **Restore from a URL** on a fresh machine (`--from`), so a new install can start in the TUI.
3. **Setup with GitHub:** `init --github` / `--remote` from the TUI.
4. **Undo preview** (`undo --dry-run`) before the undo confirms.
5. **Full and deep scans, history, export and AI** inside `omaphoenix tui`.
