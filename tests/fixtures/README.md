# Parity fixtures

Behaviour the Rust crates must reproduce, captured from the Go and Bash versions. Tests read these files, so regenerate them, never hand-edit `go/`.

- `go/`: written by the Go OmaPhoenix. Regenerate by copying `gen-go-fixtures.go.txt` to `cmd/fixturegen/main.go` in a scratch copy of the Go repo, then `go run ./cmd/fixturegen <this dir>/go`.
  - `manifest/`: `core.Manifest` as Go marshals it (format 1).
  - `selection/path-match.json`: Go's `path.Match` answers, including bad-pattern errors.
  - `selection/selected.json`: `Parse` + `Selected` + `Machine` over rule sets.
  - `selection/build.json`: `Build` output for TUI choices.
  - `snapshot.bundle`: a whole snapshot repo (format 1) from a FAKE home: two config files, a fake SSH key in an age vault sealed with the throwaway passphrase `fixture-passphrase-not-secret`. Regenerate with `scripts/gen-go-snapshot-fixture`, which refuses anything beyond the fake home. Its manifest names the generating machine (host, hardware model, user), as every manifest does. `crates/omaphoenix/tests/go_snapshot.rs` restores it.
- `secaudit/legacy.jsonl`: today's Bash `secaudit --json` lines (schema 0). Taken from a real run, with machine-specific lines replaced.
- `secaudit/v1.jsonl`: contract v1 (docs/04, ADR-0016).
