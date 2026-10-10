# secaudit-core

The Security Audit engine and the finding contract, shared by Security Audit and the OmaPhoenix gate. A port of Manish's Bash `bin/secaudit` (owner: Manish, ADR-0013). The root AGENTS.md rules apply too.

## Rules
- **Read-only.** The audit never changes the system (ADR-0004). It suggests fix commands; it never runs them.
- **Parity with the Bash engine is the spec.** Messages stay word for word, severities and order match, and every finding and detail list must match on a real machine (`scripts/secaudit-parity`, docs/10). The Bash quirks S1–S7 in docs/10 are kept on purpose for parity; don't "fix" them here. They are proposals for Manish.
- **The finding contract is versioned** (`contract.rs`, ADR-0016). `id` (`section.check`) and `targets` are stable: OmaPhoenix and saved histories key on them. Adding a field is fine. Renaming an id or changing a meaning needs a schema bump and an ADR.
- **One module per report section** in `checks/`. A new check gets an id, `targets`, and a unit test.
- **The system is reached through `Ctx`.** Commands go through the `CommandRunner` seam, and files go through the context's root and home. That is how root-only paths are tested on `checks::testing::FakeSystem` without root.
- **Never print a secret.** Findings name the file, never the matching line.
- **Bounded scans.** History files stream line by line, lines are capped at 64 KiB, and home walks stop at their result limit. Full audit ≤ 48 MB own peak RSS.

## Keeping up with Manish
Pull `~/personal/secaudit` read-only, read the diff since the parity base in docs/10, port it, run the parity script, and update the base. Never push to his repo.
