# 07: Decisions and open questions

**Decisions** are architecture decision records in [adr/](adr/README.md), one file each (ADR-0001 to ADR-0014). The old D-numbers map one to one: D1 is ADR-0001, and so on.

## Open questions: need the owners

Agents must not decide these. Ask the user.

| # | Question | Who | Notes |
|---|---|---|---|
| Q1 | **License** for all three crates, and **Manish's written OK** to rewrite his checks. **Blocks porting his checks** (P4.0) | both | secaudit has no LICENSE. Proposal: MIT or Apache-2.0 (dual, Rust-ecosystem standard) |
| Q2 | **Visibility and org** for the joint repo (currently private under `sumdahl`, ADR-0014) | both | |
| Q4 | **Finding contract v1** fields and ID naming scheme ([04](04-integration-design.md)) | Manish | Proposal ready |
| Q5 | **Default gate policy:** `exclude` failures and `warn` warnings, or `block` on any failure? | both | Proposal: exclude/warn |
| Q7 | **Handover mechanics:** when, crate publishing, who maintains `secaudit-core` releases | both | Proposal in ADR-0013 |
| Q9 | **Rust GUI stack** for the desktop app ([06](06-rust-rewrite.md#desktop-app)) | both | Proposal: Tauri 2 after a 2-week spike; GPUI as the fallback |

Resolved: Q3 (by ADR-0012), Q6 (moot by ADR-0011), Q8 (by ADR-0010).
