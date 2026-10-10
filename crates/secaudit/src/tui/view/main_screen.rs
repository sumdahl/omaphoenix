//! The welcome screen and the main screen: overview, sidebar, table, detail.

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
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use secaudit_core::store::{Item, Run, sev_name};

use crate::tui::app::{App, Focus, NARROW, Tab, WIDE};
use crate::tui::theme::blend;
use crate::tui::times::{ago, local_time};
use crate::tui::widgets::{self, badge, clip, fill, pad, put, wrap};

use super::{HELP_W, hline, mmss, put_lines, rounded, scrolled, vline, w16, wrap_spans};

pub(super) fn welcome(buf: &mut Buffer, app: &App, area: Rect) {
    let p = &app.pal;
    widgets::topbar(buf, p, area, &app.host, &app.user, "");
    let bw = 80.min(area.width);
    let inner = usize::from(bw.saturating_sub(8));
    let mut lines = vec![
        Line::from(Span::styled("No scans yet", p.bold(p.fg))),
        Line::from(""),
    ];
    lines.extend(wrap_spans(
        &[Span::styled(
            "Security Audit checks packages, boot and disk encryption, kernel hardening, firewall, network exposure, SSH, accounts, permissions, persistence, containers, secrets in your home folder and rootkit indicators.",
            p.fg_(p.fg),
        )],
        inner,
    ));
    lines.push(Line::from(""));
    lines.extend(wrap_spans(
        &[Span::styled(
            "Everything is read-only. A full scan uses sudo only to read root-owned files.",
            p.fg_(p.muted),
        )],
        inner,
    ));
    let bh = w16(lines.len()) + 2 + 2 + 1 + 3;
    let body = Rect {
        y: area.y + 1,
        height: area.height.saturating_sub(2),
        ..area
    };
    let x = body.x + body.width.saturating_sub(bw) / 2;
    let y = body.y + body.height.saturating_sub(bh) / 2;
    rounded(
        buf,
        p,
        Rect {
            x,
            y,
            width: bw,
            height: bh,
        },
        p.accent,
        None,
    );
    put_lines(buf, x + 4, y + 2, w16(inner), w16(lines.len()), &lines);
    // Buttons: 16 wide, 1 apart, centred.
    let by = y + 2 + w16(lines.len()) + 1;
    let total = 16 * 3 + 2;
    let mut bx = x + 4 + w16(inner).saturating_sub(total) / 2;
    let focus = if let Focus::Welcome(i) = app.focus {
        Some(i)
    } else {
        None
    };
    for (i, (label, id)) in [
        ("Full scan", "btn-full"),
        ("Quick scan", "btn-quick"),
        ("Deep scan", "btn-deep"),
    ]
    .into_iter()
    .enumerate()
    {
        widgets::button(
            buf,
            p,
            Rect {
                x: bx,
                y: by,
                width: 16,
                height: 3,
            },
            label,
            i == 0,
            false,
            focus == Some(i),
        );
        let mut hits = app.hits.borrow_mut();
        for dy in 0..3 {
            hits.buttons.push((bx, by + dy, 16, id));
        }
        bx += 17;
    }
}

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

fn trendbox(buf: &mut Buffer, app: &App, r: Rect, run: &Run) {
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

fn stats_text(app: &App, run: &Run) -> Vec<Span<'static>> {
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

fn sidebar(buf: &mut Buffer, app: &App, r: Rect, run: &Run, narrow: bool) {
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

fn center(buf: &mut Buffer, app: &App, r: Rect, run: &Run, narrow: bool, wide: bool) {
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

/// The findings `DataTable`.
fn table(buf: &mut Buffer, app: &App, r: Rect, rows: &[&Item]) {
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

fn detail(buf: &mut Buffer, app: &App, r: Rect, run: &Run) {
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
