---
name: omaphoenix-audit
description: >
  REQUIRED when running or reading OmaPhoenix Audit (`omaphoenix audit`, formerly secaudit), the
  read-only security audit for Arch and Omarchy. Use when the user asks to audit or scan the machine's security, explain a
  finding or the score, accept a risk, apply a suggested fix, compare scans over time, or export
  a report. Triggers: omaphoenix audit, secaudit, security audit, security scan, hardening,
  findings. Excludes developing the OmaPhoenix source code (follow the repo's AGENTS.md).
---

# OmaPhoenix Audit Skill

OmaPhoenix Audit (audit engine by Manish Karki) checks an Arch/Omarchy machine across 13 sections (packages, boot, kernel,
firewall, network, SSH, users, screen lock, filesystem, services, containers, secrets in home,
rootkit indicators), scores it, and suggests a fix command per finding. It never changes the
system by itself.

## When This Skill MUST Be Used

- Running a scan, or reading its findings, score or history.
- Explaining a finding, accepting a risk, or applying a fix.
- Exporting a report, or using the AI explanations.

## Topic Guides

- [`scans.md`](scans.md): quick, full and deep scans; JSON and report output; reading findings
- [`fixes.md`](fixes.md): fixes, accepted risks, history and the AI explanations

## Critical Safety Rules

1. **The audit is read-only. Keep it that way.** Never run a finding's fix command unless the
   user has seen that exact command and said yes to it. One confirmation covers one command.
2. **Never print secret contents.** Findings name files that may hold tokens or keys; report the
   path, never open or show the file.
3. **Full and deep scans need sudo.** Don't run `sudo` yourself; ask the user to run the command
   in their terminal, or use `omaphoenix audit tui`, which hands the password prompt to the terminal.
4. **Don't enable the AI explanations for the user.** They send redacted findings to Claude after
   a one-time consent the user gives in the app.
5. **Read results from JSON** (`--json FILE` or the history), not by scraping coloured output.

## Commands

| Command | What it does |
|---|---|
| `omaphoenix audit` | Quick audit as the user; streams the report, then a score |
| `sudo omaphoenix audit` | Full coverage (root-only checks) |
| `omaphoenix audit --deep` | Plus package checksums and a whole-disk SUID scan (several minutes) |
| `omaphoenix audit --json FILE` / `--report FILE` | Findings as JSON lines / a plain-text copy |
| `omaphoenix audit --home DIR`, `sudo omaphoenix audit --user NAME` | Audit another home |
| `omaphoenix audit tui [--scan quick\|full\|deep]` | Terminal app: scan, review, accept, fix, history |
| `omaphoenix audit menu [--scan quick\|full\|deep] [--last]` | Menu front end |
| `omaphoenix-audit-gui` | Desktop app (if installed) |

Exit code: 1 if anything fails, else 0.

## Score

`100 − 10 × fail − 2 × warn`, floored at 0. Grades: A ≥ 90, B 80-89, C 70-79, D 60-69, F below.
Accepted risks still appear in the report; the apps list them separately.

## Where Things Live

| Path | What |
|---|---|
| `~/.local/state/omaphoenix-audit/runs/<stamp>/` | One scan: `findings.jsonl`, `meta.json`, `output.txt`, `applied`, `report.md` (after export). The last 30 are kept. |
| `~/.local/state/omaphoenix-audit/accepted.json` | Accepted risks |
| `~/.local/state/omaphoenix-audit/ai-cache/` | Saved AI answers |
| `~/.config/omaphoenix-audit/config.json` | AI consent and backend |
| `~/.config/omaphoenix-audit/anthropic-api-key` | The API key, mode 600. Never read it out. |

OmaPhoenix reads the same history and accepted risks for its audit before backup. Before 0.1.0
these lived under `secaudit/`; they move on first use.
