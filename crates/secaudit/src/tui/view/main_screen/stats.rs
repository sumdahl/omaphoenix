//! The overview's trend box and stats line.

#![allow(
    clippy::many_single_char_names,
    reason = "drawing code: p palette, r rect, x/y/w/h"
)]
#![allow(
    clippy::too_many_lines,
    reason = "one function per screen, mirroring compose() in the original"
)]

use crate::tui::app::App;
use crate::tui::times::local_time;
use crate::tui::view::{mmss, put_lines, wrap_spans};
use crate::tui::widgets::{self, put};
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Modifier;
use ratatui::text::{Line, Span};
use secaudit_core::store::Run;
use std::fmt::Write as _;

pub(super) fn trendbox(buf: &mut Buffer, app: &App, r: Rect, run: &Run) {
    let p = &app.pal;
    put(
        buf,
        r.x,
        r.y,
        r.width,
        &Line::from(Span::styled("SCORE TREND", p.bold(p.text_muted))),
    );
    let chron: Vec<&Run> = {
        let v: Vec<&Run> = app
            .runs
            .iter()
            .rev()
            .filter(|x| x.meta.started <= run.meta.started)
            .collect();
        if v.is_empty() { vec![run] } else { v }
    };
    let mut cap: Vec<Span> = Vec::new();
    let spark = chron.len() >= 4;
    if !spark {
        for (i, x) in chron.iter().enumerate() {
            if i > 0 {
                cap.push(Span::styled(" → ", p.fg_(p.muted)));
            }
            let c = x.counts();
            cap.push(Span::styled(
                c.score().to_string(),
                p.bold(p.grade(c.grade())),
            ));
        }
        cap.push(Span::raw("\n"));
    }
    let first = chron.first().map_or(run.meta.started, |x| x.meta.started);
    let n = chron.len();
    cap.push(Span::styled(
        format!(
            "{n} scan{} · {} → {}",
            if n == 1 { "" } else { "s" },
            local_time(first, "%-d %b"),
            local_time(run.meta.started, "%-d %b")
        ),
        p.fg_(p.muted),
    ));
    if spark {
        let data: Vec<u32> = chron.iter().map(|x| x.counts().score()).collect();
        let lines = widgets::sparkline(&data, usize::from(r.width), 2, p.red, p.green);
        put_lines(buf, r.x, r.y + 2, r.width, 2, &lines);
        // The caption sits below the box's 5 rows, as in the original.
        return;
    }
    let lines = wrap_spans(&cap, usize::from(r.width));
    put_lines(buf, r.x, r.y + 2, r.width, 3, &lines);
}

pub(super) fn stats_text(app: &App, run: &Run) -> Vec<Span<'static>> {
    let p = &app.pal;
    let c = run.counts();
    let mut t = Vec::new();
    for (sev, n, icon, label) in [
        ("fail", c.fail, "✗", "fail"),
        ("warn", c.warn, "!", "warn"),
        ("pass", c.pass, "✓", "pass"),
    ] {
        let st = if n > 0 {
            p.bold(p.sev(sev)).bg(p.panel)
        } else {
            p.fg_(p.muted).bg(p.panel)
        };
        t.push(Span::styled(format!(" {icon} {n} {label} "), st));
        t.push(Span::raw("  "));
    }
    if c.accepted > 0 {
        t.push(Span::styled(
            format!(" ≈ {} accepted ", c.accepted),
            p.fg_(p.muted).bg(p.panel),
        ));
    }
    t.push(Span::raw("\n"));
    let mut mode = run.meta.mode.clone();
    if let Some(f) = mode.get_mut(0..1) {
        f.make_ascii_uppercase();
    }
    let host = if run.meta.host.is_empty() {
        app.host.clone()
    } else {
        run.meta.host.clone()
    };
    let mut meta = format!("{mode} scan of {host}");
    if let Some(fin) = run.meta.finished
        && run.meta.started > 0
    {
        let _ = write!(
            meta,
            " · took {}",
            mmss(fin.saturating_sub(run.meta.started))
        );
    }
    let _ = writeln!(meta, " · {} notes · {} skipped", c.info, c.skip);
    t.push(Span::styled(meta, p.fg_(p.muted)));
    if let Some(prev) = app.previous_of_run(run) {
        let (a, b) = (run.issue_keys(), prev.issue_keys());
        let (new, fixed) = (a.difference(&b).count(), b.difference(&a).count());
        let since = if prev.meta.mode == run.meta.mode {
            String::new()
        } else {
            format!(" ({})", prev.meta.mode)
        };
        t.push(Span::styled(
            format!("Since previous scan{since}: "),
            p.fg_(p.muted),
        ));
        t.push(Span::styled(
            format!("{new} new"),
            if new > 0 {
                p.bold(p.red)
            } else {
                p.fg_(p.muted)
            },
        ));
        t.push(Span::styled(" · ", p.fg_(p.muted)));
        t.push(Span::styled(
            format!("{fixed} resolved"),
            if fixed > 0 {
                p.bold(p.green)
            } else {
                p.fg_(p.muted)
            },
        ));
        let delta = i64::from(run.counts().score()) - i64::from(prev.counts().score());
        if delta != 0 {
            t.push(Span::styled(
                format!(" · score {}{delta}", if delta > 0 { "+" } else { "" }),
                p.fg_(if delta > 0 { p.green } else { p.red }),
            ));
        }
        t.push(Span::raw("\n"));
    } else {
        t.push(Span::styled("First scan on record\n", p.fg_(p.muted)));
    }
    let it = p.fg_(p.muted).add_modifier(Modifier::ITALIC);
    let key = p.bold(p.accent);
    if run.meta.mode == "quick" {
        t.push(Span::styled("Root-only checks skipped. Press ", it));
        t.push(Span::styled("s", key));
        t.push(Span::styled(" for a full scan.", it));
    } else {
        let applied = run.applied();
        let unfixed = run
            .items
            .iter()
            .filter(|f| {
                f.is_issue() && !f.accepted && !f.finding.fix.is_empty() && !applied.contains(&f.n)
            })
            .count();
        if unfixed > 0 {
            t.push(Span::styled(
                format!(
                    "{unfixed} issue{} can be fixed automatically. Press ",
                    if unfixed == 1 { "" } else { "s" }
                ),
                it,
            ));
            t.push(Span::styled("*", key));
            t.push(Span::styled(" then ", it));
            t.push(Span::styled("x", key));
            t.push(Span::styled(" to fix them together.", it));
        }
    }
    t
}
