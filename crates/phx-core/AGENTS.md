# phx-core

The OmaPhoenix engine: detect, capture and restore an Omarchy setup through a git-backed snapshot store, with the Security Audit gate in front (ADR-0001, ADR-0021). The CLI, TUI and desktop app are thin layers over it. The root AGENTS.md rules apply too.

## Rules
- **No UI here.** Never print, prompt or draw. Report through the `log` callback and progress callbacks, and return data. Ask for a passphrase only through `App::passphrase`.
- **Every command goes through `sys::command`.** `std::env::set_var` is `unsafe` in edition 2024 and unsafe is forbidden, so variables for child processes go through `sys::set_child_env`.
- **One run at a time.** The repo lock (`File::try_lock`) covers snapshot, restore and undo. The `Locked` message tells the user to wait, and callers don't treat it as a failure to notify about.
- **Snapshots commit only on change** and write only what changed: the previous tree moves to `.git/omaphoenix-prev`, and unchanged files (same size and mtime) are renamed back instead of copied. Copies keep the source mtime, otherwise every file looks changed next time.
- **Junk never gets captured.** Caches, logs and files a program rewrites while it runs (Spotify `*.bnk`) go in `files::JUNK`. Add to it when a snapshot commits churn nobody chose.
- **Restores never block.** Every external install is watched; slow or stalled ones are deferred and reported (docs/08). Restores go through Omarchy's own commands (`omarchy-pkg-add`, …), in stage order, and every stage reports its own failures.
- **Undo never deletes.** What a restore created goes to the undo trash; replaced files get their previous version back; a file edited after the restore is left alone.
- **Secrets** are age-encrypted with a passphrase that is never stored. Plaintext stays in memory (`Zeroizing`).
- **Memory budget:** `snapshot` ≤ 48 MB own peak RSS (RUST.md §9). Stream files; ask `pacman -Qo` about candidates, never load every owned path.

## Testing
- Unit tests use a temp `HOME` and a temp repo. Never touch the real home: restores run only in a sandbox or the VM.
- `scripts/omaphoenix-parity` snapshots this machine with the Go and Rust binaries and diffs the trees, then cross-restores into sandboxes. It never writes to the real home and never pushes. Record deliberate differences in docs/11.
