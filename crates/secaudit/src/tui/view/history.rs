//! The history screen and its comparison with the previous scan.

#![allow(
    clippy::many_single_char_names,
    reason = "drawing code: p palette, r rect, x/y/w/h"
)]
#![allow(
    clippy::too_many_lines,
    reason = "one function per screen, mirroring compose() in the original"
)]

use std::fmt::Write as _;

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Modifier;
use ratatui::text::{Line, Span};
use secaudit_core::store::{KEEP_RUNS, sev_name, sev_order};

use crate::tui::app::App;
use crate::tui::times::{ago, local_time};
use crate::tui::widgets::{self, badge, fill, pad, put};

use super::{hline, put_lines, scrolled, w16, wrap_spans};

pub(super) fn history(buf: &mut Buffer, app: &App, area: Rect, sel: usize) {
    let p = &app.pal;
    widgets::topbar(buf, p, area, &app.host, &app.user, "history");
    let footer_y = area.bottom().saturating_sub(1);
    let (x, y) = (area.x + 2, area.y + 2);
    let sum_x = area.right().saturating_sub(2 + 42);
    let trend_w = sum_x.saturating_sub(x);
    put(
        buf,
        x,
        y,
        trend_w,
        &Line::from(Span::styled(
            "SCORE TREND · oldest → newest",
            p.bold(p.text_muted),
        )),
    );
    let scores: Vec<u32> = app.runs.iter().rev().map(|r| r.counts().score()).collect();
    let data = if scores.is_empty() { vec![0] } else { scores };
    put_lines(
        buf,
        x,
        y + 2,
        trend_w,
        3,
        &widgets::sparkline(&data, usize::from(trend_w), 3, p.red, p.green),
    );
    if let Some(latest) = app.runs.first() {
        let best = app
            .runs
            .iter()
            .max_by_key(|r| r.counts().score())
            .unwrap_or(latest);
        let (lc, bc) = (latest.counts(), best.counts());
        let lines = [
            Line::from(vec![
                Span::styled("Latest  ", p.fg_(p.muted)),
                Span::styled(
                    format!("{} {}", lc.score(), lc.grade()),
                    p.bold(p.grade(lc.grade())),
                ),
            ]),
            Line::from(vec![
                Span::styled("Best    ", p.fg_(p.muted)),
                Span::styled(
                    format!("{} {}", bc.score(), bc.grade()),
                    p.bold(p.grade(bc.grade())),
                ),
                Span::styled(
                    format!("  {}", local_time(best.meta.started, "%-d %b")),
                    p.fg_(p.muted),
                ),
            ]),
            Line::from(vec![
                Span::styled("Scans   ", p.fg_(p.muted)),
                Span::styled(
                    format!("{} kept (max {KEEP_RUNS})", app.runs.len()),
                    p.fg_(p.fg),
                ),
            ]),
        ];
        put_lines(buf, sum_x + 3, y, 39, 3, &lines);
    }
    let diff = history_diff(app, sel, usize::from(area.width.saturating_sub(4)));
    let dh = w16(diff.len().min(14)) + 3;
    let dy = footer_y.saturating_sub(dh);
    hline(buf, p, area.x, dy, area.width);
    put_lines(
        buf,
        area.x + 2,
        dy + 2,
        area.width.saturating_sub(4),
        dh - 3,
        &diff,
    );
    // Table: margin 1 1 0 1.
    let t = Rect {
        x: area.x + 1,
        y: area.y + 8,
        width: area.width.saturating_sub(2),
        height: dy.saturating_sub(area.y + 8),
    };
    if t.height == 0 {
        return;
    }
    let cols = [
        (18, "When"),
        (9, ""),
        (6, "Mode"),
        (7, "Score"),
        (5, "Fail"),
        (5, "Warn"),
        (24, "Change vs previous"),
    ];
    let header = cols.iter().fold(String::new(), |mut h, (w, l)| {
        let _ = write!(h, " {} ", pad(l, *w));
        h
    });
    put(
        buf,
        t.x,
        t.y,
        t.width,
        &Line::from(Span::styled(
            pad(&header, usize::from(t.width)),
            p.bold(p.fg).bg(p.header_bg),
        )),
    );
    let body_h = usize::from(t.height - 1);
    let offset = scrolled(&app.hist_offset, sel, body_h, app.runs.len());
    for (i, r) in app.runs.iter().enumerate().skip(offset).take(body_h) {
        let c = r.counts();
        let change: Vec<Span> = match app.runs.get(i + 1) {
            None => vec![Span::styled("first scan", p.fg_(p.muted))],
            Some(prev) => {
                let (a, b) = (r.issue_keys(), prev.issue_keys());
                let (new, fixed) = (a.difference(&b).count(), b.difference(&a).count());
                vec![
                    Span::styled(
                        format!("+{new} new"),
                        p.fg_(if new > 0 { p.red } else { p.muted }),
                    ),
                    Span::raw("  "),
                    Span::styled(
                        format!("−{fixed} resolved"),
                        p.fg_(if fixed > 0 { p.green } else { p.muted }),
                    ),
                ]
            }
        };
        let cells: Vec<(Vec<Span>, usize)> = vec![
            (
                vec![Span::styled(
                    local_time(r.meta.started, "%a %-d %b %H:%M"),
                    p.fg_(p.fg),
                )],
                18,
            ),
            (vec![Span::styled(ago(r.meta.started), p.fg_(p.muted))], 9),
            (vec![Span::styled(r.meta.mode.clone(), p.fg_(p.fg))], 6),
            (
                vec![Span::styled(
                    format!("{:>3} {}", c.score(), c.grade()),
                    p.bold(p.grade(c.grade())),
                )],
                7,
            ),
            (
                vec![Span::styled(
                    c.fail.to_string(),
                    p.fg_(if c.fail > 0 { p.red } else { p.muted }),
                )],
                5,
            ),
            (
                vec![Span::styled(
                    c.warn.to_string(),
                    p.fg_(if c.warn > 0 { p.yellow } else { p.muted }),
                )],
                5,
            ),
            (change, 24),
        ];
        let mut spans = Vec::new();
        for (cell, w) in cells {
            spans.push(Span::raw(" "));
            let n: usize = cell.iter().map(|s| s.content.chars().count()).sum();
            spans.extend(cell);
            spans.push(Span::raw(" ".repeat(w.saturating_sub(n) + 1)));
        }
        let ry = t.y + 1 + w16(i - offset);
        if i == sel {
            let st = p.cursor();
            fill(
                buf,
                Rect {
                    x: t.x,
                    y: ry,
                    width: t.width,
                    height: 1,
                },
                st,
            );
            spans = spans
                .into_iter()
                .map(|s| Span::styled(s.content, st))
                .collect();
        }
        put(buf, t.x, ry, t.width, &Line::from(spans));
    }
    app.hits.borrow_mut().history = Some((t.y + 1, w16(body_h), offset));
}

