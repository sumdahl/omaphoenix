//! The findings table.

#![allow(
    clippy::too_many_lines,
    reason = "one function per screen, mirroring compose() in the original"
)]

use crate::tui::app::{App, Focus};
use crate::tui::view::{HELP_W, scrolled, w16};
use crate::tui::widgets::{self, badge, clip, fill, pad, put};
use phx_audit::store::{Item, Run, sev_name};
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Modifier;
use ratatui::text::{Line, Span};
use std::fmt::Write as _;

/// The findings `DataTable`.
pub(super) fn table(buf: &mut Buffer, app: &App, r: Rect, rows: &[&Item]) {
    let p = &app.pal;
    if r.height == 0 {
        return;
    }
    // Columns size on resize only (`on_resize`), so the keys panel narrows the table without re-sizing them.
    let avail = usize::from((r.width + if app.help { HELP_W } else { 0 }).max(60));
    let sec_w = if app.section.is_some() { 0 } else { 22 };
    let ncols = if sec_w > 0 { 5 } else { 4 };
    let msg_w = 24.max(avail.saturating_sub(1 + 7 + sec_w + 8 + ncols * 2 + 2));
    let mut cols = vec![(1, ""), (7, "")];
    if sec_w > 0 {
        cols.push((sec_w, "Section"));
    }
    cols.push((msg_w, "Finding"));
    cols.push((8, "Fix"));
    let content_w: usize = cols.iter().map(|c| c.0 + 2).sum();
    let mut body_h = usize::from(r.height) - 1;
    let vscroll = rows.len() > body_h;
    let view_w = usize::from(r.width) - if vscroll { 2 } else { 0 };
    let hscroll = content_w > view_w && body_h > 1;
    if hscroll {
        body_h -= 1;
    }
    let header = cols.iter().fold(String::new(), |mut h, (w, l)| {
        let _ = write!(h, " {} ", pad(l, *w));
        h
    });
    let focused = app.focus == Focus::Table;
    let hs = p.bold(p.fg).bg(if focused { p.header_bg } else { p.panel });
    put(
        buf,
        r.x,
        r.y,
        w16(view_w),
        &Line::from(Span::styled(pad(&header, view_w), hs)),
    );
    let offset = scrolled(&app.row_offset, app.row, body_h, rows.len());
    let applied = app.run().map(Run::applied).unwrap_or_default();
    for (i, f) in rows.iter().enumerate().skip(offset).take(body_h) {
        let y = r.y + 1 + w16(i - offset);
        let fd = &f.finding;
        let sev = sev_name(fd.sev);
        let fix = if f.accepted {
            Span::styled("accepted", p.fg_(p.muted))
        } else if applied.contains(&f.n) {
            Span::styled("✓ done", p.fg_(p.green))
        } else if !fd.fix.is_empty() {
            Span::styled("● fix", p.fg_(p.accent))
        } else if !fd.hint.is_empty() && f.is_issue() {
            Span::styled("advice", p.fg_(p.muted))
        } else {
            Span::raw("")
        };
        let mark = if app.marked.contains(&f.n) {
            Span::styled("◉", p.bold(p.accent))
        } else {
            Span::raw("")
        };
        let mut b = badge(p, sev);
        if f.accepted {
            b.style = p.fg_(p.muted).add_modifier(Modifier::DIM | Modifier::BOLD);
        }
        let mut cells = vec![(mark, 1), (b, 7)];
        if sec_w > 0 {
            cells.push((
                Span::styled(clip(&fd.section, sec_w), p.fg_(p.muted)),
                sec_w,
            ));
        }
        cells.push((
            Span::styled(
                clip(f.title(), msg_w),
                if f.accepted {
                    p.fg_(p.muted)
                } else {
                    p.fg_(p.fg)
                },
            ),
            msg_w,
        ));
        cells.push((fix, 8));
        let mut spans = Vec::new();
        for (s, w) in cells {
            spans.push(Span::raw(" "));
            spans.push(Span::styled(pad(&s.content, w), s.style));
            spans.push(Span::raw(" "));
        }
        if i == app.row {
            // Blurred cursor: plain text colour on the highlight.
            let st = if focused {
                p.cursor()
            } else {
                p.fg_(p.fg).bg(p.highlight)
            };
            spans = spans
                .into_iter()
                .map(|s| {
                    let keep_bold = s.style.add_modifier.contains(Modifier::BOLD);
                    let blurred = if keep_bold {
                        st.add_modifier(Modifier::BOLD)
                    } else {
                        st
                    };
                    Span::styled(s.content, if focused { st } else { blurred })
                })
                .collect();
            fill(
                buf,
                Rect {
                    x: r.x,
                    y,
                    width: w16(view_w),
                    height: 1,
                },
                st,
            );
        }
        put(buf, r.x, y, w16(view_w), &Line::from(spans));
    }
    if vscroll {
        widgets::scrollbar(
            buf,
            p,
            r.right() - 2,
            r.y,
            r.height - u16::from(hscroll),
            rows.len() + 1,
            offset,
        );
    }
    if hscroll {
        widgets::hscrollbar(buf, p, r.x, r.y + 1 + w16(body_h), w16(view_w), content_w);
    }
    app.hits.borrow_mut().table = Some((r.x, r.y + 1, w16(body_h), offset));
}
