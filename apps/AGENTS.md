# apps/: the Slint desktop apps

`omaphoenix-gui` and `omaphoenix-audit-gui` (ADR-0019, Slint under its royalty-free license, ADR-0020). Each is its own cargo workspace, excluded from the root one, with its own `Cargo.lock`. Run fmt, clippy and tests inside each app directory. The root AGENTS.md and DESIGN.md apply.

## Rules
- **The apps are front ends.** All logic lives in `phx-core` / `phx-audit`, or comes from running the CLI. When an app runs the CLI, it sets `OMAPHOENIX_NO_NOTIFY` and shows the result itself.
- **Never block the UI thread.** Work runs on a thread and reports back with `slint::invoke_from_event_loop`. Keep `Timer`s alive (store them), or they stop.
- **Long actions show progress:** the `ProgressStrip`, with a bar and the current step. Buttons that would start a second run are disabled while one runs. A run started elsewhere (the timer) is waited for, not reported as a failure.
- **Theme** comes from the live Omarchy theme and follows theme switches (DESIGN.md).
- **Memory:** idle ≤ 150 MB target, 250 MB ceiling (PSS, all processes).
- **Structure:** `main.rs` only builds the window and wires sections. Each section's callbacks live in their own module.

## Slint traps
- `word-wrap`'s minimum width is the longest word; use `char-wrap` for paths.
- Binding to `root.width` (or `if root.width …`) makes a binding loop. Use stretch layouts.
- A `ScrollView`'s content needs `width: sv.visible-width`.
- Never hold a `RefCell` borrow across a call that can re-enter (e.g. `toast()`): it panics.
- An element inside an `if` isn't reachable from Rust; keep it present and toggle `visible`.
- Set the Wayland app id (`set_xdg_app_id`) before creating the window.

## Screenshots and key tests
Use a headless Hyprland output (`hyprctl output create headless`), and send keys only to the test window by pid, never by class: the user's own copy may be running. Afterwards, always `hyprctl reload` and `hyprctl output remove`.