/// `HistoryScreen.highlighted()`: what changed against the previous scan.
fn history_diff(app: &App, sel: usize, width: usize) -> Vec<Line<'static>> {
    let p = &app.pal;
    let Some(r) = app.runs.get(sel) else {
        return vec![Line::from(Span::styled("No scans yet.", p.fg_(p.muted)))];
    };
    let Some(prev) = app.runs.get(sel + 1) else {
        return vec![Line::from(Span::styled(
            "First scan on record; nothing to compare against.",
            p.fg_(p.muted),
        ))];
    };
    let (a, b) = (r.issue_keys(), prev.issue_keys());
    let mut new: Vec<_> = a.difference(&b).collect();
    let mut fixed: Vec<_> = b.difference(&a).collect();
    new.sort_by_key(|k| sev_order(k.0));
    fixed.sort_by_key(|k| sev_order(k.0));
    let mut t: Vec<Span> = vec![
        Span::styled(
            format!(
                "Compared with {} ({} scan)\n",
                local_time(prev.meta.started, "%a %-d %b %H:%M"),
                prev.meta.mode
            ),
            p.fg_(p.muted),
        ),
        Span::styled("New      ", p.bold(p.fg)),
        Span::styled(if new.is_empty() { "none\n" } else { "\n" }, p.fg_(p.muted)),
    ];
    for (sev, sec, msg) in new.iter().take(6) {
        t.push(Span::raw("  "));
        t.push(badge(p, sev_name(*sev)));
        t.push(Span::styled(format!("  {sec}: "), p.fg_(p.muted)));
        t.push(Span::styled(
            format!("{}\n", msg.trim_end_matches(':')),
            p.fg_(p.fg),
        ));
    }
    t.push(Span::styled("Resolved ", p.bold(p.fg)));
    t.push(Span::styled(
        if fixed.is_empty() { "none\n" } else { "\n" },
        p.fg_(p.muted),
    ));
    for (_, sec, msg) in fixed.iter().take(6) {
        t.push(Span::styled("  ✓ ", p.fg_(p.green)));
        t.push(Span::styled(format!("{sec}: "), p.fg_(p.muted)));
        t.push(Span::styled(
            format!("{}\n", msg.trim_end_matches(':')),
            p.fg_(p.fg),
        ));
    }
    if prev.meta.mode != r.meta.mode {
        t.push(Span::styled(
            "Different scan modes: some differences come from coverage, not changes.",
            p.fg_(p.muted).add_modifier(Modifier::ITALIC),
        ));
    }
    let mut lines = wrap_spans(&t, width);
    while lines.last().is_some_and(|l| l.width() == 0) {
        lines.pop();
    }
    lines
}
