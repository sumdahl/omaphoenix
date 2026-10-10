# Security Audit (`secaudit`)

<img src="../../share/icons/secaudit.svg" width="64" alt="Security Audit logo">

A read-only security audit for Arch and Omarchy: the Rust successor of Manish's Bash `bin/secaudit`, Python `bin/secaudit-tui` and gum `bin/secaudit-ui`, as three commands with the same names. It never changes the system. Fix commands run only when you choose one, in your terminal.

```sh
secaudit                     # quick audit as you; sudo secaudit for full coverage
secaudit --deep              # plus package checksums and a whole-disk SUID scan
secaudit --json FILE         # findings as JSON lines (finding contract v1)
secaudit --report FILE       # plain-text copy of the report
secaudit --home DIR          # audit another home (e.g. a restored one)
secaudit-tui [--scan MODE]   # the terminal app: scan, review, accept risks, run fixes, history
secaudit-ui [--scan MODE] [--last]   # the menu front end
```

`secaudit tui` opens the same terminal app. Both front ends run the `secaudit` engine beside them, or `$SECAUDIT_ENGINE`.

## The same apps, in Rust
- **`secaudit-tui`** is the Textual app ported screen by screen: the same layout at every width (trend box, sidebar, side-by-side detail at 150 columns), keys and enabled/disabled rules, messages, toasts, command palette (`ctrl+p`), keys panel (`?`) and mouse clicks. Textual's widgets are drawn the way Textual draws them (Digits, Sparkline, scrolling Tabs, the tall Input, DataTable, eighth-cell scrollbars). Compared in tmux against the Python app on the same history: 14 screens at 80×24, 120×36 and 160×45 are identical in text, and colours match to within one unit.
- **`secaudit-ui`** is the gum script ported line for line, with `gum choose`/`gum confirm` built in, drawn as gum 2 draws them, in the theme's `GUM_*` colours, with the same colour-profile detection. No gum or jq needed. Six screens compared against the script are identical in text; five are byte-identical, escape codes included.
- Known differences, both Textual quirks rather than design: a compact button keeps the width of its first label until the next resize, and the keys panel lists slightly fewer rows below the fold.

## Compatible with the Bash version
- **Report:** the same 13 sections, word-for-word messages, the same `[PASS]`/`[WARN]`/`[FAIL]` lines and the same exit code (1 if anything fails). `scripts/secaudit-parity` diffs both on a machine. On the reference machine, every finding and every detail list is identical.
- **JSON:** each line adds `schema`, a stable `id` (`section.check`) and `targets` (files, units, packages) to the Bash fields (docs/04, ADR-0016). Readers that ignore unknown fields keep working.
- **History:** `~/.local/state/secaudit/runs`, `accepted.json` and `applied`, in the layout the Python `secaudit-tui` and gum `secaudit-ui` use, so all three share one history.

## Crates
- **`secaudit-core`:**
  - `contract`: the finding type
  - `audit`: the engine (`Ctx`, `Report`, `CommandRunner` seam, `walk`)
  - `checks`: one module per section
  - `store`: history and accepted risks
- **`secaudit`:** the three binaries: `src/main.rs` (engine CLI), `src/tui/` (`secaudit-tui`), `src/ui/` (`secaudit-ui`).

Neither depends on any OmaPhoenix crate (ADR-0013). OmaPhoenix depends on `secaudit-core` for its audit-before-backup gate.

## Tests
`cargo test -p secaudit-core -p secaudit`:
- every section runs against a fake system tree and canned command output (`checks::testing::FakeSystem`), so root-only paths are covered without root
- the terminal app has screen tests at 80×24, 100×30 and 120×36 (layout, tabs, search, accept, selection, fix dialog, chooser, history, keys panel, palette)
- `secaudit-ui` checks bash's byte-based padding and gum's 256-colour mapping

Memory: a full quick audit peaks at ~8 MB; the TUI idles at ~5 MB.

Install with the repo's `install.sh`: it adds all three commands, a "Security Audit" launcher with its icon, and with `--sysctl` the kernel hardening drop-in from `extras/sysctl`.

License: MIT OR Apache-2.0 (ADR-0015).
