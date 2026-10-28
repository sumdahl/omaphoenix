# ADR-0024: Security Audit folds into OmaPhoenix, and the repo goes public

- **Status:** Accepted
- **Date:** 2026-10-27
- **Decided by:** Sumiran Dahal, with Manish Karki's agreement. Supersedes ADR-0013, ADR-0022 and ADR-0023.

## Context
ADR-0013 planned a standalone Security Audit handed to Manish, and ADR-0022 kept the repo private. For the launch, one product with one name is easier to explain and install, and an open-source tool that handles SSH keys has to be readable by the people who run it.

## Decision
- **One product, one binary:** `secaudit`, `secaudit-tui` and `secaudit-ui` became `omaphoenix audit`, `audit tui` and `audit menu` in 0.1.0; the crates are `phx-audit` and `phx-audit-ui`. The README credits "Security Audit by Manish Karki".
- **The repo is `sumdahl/omaphoenix` and goes public.** Before that, its history was rewritten into a new repo to remove a work hostname and key name, and commit trailers the authorship rule forbids. The old repo is kept as the private `sumdahl/omaphoenix-archive` and must never be made public: its pull requests still hold the old commits. The Go version moved to `sumdahl/omaphoenix-go`.
- **Release order:** public first, then the `v0.1.0` tag. The release workflow attests build provenance only on a public repo, and a release without it can't be attested later.

## Consequences
- Existing installs upgrade through `install.sh`, which removes the old `secaudit` commands; scan history moves from `secaudit/` to `omaphoenix-audit/` on first use (docs/13).
- The handover in docs/12 no longer happens; Manish keeps co-ownership of the audit code here (CODEOWNERS).
- Old PR numbers refer to the archive; issues moved with new numbers from #20.
