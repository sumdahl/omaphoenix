//! The scan chooser and the command palette.

#![allow(
    clippy::many_single_char_names,
    reason = "drawing code: p palette, r rect, x/y/w/h"
)]
use crate::tui::app::{App, CHOOSER};
use crate::tui::view::w16;
use crate::tui::widgets::{fill, put};
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};

use super::dialog;

pub(crate) fn chooser(buf: &mut Buffer, app: &App, full: Rect, sel: usize) {
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
pub(crate) fn palette(buf: &mut Buffer, app: &App, full: Rect, q: &str, sel: usize) {
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
