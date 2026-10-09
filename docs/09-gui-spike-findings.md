# 09: GUI-stack spike findings (issue #12 → decision #11, Q9)

**Date:** 2026-10-09. **Machine:** Dell Latitude 7430, Omarchy 4.0.4, Hyprland, eDP-1 1920×1080 **@ 60 Hz**, scale 2.
**Stack tried:** Tauri 2 (Rust backend) + Svelte 5 / TypeScript (web UI on WebKitGTK 4.1, webkit2gtk 2.52).
**Built:** Pulse (score ring, timeline, snapshot action) and the Migrate "Preview" sheet, to DESIGN.md.
- **Code:** `crates/omaphoenix-desktop`, `ui/desktop`
- **Harness:** `scripts/spike-bench.sh` and `scripts/spike-a11y.py`

## Results

| DESIGN.md requirement | Measured | Verdict |
|---|---|---|
| Smooth frames (§6.5) | 630 frames under scripted stress: median 17 ms, p99 17 ms, max 19 ms, **0 dropped** | ✅ at 60 Hz |
| 120 Hz budget (8.3 ms) | Spring work per frame **≤ 1 ms**. Paint/composite time isn't observable from JS; this display is 60 Hz | ⚠ **not verified on a 120 Hz panel** |
| Interruptible springs, velocity kept (§6.1) | Largest single-frame step of the rendered value: ring 14 % (two retargets mid-flight), sheet 10 % (closed while opening), timeline row 7 % (3 snapshots 80 ms apart) | ✅ no jumps |
| Velocity hand-off on drag release (§6.3) | Implemented (`Spring.fling`), exercised by hand only | ⚠ no automated measure |
| Live Omarchy theme switch (§5.1, §10) | Rust watcher picks up the swap; tokens re-applied with a 200 ms cross-fade; accent verified equal to the new theme's | ✅ |
| User's Omarchy font (§5.2) | `omarchy-font-current` → CSS (JetBrains Mono on this machine) | ✅ |
| Reduced motion (§6.1 law 6) | `hyprctl animations:enabled` + `prefers-reduced-motion` → springs settle instantly, fades ≤ 120 ms | ✅ implemented; not bench-measured |
| Screen reader / AT-SPI (§9) | Buttons named ("Pulse", "Take snapshot"), shortcuts via `aria-keyshortcuts` (not read into names), 3 headings, score as a progress bar with value, landmarks, list items, live region | ✅ 5/5 checks |
| Startup and size | Release binary 6.2 MB; UI JS 52 KB (20 KB gzip) | ✅ |
| **Memory** | **≈ 400 MB** (app + WebKit network + web processes, PSS) for an idle window | ❌ heavy for a utility |
| Supply chain (RUST.md §7) | Clean after work. Upgrading fixed 2 vulnerabilities (`quick-xml`, `time`), which forced MSRV 1.88. MPL-2.0 (Servo `cssparser`) allowed with a reason. **6 unmaintained-crate notices from Tauri's Linux stack**: gtk-rs 0.18 GTK3 bindings (end of life upstream) and `unic-*` via `urlpattern` | ⚠ long-term risk |

## What we learned building it
- **The web stack makes DESIGN.md fast to hit.** Springs, FLIP-style transitions, theme tokens as CSS variables and accessibility semantics are all mature. Accessibility came almost free from semantic HTML, and WebKitGTK exposes it over AT-SPI.
- **Accessible names need care.** Visible shortcut hints ("1", "s") were read into button names until marked `aria-hidden` and exposed via `aria-keyshortcuts`.
- **Honesty rule:** the spike headline said "You're protected." while the score showed F. The real app must derive every headline from state (DESIGN.md §1–2).
- **Measurement traps hit while building the harness**, all fixed:
  - pipe `SIGPIPE` and `xargs`/`grep -L` exit codes under `pipefail` killed scripts silently
  - a FLIP list's own offset jumps by design, so continuity must be measured on what the eye follows, not on the internal transform
  - report fields must be written to the file the checks read

## Recommendation for Q9 (#11)

**Tauri 2 meets DESIGN.md's experience requirements today**: smooth, interruptible motion, live theming, and real accessibility. Its costs are **≈ 400 MB of memory** and **dependence on end-of-life GTK3 bindings on Linux**.

**Proposal:**
1. **Keep Tauri 2 as the desktop stack** for the first release, because it's the only option that met every UX and accessibility requirement in this spike.
2. **Before committing, run the same two screens on GPUI for one week,** measuring memory, AT-SPI and the same bench, so the trade-off is made on numbers, not reputation. GPUI is likely far lighter but weaker on accessibility; if its AT-SPI story doesn't pass §9, Tauri stands.
3. **Verify the 120 Hz budget** on a high-refresh display before release.
4. **Track Tauri's GTK4/WebKitGTK 6 migration;** the `deny.toml` ignores are tied to it.

The final call is the owners' (Q9 is "decision needed").
