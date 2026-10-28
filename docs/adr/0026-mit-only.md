# ADR-0026: MIT only

- **Status:** Accepted
- **Date:** 2026-10-28
- **Decided by:** Sumiran Dahal and Manish Karki (Manish's approval of the PR that adds this ADR)

## Context
ADR-0015 licensed everything MIT OR Apache-2.0, the Rust-ecosystem default. OmaPhoenix is an end-user tool, not a library others build on, so the second license adds a file and a choice without helping anyone. Going public (ADR-0024) is the moment to keep it simple.

## Decision
OmaPhoenix is licensed under the [MIT License](https://opensource.org/license/MIT) alone: one `LICENSE` file, copyright Sumiran Dahal and Manish Karki, and `license = "MIT"` in every crate and app.

## Consequences
- Everything released so far was already offered under MIT, so no one who used it loses anything.
- `LICENSE-APACHE` is removed and `LICENSE-MIT` becomes `LICENSE`, which GitHub shows on the repo page.
- Dependencies are unaffected: `cargo deny` still allows Apache-2.0 and the other permissive licenses for them.
