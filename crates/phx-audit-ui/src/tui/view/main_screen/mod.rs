//! The main screen: overview header, sidebar and center column.

mod center;
mod detail;
mod sidebar;
mod stats;
mod table;

use crate::tui::app::{App, NARROW, WIDE};
use crate::tui::times::{ago, local_time};
use crate::tui::view::{hline, put_lines, vline, w16, wrap_spans};
use crate::tui::widgets::{self, put};
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::text::{Line, Span};
use phx_audit::store::Run;

use center::center;
use detail::detail;
use sidebar::sidebar;
use stats::{stats_text, trendbox};
use table::table;

pub(super) fn main_screen(buf: &mut Buffer, app: &App, area: Rect, run: &Run) {
    let p = &app.pal;
    let narrow = app.size.0 < NARROW;
    let wide = app.size.0 >= WIDE;
    let latest = app.runs.first().is_some_and(|r| r.path == run.path);
    let when = format!(
        "{} · {} scan · {}",
        local_time(run.meta.started, "%a %-d %b %H:%M"),
        run.meta.mode,
        ago(run.meta.started)
    );
    let right = if latest {
        when
    } else {
        format!("{when}  (older scan, h for history)")
    };
    widgets::topbar(buf, p, area, &app.host, &app.user, &right);
    overview(buf, app, area, run, narrow);

    let body_top = area.y + 7;
    let footer_y = area.bottom().saturating_sub(1);
    if footer_y <= body_top + 1 {
        return;
    }
    hline(buf, p, area.x, body_top, area.width);
    let top = body_top + 1;
    let bh = footer_y - top;
    let side_w = if narrow { 26 } else { 32 }.min(area.width);
    vline(buf, p, area.x + side_w - 1, top, bh);
    sidebar(
        buf,
        app,
        Rect {
            x: area.x,
            y: top,
            width: side_w - 1,
            height: bh,
        },
        run,
        narrow,
    );
    center(
        buf,
        app,
        Rect {
            x: area.x + side_w,
            y: top,
            width: area.width - side_w,
            height: bh,
        },
        run,
        narrow,
        wide,
    );
}

fn overview(buf: &mut Buffer, app: &App, area: Rect, run: &Run, narrow: bool) {
    let p = &app.pal;
    let c = run.counts();
    let (score, grade) = (c.score(), c.grade());
    let gc = p.grade(grade);
    let (x, y) = (area.x + 2, area.y + 2);
    for (i, row) in widgets::digits(&score.to_string()).iter().enumerate() {
        put(
            buf,
            x,
            y + w16(i),
            18,
            &Line::from(Span::styled(row.clone(), p.fg_(gc))),
        );
    }
    let filled = usize::try_from((12 * score + 50) / 100)
        .unwrap_or(0)
        .min(12);
    let meter = Line::from(vec![
        Span::styled("━".repeat(filled), p.fg_(gc)),
        Span::styled("─".repeat(12 - filled), p.fg_(p.muted)),
        Span::raw("  "),
        Span::styled(grade.to_string(), p.bold(gc)),
    ]);
    put(buf, x, y + 4, 18, &meter);
    let trend_w = if narrow { 0 } else { 36 };
    let stats_w = area.width.saturating_sub(4 + 20 + trend_w);
    let lines = wrap_spans(
        &stats_text(app, run),
        usize::from(stats_w.saturating_sub(4)),
    );
    put_lines(buf, x + 22, y, stats_w.saturating_sub(4), 5, &lines);
    if !narrow && area.width > 40 {
        trendbox(
            buf,
            app,
            Rect {
                x: area.right() - 2 - 36,
                y,
                width: 36,
                height: 5,
            },
            run,
        );
    }
}
