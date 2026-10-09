# 01: Problem and vision

## The situation

A user moves from an Omarchy machine they've used for months to a freshly installed one. OmaPhoenix carries the setup across: packages, plugins, themes, config, `/etc` edits, services, and optionally encrypted SSH keys.

A long-used machine also builds up things that should **not** cross over:

| Risk | Example on a real machine | What a naive backup does |
|---|---|---|
| **Secrets in config** | A token pasted into a shell rc file or an app config; `.env` files; tokens in shell history | Commits them to a GitHub repo, so they end up on the internet. |
| **Persistence and backdoors** | A user systemd unit or autostart entry nobody remembers adding; a `curl … \| sh` line in `.bashrc` | Re-installs them on the fresh machine, so the compromise survives the reinstall. |
| **Dangerous system changes** | A pacman hook piping a download into a root shell; a SUID binary in `/usr/local/bin`; a NOPASSWD sudoers drop-in | OmaPhoenix's `etc`/`bin` categories copy them and recreate them on restore as root. |
| **Weak settings** | Firewall off, sysctl hardening reverted, SSH keys without a passphrase, autologin | Faithfully reproduced, so the "fresh" machine starts weak. |
| **Junk** | Stale caches, dead autostarts, abandoned units | Bloats snapshots and confuses restores. |

OmaPhoenix already filters some of this: secret-named files, content shaped like a secret, app profiles, junk patterns, and a `/etc` never-restore list for boot and auth files. But it is a **backup tool**, not a security tool: it has no idea whether a unit is malicious or a setting is weak. That is exactly what Security Audit knows.

## Evidence from a real machine (2026-10-09)

The first fresh-VM rehearsal of OmaPhoenix showed that its snapshot of a real Omarchy machine included `~/.config/.wrangler/config/default.toml`. That file holds a live Cloudflare OAuth `oauth_token` and `refresh_token`. None of the vendor-specific token regexes matched it, and in real use it would have been pushed to GitHub.

A broader key-name check then found **12** credential files in `~/.config`: wrangler, ngrok, an Android `signing.properties` with keystore passwords, spotdl, obs-websocket, kimchi, Copilot, and IDE `.vmoptions` carrying a Toolbox token.

OmaPhoenix now excludes them, but each one was a surprise found by testing. Systematic, explainable detection is secaudit's whole job.

## The vision

```
 old machine                                                fresh machine
 ┌──────────────┐  ┌────────────┐  ┌──────────────┐        ┌───────────┐  ┌──────────────┐
 │ 1 AUDIT      │→ │ 2 CLEAN    │→ │ 3 SNAPSHOT   │ ─git─→ │ 4 RESTORE │→ │ 5 RE-AUDIT   │
 │ secaudit     │  │ fix or     │  │ omaphoenix,  │        │ omaphoenix│  │ score must be│
 │ findings     │  │ accept,    │  │ minus what   │        │           │  │ ≥ the old one│
 │              │  │ user OKs   │  │ the audit    │        │           │  │              │
 │              │  │            │  │ flagged      │        │           │  │              │
 └──────────────┘  └────────────┘  └──────────────┘        └───────────┘  └──────────────┘
```

1. **Audit before every snapshot.** Unattended timer runs use the quick, non-root audit. A manual "migration snapshot" can use the full root audit.
2. **Gate the snapshot.** Items the audit flags (a file with a secret, a suspicious unit, a dangerous hook) are excluded or block the snapshot, depending on policy. The user sees why.
3. **Clean with consent.** The TUI shows the findings next to the backup. Fixes run only after the user confirms, exactly as secaudit does today.
4. **Restore as usual.**
5. **Re-audit the new machine.** The new score should be at least the old one, and nothing restored should introduce a new failure. This proves the migration didn't import problems.

## Non-goals

- **Not antivirus or EDR.** It flags indicators; it does not clean infections.
- **No automatic fixes.** Nothing is fixed without a human saying yes.
- **No data backup.** Documents and photos stay out of scope, as in OmaPhoenix.
- **No cloud service.** Everything runs locally; the only remote is the user's own private git repo.

## End state

One product (working name: OmaPhoenix, with Security Audit as its audit engine) rewritten in **Rust**. See [06-rust-rewrite](06-rust-rewrite.md). The rewrite comes **last**: first prove the integration in the current Go and Bash code, so the Rust version ports known-good behaviour instead of discovering it.
