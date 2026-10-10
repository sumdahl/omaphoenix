# ADR-0020: Slint under its Royalty-free license

- **Status:** Accepted
- **Date:** 2026-10-10
- **Decided by:** Sumiran Dahal ("yes we can"). Answers Q10.

## Context
Slint is offered under GPL-3.0, the Slint Royalty-free Desktop, Mobile, and Web Applications License 2.0, or a commercial license. Our own code is MIT OR Apache-2.0 (ADR-0015).

## Decision
The desktop binaries ship Slint under the Royalty-free 2.0 license. Its one obligation, a visible "Made with Slint" attribution, is met in each app's Settings/About area. Our source stays MIT OR Apache-2.0.

## Consequences
- The GUI apps stay in `apps/`, outside the workspace, so the workspace's `cargo deny` license policy stays strict for everything else; CI builds and lints them separately.
- Anyone forking the GUI inherits the same choice (or GPL-3.0).
