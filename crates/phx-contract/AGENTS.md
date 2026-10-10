# phx-contract

The on-disk formats every snapshot is made of: `manifest.json` and `selection.conf`. Pure parsing and building, no I/O, no commands. The root AGENTS.md rules apply too.

## Rules
- **These are contracts, not internals.** Snapshots taken years ago, by the Go version too, must still read. Any change to a field, a rule or an escape is a format change: bump the format version, keep reading the old one, and write an ADR.
- **Go compatibility is pinned** by `tests/fixtures/go` and `crates/omaphoenix/tests/go_snapshot.rs`. Both must keep passing; never regenerate a fixture to make a test green.
- **Round-trips are property-tested** (`proptest`): parse∘build is identity for any id, including ones with `#`, spaces and globs. Add a case for every bug.
- **Hostile input.** A manifest comes from a git repo anyone could have edited. Reject absolute paths and `..`, bound sizes, and never panic on malformed lines.
