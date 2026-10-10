//! The center column: tabs, search, table and detail.

#![allow(
    clippy::many_single_char_names,
    reason = "drawing code: p palette, r rect, x/y/w/h"
)]
#![allow(
    clippy::too_many_lines,
    reason = "one function per screen, mirroring compose() in the original"
)]

use crate::tui::app::{App, Focus, Tab};
use crate::tui::theme::blend;
use crate::tui::view::{hline, vline};
use crate::tui::widgets::{self, fill, put};
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use secaudit_core::store::Run;

use super::{detail, table};

pub(super) fn center(buf: &mut Buffer, app: &App, r: Rect, run: &Run, narrow: bool, wide: bool) {
    let p = &app.pal;
    let search_w = if narrow { 22 } else { 36 };
    let sx = r.right().saturating_sub(1 + search_w);
    let c = run.counts();
    let labels = [
        format!("Issues {}", c.fail + c.warn),
        format!("Accepted {}", c.accepted),
        format!("Notes {}", c.info + c.skip),
        format!("Passed {}", c.pass),
        format!("All {}", run.items.len()),
    ];
    let active = Tab::ALL.iter().position(|t| *t == app.tab).unwrap_or(0);
    let tabs_area = Rect {
        x: r.x + 1,
        y: r.y + 1,
        width: sx.saturating_sub(r.x + 1),
        height: 2,
    };
    let ranges = widgets::tabs(buf, p, tabs_area, &labels, active, app.focus == Focus::Tabs);
    widgets::input(
        buf,
        p,
        Rect {
            x: sx,
            y: r.y,
            width: search_w,
            height: 3,
        },
        &app.search,
        "Search findings…  /",
        app.focus == Focus::Search,
    );
    {
        let mut hits = app.hits.borrow_mut();
        hits.tabs = ranges
            .iter()
            .filter(|(a, _)| *a < sx)
            .map(|(a, b)| (*a, (*b).min(sx), r.y + 1))
            .collect();
        hits.search = Some((sx, r.y, search_w));
    }
    let mut y = r.y + 3;
    if !app.marked.is_empty() {
        let n = app.marked.len();
        let bg = blend(p.accent, p.bg, 0.25);
        fill(
            buf,
            Rect {
                x: r.x,
                y,
                width: r.width,
                height: 1,
            },
            Style::new().bg(bg),
        );
        let k = p.bold(p.accent).bg(bg);
        let t = p.fg_(blend(Color::Rgb(255, 255, 255), bg, 0.87)).bg(bg);
        let l = Line::from(vec![
            Span::styled("◉ ", k),
            Span::styled(format!("{n} selected"), t.add_modifier(Modifier::BOLD)),
            Span::styled("   ", t),
            Span::styled("x", k),
            Span::styled(format!(" fix {}{n}   ", if n > 1 { "all " } else { "" }), t),
            Span::styled("space", k),
            Span::styled(" toggle   ", t),
            Span::styled("*", k),
            Span::styled(" all   ", t),
            Span::styled("esc", k),
            Span::styled(" clear", t),
        ]);
        put(buf, r.x + 1, y, r.width.saturating_sub(2), &l);
        y += 1;
    }
    let sh = r.bottom().saturating_sub(y);
    let rows = app.visible();
    if wide {
        let tw = r.width * 3 / 5;
        table(
            buf,
            app,
            Rect {
                x: r.x,
                y,
                width: tw,
                height: sh,
            },
            &rows,
        );
        vline(buf, p, r.x + tw, y, sh);
        detail(
            buf,
            app,
            Rect {
                x: r.x + tw + 3,
                y,
                width: (r.width - tw).saturating_sub(5),
                height: sh,
            },
            run,
        );
    } else {
        // 3fr / 2fr: the table gets floor(3/5), the detail the rest, 9..=18.
        let dh = (sh - sh * 3 / 5).clamp(9, 18).min(sh);
        let th = sh - dh;
        table(
            buf,
            app,
            Rect {
                x: r.x,
                y,
                width: r.width,
                height: th,
            },
            &rows,
        );
        if dh > 0 {
            hline(buf, p, r.x, y + th, r.width);
            detail(
                buf,
                app,
                Rect {
                    x: r.x + 2,
                    y: y + th + 1,
                    width: r.width.saturating_sub(4),
                    height: dh - 1,
                },
                run,
            );
        }
    }
}
