//! Dialogs, chooser, command palette, keys panel and toasts.

#![allow(
    clippy::many_single_char_names,
    reason = "drawing code: p palette, r rect, x/y/w/h"
)]
#![allow(
    clippy::too_many_lines,
    reason = "one function per screen, mirroring compose() in the original"
)]

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};

use crate::tui::app::{App, CHOOSER, Focus, Screen, Severity};
use crate::tui::theme::Pal;
use crate::tui::widgets::{self, fill, pad, put, wrap};

use super::{put_lines, rounded, w16, wrap_spans};

/// `#dialog`: width 84, round accent border, padding 1 2, on `$surface`.
pub(super) fn dialog(buf: &mut Buffer, p: &Pal, full: Rect, content_h: u16) -> Rect {
    let w = 84.min(full.width);
    let h = (content_h + 4).min(full.height);
    let r = Rect {
        x: full.x + (full.width - w) / 2,
        y: full.y + (full.height - h) / 2,
        width: w,
        height: h,
    };
    rounded(buf, p, r, p.accent, Some(p.surface));
    Rect {
        x: r.x + 3,
        y: r.y + 2,
        width: w.saturating_sub(6),
        height: h.saturating_sub(4),
    }
}

pub(super) fn confirm(buf: &mut Buffer, app: &App, full: Rect, c: &crate::tui::app::Confirm) {
    let p = &app.pal;
    let width = 78.min(usize::from(full.width.saturating_sub(6)));
    let mut lines = wrap_spans(&[Span::styled(c.title.clone(), p.bold(p.fg))], width);
    lines.push(Line::from(""));
    if !c.code.is_empty() {
        let mut code = Vec::new();
        for l in &c.code {
            let st = if l.starts_with("$ ") {
                p.fg_(p.accent)
            } else {
                p.bold(p.fg)
            };
            for wl in wrap(l, width.saturating_sub(2)) {
                code.push(Line::from(Span::styled(
                    pad(&format!(" {wl}"), width),
                    st.bg(p.boost),
                )));
            }
        }
        code.truncate(16);
        lines.extend(code);
    }
    if !c.note.is_empty() {
        lines.push(Line::from(""));
        lines.extend(wrap_spans(
            &[Span::styled(c.note.clone(), p.fg_(p.muted_on(p.surface)))],
            width,
        ));
    }
    let inner = dialog(buf, p, full, w16(lines.len()) + 4);
    put_lines(buf, inner.x, inner.y, inner.width, inner.height, &lines);
    let by = inner.y + w16(lines.len()) + 1;
    let ok = format!("{}  y", c.ok);
    let (w1, w2) = (16u16, w16(ok.chars().count() + 4).max(16));
    let x2 = inner.right().saturating_sub(w2);
    let x1 = x2.saturating_sub(1 + w1);
    widgets::button(
        buf,
        p,
        Rect {
            x: x1,
            y: by,
            width: w1,
            height: 3,
        },
        "Cancel  n",
        false,
        false,
        !c.on_ok,
    );
    widgets::button(
        buf,
        p,
        Rect {
            x: x2,
            y: by,
            width: w2,
            height: 3,
        },
        &ok,
        !c.danger,
        c.danger,
        c.on_ok,
    );
    let mut hits = app.hits.borrow_mut();
    for dy in 0..3 {
        hits.buttons.push((x1, by + dy, w1, "no"));
        hits.buttons.push((x2, by + dy, w2, "yes"));
    }
}

