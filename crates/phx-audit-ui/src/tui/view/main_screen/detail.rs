//! The detail pane and its action buttons.

#![allow(
    clippy::too_many_lines,
    reason = "one function per screen, mirroring compose() in the original"
)]

use crate::tui::app::{App, Tab};
use crate::tui::times::local_time;
use crate::tui::view::{put_lines, w16, wrap_spans};
use crate::tui::widgets::{self, badge, pad, wrap};
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Modifier;
use ratatui::text::{Line, Span};
use phx_audit::store::{Run, sev_name};

/// Detail lines, the row of the action buttons, and each button's (x, width, id).
type DetailLines = (
    Vec<Line<'static>>,
    Option<usize>,
    Vec<(u16, u16, &'static str)>,
);

fn detail_lines(app: &App, run: &Run, width: usize) -> DetailLines {
    let p = &app.pal;
    let mut out: Vec<Line<'static>> = vec![Line::from("")];
    let Some(f) = app.selected() else {
        let (msg, sub) = if app.tab == Tab::Accepted && app.search.is_empty() {
            (
                Span::styled("No accepted risks", p.bold(p.fg)),
                "Press a on an issue you've decided to live with; it stops counting against your score.",
            )
        } else if app.tab == Tab::Issues && app.search.is_empty() {
            (
                Span::styled("✓ No issues here", p.bold(p.green)),
                "Nothing to fix in this view.",
            )
        } else {
            (
                Span::styled("No matching findings", p.bold(p.fg)),
                "Try another tab, section, or search.",
            )
        };
        out.extend(wrap_spans(&[msg], width));
        out.extend(wrap_spans(&[Span::styled(sub, p.fg_(p.text_muted))], width));
        return (out, None, Vec::new());
    };
    let fd = &f.finding;
    let sev = sev_name(fd.sev);
    out.extend(wrap_spans(
        &[
            badge(p, sev),
            Span::raw("  "),
            Span::styled(f.title().to_owned(), p.bold(p.fg)),
        ],
        width,
    ));
    let mut meta = vec![Span::styled(fd.section.clone(), p.fg_(p.muted))];
    if f.accepted {
        let since = app
            .accepted
            .get(&f.accept_key())
            .map(|a| a.at)
            .filter(|at| *at > 0)
            .map(|at| format!(" since {}", local_time(at, "%-d %b")))
            .unwrap_or_default();
        meta.push(Span::styled("  ·  ", p.fg_(p.muted)));
        meta.push(Span::styled(
            format!("accepted risk{since}, not scored"),
            p.fg_(p.muted).add_modifier(Modifier::ITALIC),
        ));
    } else if run.applied().contains(&f.n) {
        meta.push(Span::styled("  ·  ", p.fg_(p.muted)));
        meta.push(Span::styled(
            "✓ fix applied, rescan (r) to verify",
            p.fg_(p.green),
        ));
    }
    if sev == "pass" && fd.detail.is_empty() {
        meta.push(Span::styled("\nThis check passed.", p.fg_(p.muted)));
    }
    out.extend(wrap_spans(&meta, width));
    let actions_row;
    let mut buttons = Vec::new();
    // The actions row is always there now (it holds Explain).
    {
        out.push(Line::from(""));
        let mut spans = Vec::new();
        let mut x = 0u16;
        let mut add =
            |label: String, primary: bool, id: &'static str, spans: &mut Vec<Span<'static>>| {
                let w = w16(label.chars().count() + 4).max(14);
                spans.push(widgets::compact(p, &label, primary));
                spans.push(Span::raw("  "));
                buttons.push((x, w, id));
                x += w + 2;
            };
        if !fd.fix.is_empty() && !f.accepted {
            add("Run fix  x".into(), true, "btn-runfix", &mut spans);
        }
        if !fd.fix.is_empty() || !fd.hint.is_empty() {
            add(
                if fd.fix.is_empty() {
                    "Copy advice  c"
                } else {
                    "Copy fix  c"
                }
                .into(),
                false,
                "btn-copy",
                &mut spans,
            );
        }
        if f.is_issue() {
            add(
                if f.accepted {
                    "Un-accept  a"
                } else {
                    "Accept risk  a"
                }
                .into(),
                false,
                "btn-accept",
                &mut spans,
            );
        }
        if sev_name(fd.sev) != "pass" {
            add("Explain  i".into(), false, "btn-explain", &mut spans);
        }
        actions_row = Some(out.len());
        out.push(Line::from(spans));
        out.push(Line::from(""));
    }
    let cap = |t: &str| Line::from(Span::styled(t.to_owned(), p.bold(p.text_muted)));
    if !fd.detail.is_empty() {
        out.push(cap("DETAILS"));
        out.extend(
            wrap(&fd.detail, width)
                .into_iter()
                .map(|l| Line::from(Span::styled(l, p.fg_(p.fg)))),
        );
        out.push(Line::from(""));
    }
    if !fd.fix.is_empty() {
        out.push(cap("SUGGESTED FIX"));
        // Round border in $panel around a $boost box, text padded by 1.
        let inner = width.saturating_sub(2);
        let edge = p.fg_(p.panel);
        out.push(Line::from(Span::styled(
            format!("╭{}╮", "─".repeat(inner)),
            edge,
        )));
        for l in wrap(&fd.fix, inner.saturating_sub(2)) {
            out.push(Line::from(vec![
                Span::styled("│", edge.bg(p.boost)),
                Span::styled(pad(&format!(" {l}"), inner), p.fg_(p.accent).bg(p.boost)),
                Span::styled("│", edge.bg(p.boost)),
            ]));
        }
        out.push(Line::from(Span::styled(
            format!("╰{}╯", "─".repeat(inner)),
            edge,
        )));
        out.push(Line::from(""));
    }
    if !fd.hint.is_empty() {
        out.push(cap("HOW TO ADDRESS"));
        out.extend(
            wrap(&fd.hint, width)
                .into_iter()
                .map(|l| Line::from(Span::styled(l, p.fg_(p.fg).add_modifier(Modifier::ITALIC)))),
        );
        out.push(Line::from(""));
    }
    (out, actions_row, buttons)
}

pub(super) fn detail(buf: &mut Buffer, app: &App, r: Rect, run: &Run) {
    let p = &app.pal;
    let h = usize::from(r.height);
    // Textual decides at the narrower width: text that fits only without the scrollbar keeps it.
    let (narrow, narrow_arow, narrow_buttons) =
        detail_lines(app, run, usize::from(r.width.saturating_sub(2)));
    let scroll = narrow.len() > h;
    let (lines, arow, buttons) = if scroll {
        (narrow, narrow_arow, narrow_buttons)
    } else {
        detail_lines(app, run, usize::from(r.width))
    };
    let max = lines.len().saturating_sub(h);
    let off = app.detail_scroll.min(max);
    put_lines(
        buf,
        r.x,
        r.y,
        r.width,
        r.height,
        lines.get(off..).unwrap_or_default(),
    );
    if scroll {
        // Textual keeps it inside the right padding.
        widgets::scrollbar(buf, p, r.right() - 2, r.y, r.height, lines.len(), off);
    }
    if let Some(ar) = arow
        && ar >= off
        && ar - off < h
    {
        let y = r.y + w16(ar - off);
        let mut hits = app.hits.borrow_mut();
        for (bx, bw, id) in buttons {
            hits.buttons.push((r.x + bx, y, bw, id));
        }
    }
}
