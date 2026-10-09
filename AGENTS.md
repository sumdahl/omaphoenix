# AGENTS.md: working on the OmaPhoenix × Security Audit integration

Read this before touching anything. It applies to Claude Code and any other coding agent.

## Orientation (do this first)

1. Read `README.md`, then `docs/05-roadmap.md` to find the current phase. The first task whose status isn't `done` is next.
2. Read `docs/07-decisions.md`. **Never implement anything that depends on an open question.** Ask the user instead.
3. For code work, also read the target repo's own docs:
   - OmaPhoenix: `~/personal/omaphoenix/docs/` (especially `08-implementation-notes.md`, which lists real traps) and its `CLAUDE.md`.
   - secaudit: `~/personal/secaudit/README.md` ("Adding a check" explains its helper functions).

## Standards documents (binding)

- **[RUST.md](RUST.md):** how all Rust is written (forbid `unsafe`, no `unwrap`/`panic!`, pedantic clippy, `thiserror`/`anyhow`, `tracing`, `proptest`/`insta`/fuzzing, `cargo-deny`).
- **[DESIGN.md](DESIGN.md):** how the desktop app and the TUI look, move and speak.
- **[docs/08-slow-download-watchdog.md](docs/08-slow-download-watchdog.md):** every external install is watched, and slow or stalled ones are deferred, never blocking.

## Hard rules

- **Two owners, two repos.**
  - OmaPhoenix belongs to Sumiran Dahal. secaudit belongs to Manish Karki.
  - Never commit to or push `mnshchtri/secaudit` unless the user explicitly says this specific change is approved by Manish. Proposed secaudit changes go in a patch file or a branch in the local clone, and the user forwards them.
- **Authorship on Sumiran's repos:** `Sumiran Dahal <sumirandahal46@gmail.com>`, no `Co-Authored-By` trailer. The session's work email is not for commits.
- **OmaPhoenix stays private** until the user says otherwise. Ask before every push. Never tag a release without an explicit OK.
- **Never print secret values.** Neither tool ever prints a token, key or `.env` value. Findings name the *file*, never its content. Keep it that way in every output, log, test fixture and doc.
- **Real keys never leave the machine.** Tests use generated fake keys and throwaway passphrases. The OmaPhoenix VM test (`scripts/vm/`) uses a throwaway SSH key and test user.
- **The audit is read-only.** Integration code may *read* secaudit findings and *exclude* items from a snapshot. It must never apply a secaudit fix on its own; fixes need a human confirmation, as secaudit's own TUI already does.
- **No sudo prompts from unattended runs.** Timer snapshots run without a terminal, so they use secaudit's non-root quick pass and must never block on a password.

## Verification standard

- **Go (OmaPhoenix):** run `set -o pipefail` before any `go test … | …` chain. A pipe once hid a failing test and it got pushed.
- **Full check:** `gofmt -l .` is empty, then `go vet ./...` and `go test -count=1 ./...` pass. Then run `scripts/smoke.sh`, which takes about 12 minutes; run it in the background.
- **secaudit:** `bash -n bin/secaudit` and `shellcheck bin/secaudit`. Run `secaudit --json /tmp/x.jsonl` as the user, and check each line parses with `jq`.
- **Integration behaviour** is proven by a test that plants a fake secret or fake persistence entry in a temp `HOME` and shows that the snapshot excludes or blocks it. "It compiles" is not done.
- **Restores** are proven in the throwaway VM (`~/personal/omaphoenix/scripts/vm/vm.sh`), never on the user's real home.

## Style

- **OmaPhoenix:** match surrounding Go. Comments explain *why*. No UI code in `internal/core`.
- **secaudit:** match its Bash helpers (`ok`/`wn`/`bad`/`info`/`skp`, `detail`, `fix`, `hint`).
- **Docs:** short sentences, tables for facts. Update the roadmap status when a task changes state.