pub(super) fn chooser(buf: &mut Buffer, app: &App, full: Rect, sel: usize) {
    let p = &app.pal;
    let inner = dialog(buf, p, full, 2 + 6 + 2);
    put(
        buf,
        inner.x,
        inner.y,
        inner.width,
        &Line::from(Span::styled("Start a scan", p.bold(p.fg))),
    );
    let mut y = inner.y + 2;
    for (i, (_, name, key, desc)) in CHOOSER.iter().enumerate() {
        // The focused list's highlight: accent background, cursor text.
        let on = i == sel;
        let (name_st, key_st, desc_st) = if on {
            (
                p.cursor(),
                p.fg_(p.accent).bg(p.accent).add_modifier(Modifier::BOLD),
                p.bold(p.muted).bg(p.accent),
            )
        } else {
            (p.bold(p.fg), p.fg_(p.accent), p.fg_(p.muted))
        };
        if on {
            fill(
                buf,
                Rect {
                    x: inner.x + 1,
                    y,
                    width: inner.width.saturating_sub(2),
                    height: 2,
                },
                p.cursor(),
            );
        }
        put(
            buf,
            inner.x + 2,
            y,
            inner.width - 3,
            &Line::from(vec![
                Span::styled(*name, name_st),
                Span::styled(format!("  {key}"), key_st),
            ]),
        );
        put(
            buf,
            inner.x + 2,
            y + 1,
            inner.width - 3,
            &Line::from(Span::styled(*desc, desc_st)),
        );
        app.hits.borrow_mut().options.push((y, 2, i));
        y += 2;
    }
    put(
        buf,
        inner.x,
        y + 1,
        inner.width,
        &Line::from(Span::styled(
            "All scans are read-only. sudo is only used to read root-owned files.",
            p.fg_(p.muted_on(p.surface)),
        )),
    );
}

/// Textual's command palette.
pub(super) fn palette(buf: &mut Buffer, app: &App, full: Rect, q: &str, sel: usize) {
    let p = &app.pal;
    let cmds = app.commands(q);
    let y0 = full.y + 3;
    let max_rows = usize::from(full.height.saturating_sub(y0 + 7)) / 2;
    let shown = cmds.len().min(max_rows);
    let h = w16(shown * 2) + 6;
    let r = Rect {
        x: full.x,
        y: y0,
        width: full.width,
        height: h.min(full.height.saturating_sub(y0)),
    };
    fill(buf, r, Style::new().bg(p.surface).fg(p.fg));
    let w = usize::from(r.width);
    put(
        buf,
        r.x,
        r.y,
        r.width,
        &Line::from(Span::styled("▔".repeat(w), p.fg_(p.accent).bg(p.surface))),
    );
    let prompt = if q.is_empty() {
        Span::styled("Search for commands…", p.fg_(p.muted))
    } else {
        Span::styled(format!("{q}▏"), p.fg_(p.fg))
    };
    put(
        buf,
        r.x + 2,
        r.y + 2,
        r.width.saturating_sub(4),
        &Line::from(vec![Span::raw("🔎 "), prompt]),
    );
    let mut y = r.y + 5;
    let start = sel.saturating_sub(shown.saturating_sub(1));
    for (i, (name, help)) in cmds.iter().enumerate().skip(start).take(shown) {
        let bg = if i == sel { p.highlight } else { p.surface };
        fill(
            buf,
            Rect {
                x: r.x,
                y,
                width: r.width,
                height: 2,
            },
            Style::new().bg(bg),
        );
        put(
            buf,
            r.x + 2,
            y,
            r.width.saturating_sub(4),
            &Line::from(Span::styled(*name, p.bold(p.fg).bg(bg))),
        );
        put(
            buf,
            r.x + 2,
            y + 1,
            r.width.saturating_sub(4),
            &Line::from(Span::styled(*help, p.fg_(p.muted).bg(bg))),
        );
        y += 2;
    }
    if r.bottom() > 0 {
        put(
            buf,
            r.x,
            r.bottom() - 1,
            r.width,
            &Line::from(Span::styled("▁".repeat(w), p.fg_(p.accent).bg(p.bg))),
        );
    }
}

/// Textual's `HelpPanel`: the focused widget's keys, then the app's.
pub(super) fn help_panel(buf: &mut Buffer, app: &App, r: Rect) {
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

pub(super) fn toasts(buf: &mut Buffer, app: &App, full: Rect) {
    let p = &app.pal;
    let mut bottom = full.bottom().saturating_sub(1);
    for t in app.toasts.iter().rev() {
        let edge = match t.severity {
            Severity::Information => p.green,
            Severity::Warning => p.yellow,
            Severity::Error => p.red,
        };
        let h = widgets::toast(
            buf,
            p,
            Rect {
                height: bottom.saturating_sub(full.y),
                ..full
            },
            t.title.as_deref(),
            &t.body,
            edge,
        );
        bottom = bottom.saturating_sub(h + 1);
        if bottom <= full.y + h {
            break;
        }
    }
}
