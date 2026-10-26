# secaudit (CLI, TUI, gum UI, AI)

The `secaudit`, `secaudit-tui` and `secaudit-ui` binaries: exact ports of Manish's Bash CLI, Python/Textual TUI and gum script (README.md here). The root AGENTS.md and `crates/phx-audit/AGENTS.md` rules apply too.

## Exact ports
- **His app is the spec.** A screen, key, message, colour or toast that differs from his is a bug unless docs/10 records it as a known Textual quirk. Never redesign here; propose changes to Manish instead.
- **Prove a screen change** by capturing his app and ours in tmux on a copy of the same history (`XDG_STATE_HOME`), at 80×24, 120×36 and 160×45. Compare text, then colours cell by cell (`tmux capture-pane -e`).
- **Shared state.** History (`runs/`, `accepted.json`, `applied`), AI `config.json`, the API key and the answer cache use his layout, so his apps and ours share them. Never change a path or a format on one side only.

## AI assistant
- **His prompts are used byte for byte**, and the cache key is `sha256(model\0SYSTEM\0prompt)[:24]`. `tests/ai_parity.rs` checks both against his Python (`OMAPHOENIX_AUDIT_PY_AI_JSON`). Never reflow those string literals.
- **Nothing identifying leaves the machine:** username, hostname and home path are redacted before sending.
- **The API key never goes on argv.** It is stored mode 600 and sent with `curl` from a private 0700 temp file.
- **Claude Code runs with every tool, MCP server, slash command and hook off.**
- AI is opt-in: a one-time consent, after which nothing is sent until the user asks.

## Fix commands
The only place a shell string runs: a finding's fix command, through `bash -c`, after the user confirms it in the terminal. Sudo prompts go to the plain terminal, never a hidden one.

## Testing
- `tests/pty.rs` drives the real TUI and gum UI in a pseudo-terminal. Run it in release.
- The TUI must draw its first frame in under 100 ms and never block on a terminal query. Idle ≤ 24 MB.
