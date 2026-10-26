# Fixes, Accepted Risks, History, AI

## Applying a fix

1. Show the finding and its exact `fix` command.
2. Explain what it changes and whether it needs `sudo`.
3. Run it only after the user says yes to that command. If it needs `sudo`, ask the user to run
   it in their terminal instead.
4. Re-scan to confirm.

In `omaphoenix audit tui`, `x` runs the selected finding's fix in the terminal after asking, and records
it in the run's `applied` file.

## Accepted risks

A risk the user decides to live with: `a` in `omaphoenix audit tui` (or the desktop app) accepts the
selected finding; pressing it again un-accepts. Stored in `accepted.json` by section and message.
OmaPhoenix's audit before backup ignores accepted risks. Accept only on the user's decision.

## History

`h` in `omaphoenix audit tui` lists past scans with new and resolved issues between them; `e` exports a
Markdown report into the run directory; `o` shows the full output. The newest 30 runs are kept.

## AI explanations

`i` explains the selected finding, `I` plans the whole scan, in `omaphoenix audit tui` and the desktop
app. The first use asks for consent and a backend: Claude Code when installed, else an Anthropic
API key. The username, hostname and home path are replaced before anything is sent. Answers are
saved, so asking again is free.

| Variable | Effect |
|---|---|
| `OMAPHOENIX_AUDIT_AI_MODEL` | Model for the API backend |
| `OMAPHOENIX_AUDIT_AI_EFFORT` | Effort setting for the request |
| `OMAPHOENIX_AUDIT_CLAUDE_BIN` | Path to the `claude` binary |

Never turn AI on, store a key, or send findings anywhere on the user's behalf.
