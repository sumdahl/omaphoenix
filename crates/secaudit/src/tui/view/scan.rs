//! The scan progress screen.

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
use ratatui::style::Modifier;
use ratatui::text::{Line, Span};

use crate::tui::app::App;
use crate::tui::scan::{LogLine, RowState, Scan};
use crate::tui::widgets::{self, badge, put};

use super::{mmss, put_lines, rounded, w16, wrap_spans};

const FRAMES: [&str; 10] = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];

pub(super) fn scan_screen(buf: &mut Buffer, app: &App, area: Rect, scan: &Scan) {
    let p = &app.pal;
    let who = if scan.mode == "quick" {
        app.user.as_str()
    } else {
        "root"
    };
    widgets::topbar(
        buf,
        p,
        area,
        &app.host,
        &app.user,
        &format!("{} scan · as {who}", scan.mode),
    );
    let bw = 80.min(area.width);
    let x = area.x + (area.width - bw) / 2;
    let mut y = area.y + 2;
    let title = if scan.failed.is_some() {
        Span::styled("Scan failed", p.bold(p.red))
    } else {
        Span::styled(format!("Scanning {}…", app.host), p.bold(p.fg))
    };
    put(buf, x, y, bw, &Line::from(title));
    y += 2;
    let total = scan.rows.len().max(1);
    let done = if scan.done_at.is_some() {
        total
    } else {
        scan.done()
    };
    let bar_w = usize::from(bw.saturating_sub(5));
    let filled = bar_w * done / total;
    let complete = done == total;
    let col = if complete { p.green } else { p.accent };
    let mut bar = vec![Span::styled("━".repeat(filled), p.fg_(col))];
    if filled < bar_w {
        bar.push(Span::styled(
            if filled > 0 { "╺" } else { "━" },
            p.fg_(p.bar_dim),
        ));
        bar.push(Span::styled(
            "━".repeat(bar_w - filled - 1),
            p.fg_(p.bar_dim),
        ));
    }
    bar.push(Span::styled(
        format!("{:>4}%", 100 * done / total),
        p.fg_(p.fg),
    ));
    put(buf, x, y, bw, &Line::from(bar));
    y += 1;
    let secs = scan.started.elapsed().as_secs();
    let elapsed = if scan.failed.is_some() {
        Span::styled("press escape to go back", p.fg_(p.muted))
    } else if scan.done_at.is_some() {
        Span::styled(format!("Finished in {}", mmss(secs)), p.fg_(p.green))
    } else {
        Span::styled(
            format!("{} elapsed · press escape to cancel", mmss(secs)),
            p.fg_(p.muted),
        )
    };
    put(buf, x, y, bw, &Line::from(elapsed));
    y += 2;
    let frame = usize::try_from(scan.started.elapsed().as_millis() / 100).unwrap_or(0);
    for row in &scan.rows {
        let l = match row.state {
            RowState::Done => {
                let (icon, c) = if row.fail > 0 {
                    ("✗", p.red)
                } else if row.warn > 0 {
                    ("!", p.yellow)
                } else {
                    ("✓", p.green)
                };
                let mut s = vec![
                    Span::raw(" "),
                    Span::styled(icon, p.bold(c)),
                    Span::raw("  "),
                    Span::raw(format!("{:<34}", row.name)),
                ];
                if row.fail > 0 {
                    s.push(Span::styled(format!("{} fail  ", row.fail), p.fg_(p.red)));
                }
                if row.warn > 0 {
                    s.push(Span::styled(format!("{} warn", row.warn), p.fg_(p.yellow)));
                }
                Line::from(s)
            }
            RowState::Running => Line::from(vec![
                Span::raw(" "),
                Span::styled(
                    FRAMES.get(frame % FRAMES.len()).copied().unwrap_or("⠋"),
                    p.bold(p.accent),
                ),
                Span::raw("  "),
                Span::styled(format!("{:<34}", row.name), p.bold(p.fg)),
                Span::styled("checking…", p.fg_(p.muted).add_modifier(Modifier::ITALIC)),
            ]),
            RowState::Waiting => Line::from(vec![
                Span::raw(" "),
                Span::styled("·", p.fg_(p.muted)),
                Span::raw("  "),
                Span::styled(row.name, p.fg_(p.muted)),
            ]),
        };
        put(buf, x, y, bw, &l);
        y += 1;
    }
    y += 1;
    put(
        buf,
        x,
        y,
        bw,
        &Line::from(Span::styled("LIVE FINDINGS", p.bold(p.text_muted))),
    );
    y += 1;
    rounded(
        buf,
        p,
        Rect {
            x,
            y,
            width: bw,
            height: 9,
        },
        p.panel,
        None,
    );
    let inner = usize::from(bw.saturating_sub(2));
    let mut lines = Vec::new();
    for l in &scan.log {
        match l {
            LogLine::Finding(fail, sec, msg) => lines.extend(wrap_spans(
                &[
                    badge(p, if *fail { "fail" } else { "warn" }),
                    Span::raw("  "),
                    Span::styled(format!("{sec}: "), p.fg_(p.muted)),
                    Span::styled(msg.clone(), p.fg_(p.fg)),
                ],
                inner,
            )),
            LogLine::Failure(reason) => lines.extend(wrap_spans(
                &[Span::styled(reason.clone(), p.fg_(p.red))],
                inner,
            )),
        }
    }
    let start = lines.len().saturating_sub(7);
    put_lines(
        buf,
        x + 1,
        y + 1,
        w16(inner),
        7,
        lines.get(start..).unwrap_or_default(),
    );
}
