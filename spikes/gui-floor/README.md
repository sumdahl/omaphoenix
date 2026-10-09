# GUI floor tests (issue #19)

The smallest Pulse-like window (heading, status line, "Take snapshot" button) in each native candidate, to measure two hard requirements cheaply before a full spike:
- idle memory (ADR-0017)
- screen-reader visibility over AT-SPI (DESIGN.md §9)

```bash
(cd slint && cargo build --release)   # same for iced, gpui
python3 floor.py slint/target/release/floor-slint slint
```

AccessKit-based toolkits (Slint) only register on the accessibility bus when the system reports an assistive technology. Test with `org.a11y.Status IsEnabled` and `ScreenReaderEnabled` set to true via `busctl --user set-property org.a11y.Bus /org/a11y/bus org.a11y.Status …`, and restore them afterwards.

`gpui/vendor/xattr`: GPUI 0.2.2 pins `xattr` 0.2.3, which no longer builds against current `libc` (`ENOATTR` was removed; on Linux it is `ENODATA`). It is patched locally for this test only.

Results (2026-10-09, Dell Latitude 7430, Hyprland) are in `docs/09-gui-spike-findings.md`.
