# OmaPhoenix Audit (`omaphoenix audit`)

<img src="../../share/icons/omaphoenix-audit.svg" width="64" alt="OmaPhoenix Audit logo">

Security Audit by Manish Karki. A read-only security audit for Arch and Omarchy: the Rust successor of his Bash `bin/secaudit`, Python `bin/secaudit-tui` and gum `bin/secaudit-ui`, now one `omaphoenix audit` command. It never changes the system. Fix commands run only when you choose one, in your terminal.

```sh
omaphoenix audit                     # quick audit as you; sudo omaphoenix audit for full coverage
omaphoenix audit --deep              # plus package checksums and a whole-disk SUID scan
omaphoenix audit --json FILE         # findings as JSON lines (finding contract v1)
omaphoenix audit --report FILE       # plain-text copy of the report
omaphoenix audit --home DIR          # audit another home (e.g. a restored one)
sudo omaphoenix audit --user alice   # audit alice's home (default: whoever ran sudo)
omaphoenix audit tui [--scan MODE]   # the terminal app: scan, review, accept risks, run fixes, history
omaphoenix audit menu [--scan MODE] [--last]   # the menu front end
omaphoenix-audit-gui                 # the desktop app (apps/omaphoenix-audit-gui, optional at install)
```

Scans from the apps run `omaphoenix audit` as a child process (under `sudo -n` for full scans): the `omaphoenix` running them, or the one beside the desktop app, or `$OMAPHOENIX_AUDIT_ENGINE`.

## The same apps, in Rust
- **`omaphoenix audit tui`** is the Textual app ported screen by screen: the same layout at every width (trend box, sidebar, side-by-side detail at 150 columns), keys and enabled/disabled rules, messages, toasts, command palette (`ctrl+p`), keys panel (`?`) and mouse clicks. Textual's widgets are drawn the way Textual draws them (Digits, Sparkline, scrolling Tabs, the tall Input, DataTable, eighth-cell scrollbars). Compared in tmux against the Python app on the same history: 14 screens at 80×24, 120×36 and 160×45 are identical in text, and colours match to within one unit.
- **`omaphoenix audit menu`** is the gum script ported line for line, with `gum choose`/`gum confirm` built in, drawn as gum 2 draws them, in the theme's `GUM_*` colours, with the same colour-profile detection. No gum or jq needed. Six screens compared against the script are identical in text; five are byte-identical, escape codes included.
- **`omaphoenix-audit-gui`** (`apps/omaphoenix-audit-gui`, Slint) is the desktop app on the same history: score, tabs, search, accept risk, copy and run fixes (the terminal asks first), quick scans in the background, full and deep scans with sudo in a terminal window, history and export. 59 MB idle.
- **AI explanations** (Manish's `i` / `I`): Claude explains a finding, or plans the whole scan, after a one-time consent. Claude Code (`claude -p` with every tool, MCP server, slash command and hook off) is used when installed, else an Anthropic API key (`ANTHROPIC_API_KEY`, or pasted and kept in `~/.config/omaphoenix-audit/anthropic-api-key`, mode 600; sent with `curl` from a private file, never on the command line). The username, hostname and home path are replaced before anything leaves the machine. His prompts are used byte for byte, so saved answers (`~/.local/state/omaphoenix-audit/ai-cache`) match his app's; `tests/ai_parity.rs` checks prompts and cache keys against his functions. In `omaphoenix audit tui` and `omaphoenix-audit-gui`. Settings: `OMAPHOENIX_AUDIT_AI_MODEL`, `OMAPHOENIX_AUDIT_AI_EFFORT`, `OMAPHOENIX_AUDIT_CLAUDE_BIN`.
- Known differences, all Textual quirks rather than design: a compact button keeps the width of its first label until the next resize, the keys panel lists slightly fewer rows below the fold, and the AI answer's scrollbar thumb can sit a fraction of a cell off. An `ant auth login` profile is detected but not read (the API backend uses `ANTHROPIC_API_KEY`/`ANTHROPIC_AUTH_TOKEN` or the saved key).

## Compatible with the Bash version
- **Report:** the same 13 sections, word-for-word messages, the same `[PASS]`/`[WARN]`/`[FAIL]` lines and the same exit code (1 if anything fails). `scripts/audit-parity` diffs both on a machine. On the reference machine, every finding and every detail list is identical.
- **JSON:** each line adds `schema`, a stable `id` (`section.check`) and `targets` (files, units, packages) to the Bash fields (docs/04, ADR-0016). Readers that ignore unknown fields keep working.
- **History:** `~/.local/state/omaphoenix-audit/runs`, `accepted.json` and `applied`, in the layout the Python `secaudit-tui` and gum `secaudit-ui` use under `~/.local/state/secaudit`. Before 0.1.0 ours lived there too; the first run moves it (ai config likewise, from `~/.config/secaudit`).

## Crates
- **`phx-audit`:**
  - `contract`: the finding type
  - `audit`: the engine (`Ctx`, `Report`, `CommandRunner` seam, `walk`)
  - `checks`: one module per section
  - `store`: history and accepted risks
- **`phx-audit-ui`:** the front ends `omaphoenix audit` runs: `src/cli/` (the report), `src/tui/` (`audit tui`), `src/ui/` (`audit menu`), `src/ai/`.

Neither depends on the OmaPhoenix engine (ADR-0013). `omaphoenix` depends on both: `phx-audit` for its audit-before-backup gate, `phx-audit-ui` for `omaphoenix audit`.

## Tests
`cargo test -p phx-audit -p phx-audit-ui`:
- every section runs against a fake system tree and canned command output (`checks::testing::FakeSystem`), so root-only paths are covered without root
- the terminal app has screen tests at 80×24, 100×30 and 120×36 (layout, tabs, search, accept, selection, fix dialog, chooser, history, keys panel, palette)
- the menu checks bash's byte-based padding and gum's 256-colour mapping

Memory: a full quick audit peaks at ~8 MB; the TUI idles at ~5 MB.

Install with the repo's `install.sh`: it adds `omaphoenix`, an "OmaPhoenix Audit" launcher with its icon, and with `--sysctl` the kernel hardening drop-in from `extras/sysctl`.

License: MIT (ADR-0026).
