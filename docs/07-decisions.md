# 07: Decisions and open questions

**Decisions** are architecture decision records in [adr/](adr/README.md), one file each (ADR-0001 to ADR-0017). The old D-numbers map one to one: D1 is ADR-0001, and so on.

## Open questions: need the owners

Agents must not decide these. Ask the user.

| # | Question | Who | Notes |
|---|---|---|---|
| Q2 ([#17](https://github.com/sumdahl/omaphoenix-secaudit/issues/17)) | **Visibility and org** for the joint repo (currently private under `sumdahl`, ADR-0014) | both | |
| Q5 ([#8](https://github.com/sumdahl/omaphoenix-secaudit/issues/8)) | **Default gate policy:** `exclude` failures and `warn` warnings, or `block` on any failure? | both | Proposal: exclude/warn. **Built that way** (`phx-core::gate`, 2026-10-10); changing it is one match arm |
| Q7 ([#16](https://github.com/sumdahl/omaphoenix-secaudit/issues/16)) | **Handover mechanics:** when, crate publishing, who maintains `secaudit-core` releases | both | Proposal in ADR-0013 |
| Q9 ([#11](https://github.com/sumdahl/omaphoenix-secaudit/issues/11)) | **Rust GUI stack** for the desktop app ([06](06-rust-rewrite.md#desktop-app)) | both | Measured (docs/09): Tauri 234 MB, Slint 46 MB; Slint meets motion, theme and a11y. **Proposal now: Slint**, if Q10 is acceptable. A Slint app is built in `apps/omaphoenix-gui`, outside the workspace until decided |
| Q10 | **Slint's license.** Slint is `GPL-3.0` or the Slint Royalty-free 2.0 license (free for desktop apps; requires a visible "Made with Slint" attribution) or commercial. Our code stays MIT/Apache (ADR-0015); the shipped GUI binary would include Slint under Royalty-free. Acceptable? | both | If not: Tauri (234 MB, over the 150 MB target) or no desktop app yet. `cargo deny` blocks Slint until this is decided |

Resolved: Q1 (by ADR-0015), Q4 (by ADR-0016), Q3 (by ADR-0012), Q6 (moot by ADR-0011), Q8 (by ADR-0010).
