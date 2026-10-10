# ADR-0019: The desktop stack is Slint

- **Status:** Accepted
- **Date:** 2026-10-10
- **Decided by:** Sumiran Dahal ("Slint, if memory usage is low"). Answers Q9 (#11).

## Context
docs/09 measured the candidates against DESIGN.md and ADR-0017's memory budget (150 MB target, 250 MB ceiling). Tauri met the UX bar at 234 MB; iced and GPUI had no accessibility; Slint met motion, theming and AT-SPI at 46 MB (spike) and 73 MB (the full `omaphoenix-gui`).

## Decision
Both desktop apps (OmaPhoenix, Security Audit) use Slint, on the condition the owner set: memory stays low. The condition is enforced, not remembered: each app's idle PSS is measured before a release and must stay under ADR-0017's 150 MB target.

## Consequences
- The Tauri spike (`crates/omaphoenix-desktop`, `ui/desktop`) is kept only as evidence; it is not shipped.
- Springs run in Rust and write properties per frame (Slint's `animate` keeps position, not velocity).
- Licensing follows ADR-0020.
