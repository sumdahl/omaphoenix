# Security Audit (`secaudit`)

A read-only security audit for Arch and Omarchy: the Rust successor of the Bash `bin/secaudit`, plus its TUI. It never changes the system. Fix commands run only when you choose one, in your terminal.

```sh
secaudit                     # quick audit as you; sudo secaudit for full coverage
secaudit --deep              # plus package checksums and a whole-disk SUID scan
secaudit --json FILE         # findings as JSON lines (finding contract v1)
secaudit --report FILE       # plain-text copy of the report
secaudit --home DIR          # audit another home (e.g. a restored one)
secaudit tui                 # scan, review, accept risks, run fixes, history
```

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
- **`secaudit`:** this binary (CLI + TUI).

Neither depends on any OmaPhoenix crate (ADR-0013). OmaPhoenix depends on `secaudit-core` for its audit-before-backup gate.

## Tests
`cargo test -p secaudit-core -p secaudit`:
- every section runs against a fake system tree and canned command output (`checks::testing::FakeSystem`), so root-only paths are covered without root
- the TUI has screen tests at 80×24

Memory: a full quick audit peaks at ~8 MB; the TUI idles at ~5 MB.

License: MIT OR Apache-2.0 (ADR-0015).
