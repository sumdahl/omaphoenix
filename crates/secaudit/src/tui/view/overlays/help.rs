//! The keys panel (`?`).

#![allow(
    clippy::too_many_lines,
    reason = "one function per screen, mirroring compose() in the original"
)]

use crate::tui::app::{App, Focus, Screen};
use crate::tui::view::put_lines;
use crate::tui::widgets::{self, put, wrap};
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::text::{Line, Span};

/// Textual's `HelpPanel`: the focused widget's keys, then the app's.
pub(crate) fn help_panel(buf: &mut Buffer, app: &App, r: Rect) {
    let p = &app.pal;
    for dy in 0..r.height {
        put(
            buf,
            r.x,
            r.y + dy,
            1,
            &Line::from(Span::styled("▏", p.fg_(p.panel))),
        );
    }
    let table: &[(&str, &str)] = &[
        ("↑", "Cursor up"),
        ("↓", "Cursor down"),
        ("←", "Cursor left"),
        ("→", "Cursor right"),
        ("home", "Home"),
        ("end", "End"),
        ("pgup", "Page up"),
        ("pgdn", "Page down"),
        ("^pgup", "Page Left"),
        ("^pgdn", "Page Right"),
        ("⏎", "Select"),
        ("^home", "Top"),
        ("^end", "Bottom"),
    ];
    let widget: &[(&str, &str)] = match (&app.screen, app.focus) {
        (Screen::Main, Focus::Search) => &[
            ("", "Input"),
            ("←", "Move cursor left"),
            ("→", "Move cursor right or accept the completion suggestion"),
            ("home ^a", "Go to start"),
            ("end ^e", "Go to end"),
            ("⌫", "Delete character left"),
            ("del ^d", "Delete character right"),
            ("⏎", "Submit"),
            ("^w", "Delete left to start of word"),
            ("^u super+⌫", "Delete all to the left"),
        ],
        (Screen::Main, Focus::Sections) => {
            &[("↑", "Cursor up"), ("↓", "Cursor down"), ("⏎", "Select")]
        }
        (Screen::Main, Focus::Tabs) => &[("←", "Previous tab"), ("→", "Next tab")],
        (Screen::Main, Focus::Detail) | (Screen::Log(..), _) => &[
            ("↑", "Scroll Up"),
            ("↓", "Scroll Down"),
            ("pgup", "Page Up"),
            ("pgdn", "Page Down"),
            ("home", "Scroll Home"),
            ("end", "Scroll End"),
        ],
        (Screen::Main, Focus::Welcome(_)) => &[("⏎", "Press button")],
        _ => table,
    };
    let common: &[(&str, &str)] = &[
        ("", ""),
        ("tab", "Focus Next"),
        ("shift+tab", "Focus Previous"),
        ("^c super+c", "Copy selected text"),
        ("", ""),
        (
            "^q q",
            "Quit Quit the app and return to the command prompt.",
        ),
    ];
    let app_keys: &[(&str, &str)] = match app.screen {
        Screen::Main => &[
            ("s", "Scan"),
            ("x", "Fix"),
            ("space", "Select"),
            ("a", "Accept risk"),
            ("i", "Explain (AI)"),
            ("I", "AI action plan"),
            ("/", "Search"),
            ("h", "History"),
            ("r", "Rescan"),
            ("*", "Select all fixable"),
            ("c", "Copy fix"),
            ("o", "Full output"),
            ("e", "Export report"),
            ("1", "Issues tab"),
            ("2", "Accepted tab"),
            ("3", "Notes tab"),
            ("4", "Passed tab"),
            ("5", "All tab"),
            ("esc", "Clear selection/search"),
            ("?", "Keys"),
        ],
        Screen::Scan(_) => &[("esc", "Cancel scan"), ("?", "Keys")],
        Screen::History(_) => &[
            ("esc", "Back"),
            ("⏎", "Open scan"),
            ("del d", "Delete scan"),
            ("?", "Keys"),
        ],
        Screen::Log(..) => &[("esc q", "Back"), ("?", "Keys")],
        Screen::Ai(_) => &[
            ("esc", "Back"),
            ("^r", "Regenerate"),
            ("^o", "Copy answer"),
            ("?", "Keys"),
        ],
    };
    let desc_w = usize::from(r.width.saturating_sub(15));
    let mut rows: Vec<Line> = Vec::new();
    for (k, d) in widget.iter().chain(common).chain(app_keys) {
        for (i, l) in wrap(d, desc_w).iter().enumerate() {
            let key = if i == 0 {
                format!("{k:>12}")
            } else {
                " ".repeat(12)
            };
            rows.push(Line::from(vec![
                Span::styled(key, p.bold(p.accent)),
                Span::raw(" "),
                Span::styled(l.clone(), p.fg_(p.fg)),
            ]));
        }
    }
    let h = r.height.saturating_sub(1);
    put_lines(buf, r.x + 1, r.y + 1, r.width - 1, h, &rows);
    if rows.len() > usize::from(h) {
        widgets::scrollbar(buf, p, r.right() - 2, r.y + 1, h, rows.len(), 0);
    }
}
