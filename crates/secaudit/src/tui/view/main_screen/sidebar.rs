//! The sections sidebar.

#![allow(
    clippy::many_single_char_names,
    reason = "drawing code: p palette, r rect, x/y/w/h"
)]
#![allow(
    clippy::too_many_lines,
    reason = "one function per screen, mirroring compose() in the original"
)]

use crate::tui::app::{App, Focus};
use crate::tui::view::{hline, scrolled, w16};
use crate::tui::widgets::{self, clip, fill, pad, put};
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use secaudit_core::store::{Run, sev_name};

pub(super) fn sidebar(buf: &mut Buffer, app: &App, r: Rect, run: &Run, narrow: bool) {
    let p = &app.pal;
    // padding: 1 1 0 1
    let (x, w) = (r.x + 1, r.width.saturating_sub(2));
    put(
        buf,
        x,
        r.y + 1,
        w,
        &Line::from(Span::styled("SECTIONS", p.bold(p.text_muted))),
    );
    let list_top = r.y + 3;
    let mut list_bottom = r.bottom();
    if !narrow && r.height > 16 {
        // border, padding 1 1 1 1, seven tips
        let ty = r.bottom() - 10;
        hline(buf, p, x, ty, w);
        let keys = [
            ("space", "select"),
            ("x", "fix selected"),
            ("*", "select all fixable"),
            ("a", "accept risk"),
            ("i", "explain with AI"),
            ("I", "AI action plan"),
            ("r", "rescan"),
        ];
        for (i, (k, what)) in keys.iter().enumerate() {
            let l = Line::from(vec![
                Span::styled(format!("{k:>5} "), p.bold(p.accent)),
                Span::styled((*what).to_owned(), p.fg_(p.muted)),
            ]);
            put(buf, x + 1, ty + 2 + w16(i), w.saturating_sub(1), &l);
        }
        list_bottom = ty;
    }
    let lh = usize::from(list_bottom.saturating_sub(list_top));
    let sections = app.sections();
    let total = sections.len() + 1;
    let scroll = total > lh;
    let text_w = usize::from(w).saturating_sub(2 + if scroll { 2 } else { 0 });
    let label_w = usize::from(if narrow { 26u16 } else { 32 }) - 1 - 2 - 2 - 1 - 2 - 3;
    let offset = scrolled(&app.sec_offset, app.section_cursor, lh, total);
    let c = run.counts();
    for row in 0..lh.min(total - offset) {
        let i = offset + row;
        let y = list_top + w16(row);
        let mut spans: Vec<Span> = if i == 0 {
            vec![
                Span::styled("◆ ", p.fg_(p.accent)),
                Span::styled(pad("All sections", label_w), p.bold(p.fg)),
                Span::styled(format!("{:>3}", c.fail + c.warn), p.fg_(p.muted)),
            ]
        } else {
            let sec = sections.get(i - 1).map_or("", String::as_str);
            let in_sec = |sev: &str| {
                run.items
                    .iter()
                    .filter(|f| {
                        f.finding.section == sec && sev_name(f.finding.sev) == sev && !f.accepted
                    })
                    .count()
            };
            let (fails, warns) = (in_sec("fail"), in_sec("warn"));
            let (icon, col) = if fails > 0 {
                ("✗", p.red)
            } else if warns > 0 {
                ("!", p.yellow)
            } else {
                ("✓", p.green)
            };
            let n = fails + warns;
            vec![
                Span::styled(format!("{icon} "), p.bold(col)),
                Span::styled(pad(&clip(sec, label_w), label_w), p.fg_(p.fg)),
                Span::styled(
                    if n > 0 {
                        format!("{n:>3}")
                    } else {
                        "   ".into()
                    },
                    p.fg_(col),
                ),
            ]
        };
        // text-overflow: ellipsis at the item's width.
        let text: String = spans.iter().map(|s| s.content.as_ref()).collect();
        if text.chars().count() > text_w {
            let mut left = text_w.saturating_sub(1);
            let mut cut = Vec::new();
            for s in spans {
                let n = s.content.chars().count();
                if left == 0 {
                    break;
                }
                let take: String = s.content.chars().take(left).collect();
                left -= n.min(left);
                cut.push(Span::styled(take, s.style));
            }
            cut.push(Span::styled("…", p.fg_(p.fg)));
            spans = cut;
        }
        if i == app.section_cursor {
            let bg = if app.focus == Focus::Sections {
                p.accent
            } else {
                p.highlight
            };
            fill(
                buf,
                Rect {
                    x,
                    y,
                    width: w16(text_w + 2),
                    height: 1,
                },
                Style::new().bg(bg),
            );
            spans = spans
                .into_iter()
                .map(|s| Span::styled(s.content, s.style.bg(bg)))
                .collect();
            if app.focus == Focus::Sections {
                spans = spans
                    .into_iter()
                    .map(|s| Span::styled(s.content, p.cursor()))
                    .collect();
            }
        }
        put(buf, x + 1, y, w16(text_w), &Line::from(spans));
    }
    if scroll {
        widgets::scrollbar(buf, p, x + w - 2, list_top, w16(lh), total, offset);
    }
    app.hits.borrow_mut().sections = Some((x, list_top, w16(lh), offset));
}
