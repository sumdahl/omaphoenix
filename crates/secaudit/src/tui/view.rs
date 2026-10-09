//! Drawing. Works at 80×24: below 100 columns the sections pane folds into
//! a `[`/`]` section filter shown on the tab bar. Every state is also
//! spelled in text (badges, counts), never by colour alone.

use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::Color;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Gauge, List, ListItem, ListState, Paragraph, Wrap};
use secaudit_core::Severity;
use secaudit_core::store::sev_order;

use super::app::{App, Screen, Tab, ago, local_time};
use super::scan::RowState;
use super::theme::Theme;

fn badge(t: &Theme, sev: Severity) -> Span<'static> {
    let (text, color) = match sev {
        Severity::Fail => ("✕ FAIL", t.red),
        Severity::Warn => ("! WARN", t.yellow),
        Severity::Pass => ("✓ PASS", t.green),
        Severity::Skip => ("– SKIP", t.muted),
        _ => ("i NOTE", t.accent),
    };
    Span::styled(
        text,
        t.fg(color).add_modifier(ratatui::style::Modifier::BOLD),
    )
}

fn grade_color(t: &Theme, grade: char) -> Color {
    match grade {
        'A' | 'B' => t.green,
        'C' | 'D' => t.yellow,
        _ => t.red,
    }
}

pub fn draw(f: &mut Frame, app: &App) {
    let area = f.area();
    match &app.screen {
        Screen::Scan(_) => scan(f, app, area),
        Screen::History { sel } => history(f, app, area, *sel),
        Screen::Log { lines, scroll } => log(f, app, area, lines, *scroll),
        _ => main(f, app, area),
    }
    match &app.screen {
        Screen::Chooser => chooser(f, app, area),
        Screen::Confirm(c) => confirm(f, app, area, c),
        Screen::Help => help(f, app, area),
        _ => {}
    }
}

fn footer(f: &mut Frame, app: &App, area: Rect, hints: &str) {
    let t = &app.theme;
    let line = if let Some((msg, _)) = &app.toast {
        Line::from(Span::styled(msg.clone(), t.fg(t.accent)))
    } else if app.searching {
        Line::from(vec![
            Span::styled("/", t.fg(t.accent)),
            Span::raw(app.search.clone()),
            Span::styled("▏", t.fg(t.accent)),
        ])
    } else {
        Line::from(Span::styled(hints.to_owned(), t.fg(t.muted)))
    };
    f.render_widget(Paragraph::new(line), area);
}

fn main(f: &mut Frame, app: &App, area: Rect) {
    let t = &app.theme;
    let [top, tabs, body, foot] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Min(5),
        Constraint::Length(1),
    ])
    .areas(area);
    let hints =
        "s scan  x fix  space select  a accept  / search  h history  e export  ? keys  q quit";
    footer(f, app, foot, hints);

    let Some(run) = app.run() else {
        let msg = vec![
            Line::from(Span::styled("Security Audit", t.bold())),
            Line::from(""),
            Line::from("No scans yet."),
            Line::from(Span::styled(
                "Press s to scan. A quick scan needs no sudo.",
                t.fg(t.muted),
            )),
        ];
        f.render_widget(Paragraph::new(msg).centered(), centered(area, 60, 6));
        return;
    };

    let c = run.counts();
    let grade = c.grade();
    f.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled("Security Audit", t.bold()),
            Span::styled(
                if area.width < 100 {
                    format!("  {} · {}   ", run.meta.mode, ago(run.meta.started))
                } else {
                    format!(
                        "  {} · {} scan · {}   ",
                        run.meta.host,
                        run.meta.mode,
                        ago(run.meta.started)
                    )
                },
                t.fg(t.muted),
            ),
            Span::styled(
                format!("Score {} {grade}", c.score()),
                t.fg(grade_color(t, grade))
                    .add_modifier(ratatui::style::Modifier::BOLD),
            ),
            Span::styled(
                format!("   {} fail · {} warn", c.fail, c.warn),
                t.fg(t.muted),
            ),
        ])),
        top,
    );

    let mut spans = Vec::new();
    for (i, tab) in Tab::ALL.iter().enumerate() {
        let label = format!(" {} {} {} ", i + 1, tab.label(), app.tab_count(*tab));
        spans.push(if *tab == app.tab {
            Span::styled(label, t.selected())
        } else {
            Span::styled(label, t.fg(t.muted))
        });
    }
    let narrow = area.width < 100;
    if narrow {
        let sec = app.section.clone().unwrap_or_else(|| "all sections".into());
        spans.push(Span::styled(format!("  [ {sec} ]"), t.fg(t.accent)));
    }
    f.render_widget(Paragraph::new(Line::from(spans)), tabs);

    let right = if narrow {
        body
    } else {
        let [left, right] =
            Layout::horizontal([Constraint::Length(30), Constraint::Min(40)]).areas(body);
        sections(f, app, left);
        right
    };
    let [list_area, detail_area] =
        Layout::vertical([Constraint::Percentage(45), Constraint::Percentage(55)]).areas(right);
    findings(f, app, list_area);
    detail(f, app, detail_area);
}

fn sections(f: &mut Frame, app: &App, area: Rect) {
    let t = &app.theme;
    let run = app.run();
    let mut items = vec![ListItem::new("All sections")];
    for name in app.sections() {
        let issues = run.map_or(0, |r| {
            r.items
                .iter()
                .filter(|i| i.finding.section == name && i.is_issue() && !i.accepted)
                .count()
        });
        let mark = if app.section.as_deref() == Some(name.as_str()) {
            "› "
        } else {
            "  "
        };
        let count = if issues > 0 {
            format!(" {issues}")
        } else {
            String::new()
        };
        items.push(ListItem::new(Line::from(vec![
            Span::raw(format!("{mark}{name}")),
            Span::styled(count, t.fg(t.yellow)),
        ])));
    }
    let title = if app.sections_focus {
        " Sections (enter to filter) "
    } else {
        " Sections (tab) "
    };
    let block = Block::default()
        .borders(Borders::RIGHT)
        .title(Span::styled(title, t.fg(t.muted)));
    let mut state =
        ListState::default().with_selected(app.sections_focus.then_some(app.section_sel));
    f.render_stateful_widget(
        List::new(items).block(block).highlight_style(t.selected()),
        area,
        &mut state,
    );
}

fn findings(f: &mut Frame, app: &App, area: Rect) {
    let t = &app.theme;
    let visible = app.visible();
    let applied = app
        .run()
        .map(secaudit_core::store::Run::applied)
        .unwrap_or_default();
    let width = usize::from(area.width).saturating_sub(16);
    let items: Vec<ListItem> = visible
        .iter()
        .map(|i| {
            let mark = if app.marked.contains(&i.n) {
                "[x] "
            } else if i.finding.fix.is_empty() || !i.is_issue() {
                "    "
            } else {
                "[ ] "
            };
            let mut title: String = i.title().chars().take(width).collect();
            if applied.contains(&i.n) {
                title.push_str("  (fix applied)");
            }
            ListItem::new(Line::from(vec![
                Span::raw(mark),
                badge(t, i.finding.sev),
                Span::raw(format!("  {title}")),
            ]))
        })
        .collect();
    if items.is_empty() {
        let msg = match (app.tab, app.search.is_empty()) {
            (_, false) => "Nothing matches the search.",
            (Tab::Issues, true) => "No open issues here. Nice.",
            _ => "Nothing here.",
        };
        f.render_widget(
            Paragraph::new(Span::styled(msg, t.fg(t.muted))),
            area.inner(ratatui::layout::Margin::new(1, 1)),
        );
        return;
    }
    let mut state = ListState::default().with_selected((!app.sections_focus).then_some(app.sel));
    f.render_stateful_widget(
        List::new(items).highlight_style(t.selected()),
        area,
        &mut state,
    );
}

fn detail(f: &mut Frame, app: &App, area: Rect) {
    let t = &app.theme;
    let block = Block::default()
        .borders(Borders::TOP)
        .border_style(t.fg(t.muted));
    let Some(i) = app.selected() else {
        f.render_widget(block, area);
        return;
    };
    let fd = &i.finding;
    let mut lines = vec![
        Line::from(vec![
            badge(t, fd.sev),
            Span::styled(format!("  {}", fd.section), t.fg(t.muted)),
        ]),
        Line::from(Span::styled(i.title().to_owned(), t.bold())),
    ];
    if i.accepted {
        lines.push(Line::from(Span::styled(
            "Accepted risk: not scored. Press a to move it back to Issues.",
            t.fg(t.muted),
        )));
    }
    if !fd.detail.is_empty() {
        lines.push(Line::from(""));
        lines.extend(fd.detail.lines().map(|l| Line::from(format!("  {l}"))));
    }
    if !fd.fix.is_empty() || !fd.hint.is_empty() {
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled("How to address", t.bold())));
    }
    if !fd.fix.is_empty() {
        lines.push(Line::from(Span::styled(
            format!("$ {}", fd.fix),
            t.fg(t.accent),
        )));
        lines.push(Line::from(Span::styled(
            "x runs it here · c copies it",
            t.fg(t.muted),
        )));
    }
    if !fd.hint.is_empty() {
        lines.push(Line::from(fd.hint.clone()));
    }
    f.render_widget(
        Paragraph::new(lines)
            .block(block)
            .wrap(Wrap { trim: false }),
        area,
    );
}

fn scan(f: &mut Frame, app: &App, area: Rect) {
    let t = &app.theme;
    let Screen::Scan(s) = &app.screen else { return };
    let [top, gauge, body, foot] = Layout::vertical([
        Constraint::Length(2),
        Constraint::Length(1),
        Constraint::Min(5),
        Constraint::Length(1),
    ])
    .areas(area);
    let elapsed = s.started.elapsed().as_secs();
    let title = if s.failed.is_some() {
        "Scan failed".to_owned()
    } else {
        format!("Scanning · {} scan", s.mode)
    };
    f.render_widget(
        Paragraph::new(vec![Line::from(vec![
            Span::styled(title, t.bold()),
            Span::styled(
                format!("   {}:{:02}", elapsed / 60, elapsed % 60),
                t.fg(t.muted),
            ),
        ])]),
        top,
    );
    let total = s.rows.len().max(1);
    let ratio = f64::from(u32::try_from(s.done()).unwrap_or(0))
        / f64::from(u32::try_from(total).unwrap_or(1));
    f.render_widget(
        Gauge::default()
            .ratio(ratio.clamp(0.0, 1.0))
            .label(format!("{} of {total} sections", s.done()))
            .gauge_style(t.fg(t.accent)),
        gauge,
    );
    let [rows_area, log_area] =
        Layout::horizontal([Constraint::Length(44), Constraint::Min(20)]).areas(body);
    let rows: Vec<Line> = s
        .rows
        .iter()
        .map(|r| {
            let (mark, style) = match r.state {
                RowState::Done => ("✓", t.fg(t.green)),
                RowState::Running => ("›", t.fg(t.accent)),
                RowState::Waiting => (" ", t.fg(t.muted)),
            };
            let counts = match (r.fail, r.warn) {
                (0, 0) => String::new(),
                (f, w) => format!("  {f}✕ {w}!"),
            };
            Line::from(vec![
                Span::styled(format!(" {mark} "), style),
                Span::raw(r.name),
                Span::styled(counts, t.fg(t.yellow)),
            ])
        })
        .collect();
    f.render_widget(Paragraph::new(rows), rows_area);
    let mut log: Vec<Line> = s
        .log
        .iter()
        .rev()
        .take(usize::from(log_area.height))
        .rev()
        .map(|(fail, msg)| {
            Line::from(vec![
                badge(
                    t,
                    if *fail {
                        Severity::Fail
                    } else {
                        Severity::Warn
                    },
                ),
                Span::raw(format!("  {msg}")),
            ])
        })
        .collect();
    if let Some(reason) = &s.failed {
        log = reason
            .lines()
            .map(|l| Line::from(Span::styled(l.to_owned(), t.fg(t.red))))
            .collect();
    }
    f.render_widget(Paragraph::new(log).wrap(Wrap { trim: true }), log_area);
    let hint = if s.failed.is_some() {
        "esc back"
    } else {
        "esc cancel scan"
    };
    footer(f, app, foot, hint);
}

fn history(f: &mut Frame, app: &App, area: Rect, sel: usize) {
    let t = &app.theme;
    let [top, table, diff, foot] = Layout::vertical([
        Constraint::Length(2),
        Constraint::Min(5),
        Constraint::Length(10),
        Constraint::Length(1),
    ])
    .areas(area);
    let summary = app
        .runs
        .first()
        .map_or("No scans yet.".to_owned(), |latest| {
            let best = app
                .runs
                .iter()
                .map(|r| r.counts().score())
                .max()
                .unwrap_or(0);
            let c = latest.counts();
            format!(
                "Latest {} {}   Best {best}   {} kept (max {})",
                c.score(),
                c.grade(),
                app.runs.len(),
                secaudit_core::store::KEEP_RUNS
            )
        });
    f.render_widget(
        Paragraph::new(vec![
            Line::from(Span::styled("Scan history", t.bold())),
            Line::from(Span::styled(summary, t.fg(t.muted))),
        ]),
        top,
    );
    let rows: Vec<ListItem> = app
        .runs
        .iter()
        .enumerate()
        .map(|(i, r)| {
            let c = r.counts();
            let delta = app.runs.get(i + 1).map_or(String::new(), |p| {
                let d = i64::from(c.score()) - i64::from(p.counts().score());
                match d {
                    0 => "  =".into(),
                    d if d > 0 => format!(" +{d}"),
                    d => format!(" {d}"),
                }
            });
            ListItem::new(Line::from(vec![
                Span::raw(format!(
                    "{:<18}",
                    local_time(r.meta.started, "%a %-d %b %H:%M")
                )),
                Span::styled(format!("{:<6}", r.meta.mode), t.fg(t.muted)),
                Span::styled(
                    format!("{:>3} {}", c.score(), c.grade()),
                    t.fg(grade_color(t, c.grade())),
                ),
                Span::styled(format!("{delta:<5}"), t.fg(t.muted)),
                Span::raw(format!("  {} fail · {} warn", c.fail, c.warn)),
            ]))
        })
        .collect();
    let mut state = ListState::default().with_selected(Some(sel));
    f.render_stateful_widget(
        List::new(rows).highlight_style(t.selected()),
        table,
        &mut state,
    );

    let lines = compare(app, sel);
    f.render_widget(
        Paragraph::new(lines).block(
            Block::default()
                .borders(Borders::TOP)
                .border_style(t.fg(t.muted)),
        ),
        diff,
    );
    footer(f, app, foot, "enter open  d delete  esc back");
}

/// "New" and "Resolved" issues of scan `sel` against the scan before it.
fn compare(app: &App, sel: usize) -> Vec<Line<'static>> {
    let t = &app.theme;
    let mut lines = Vec::new();
    if let Some(r) = app.runs.get(sel) {
        match app.runs.get(sel + 1) {
            None => lines.push(Line::from(Span::styled(
                "First scan on record; nothing to compare against.",
                t.fg(t.muted),
            ))),
            Some(prev) => {
                let (now, before) = (r.issue_keys(), prev.issue_keys());
                let mut new: Vec<_> = now.difference(&before).collect();
                let mut fixed: Vec<_> = before.difference(&now).collect();
                new.sort_by_key(|k| sev_order(k.0));
                fixed.sort_by_key(|k| sev_order(k.0));
                lines.push(Line::from(Span::styled(
                    format!(
                        "Compared with {} ({} scan)",
                        local_time(prev.meta.started, "%a %-d %b %H:%M"),
                        prev.meta.mode
                    ),
                    t.fg(t.muted),
                )));
                lines.push(Line::from(Span::styled(
                    if new.is_empty() {
                        "New       none"
                    } else {
                        "New"
                    },
                    t.bold(),
                )));
                for (sev, sec, msg) in new.iter().take(3) {
                    lines.push(Line::from(vec![
                        Span::raw("  "),
                        badge(t, *sev),
                        Span::styled(format!("  {sec}: "), t.fg(t.muted)),
                        Span::raw(msg.trim_end_matches(':').to_owned()),
                    ]));
                }
                lines.push(Line::from(Span::styled(
                    if fixed.is_empty() {
                        "Resolved  none"
                    } else {
                        "Resolved"
                    },
                    t.bold(),
                )));
                for (_, sec, msg) in fixed.iter().take(3) {
                    lines.push(Line::from(vec![
                        Span::styled("  ✓ ", t.fg(t.green)),
                        Span::styled(format!("{sec}: "), t.fg(t.muted)),
                        Span::raw(msg.trim_end_matches(':').to_owned()),
                    ]));
                }
                if prev.meta.mode != r.meta.mode {
                    lines.push(Line::from(Span::styled(
                        "Different scan modes: some differences come from coverage, not changes.",
                        t.fg(t.muted),
                    )));
                }
            }
        }
    }
    lines
}

fn log(f: &mut Frame, app: &App, area: Rect, lines: &[String], scroll: usize) {
    let t = &app.theme;
    let [body, foot] = Layout::vertical([Constraint::Min(3), Constraint::Length(1)]).areas(area);
    let text: Vec<Line> = lines
        .iter()
        .skip(scroll)
        .take(usize::from(body.height))
        .map(|l| Line::from(l.clone()))
        .collect();
    f.render_widget(
        Paragraph::new(text).block(Block::default().title(Span::styled(" Full output ", t.bold()))),
        body,
    );
    footer(f, app, foot, "↑↓ scroll  space page  esc back");
}

fn centered(area: Rect, w: u16, h: u16) -> Rect {
    let w = w.min(area.width);
    let h = h.min(area.height);
    Rect {
        x: area.x + (area.width - w) / 2,
        y: area.y + (area.height - h) / 2,
        width: w,
        height: h,
    }
}

fn modal(f: &mut Frame, app: &App, area: Rect, title: &str, lines: Vec<Line>) {
    let t = &app.theme;
    let h = u16::try_from(lines.len())
        .unwrap_or(u16::MAX)
        .saturating_add(2);
    let r = centered(area, 72, h);
    f.render_widget(Clear, r);
    let block = Block::bordered()
        .title(Span::styled(format!(" {title} "), t.bold()))
        .border_style(t.fg(t.accent));
    f.render_widget(
        Paragraph::new(lines)
            .block(block)
            .wrap(Wrap { trim: false }),
        r,
    );
}

fn chooser(f: &mut Frame, app: &App, area: Rect) {
    let t = &app.theme;
    let line = |k: &str, name: &str, what: &str| {
        Line::from(vec![
            Span::styled(format!(" {k} "), t.selected()),
            Span::styled(format!("  {name:<7}"), t.bold()),
            Span::styled(what.to_owned(), t.fg(t.muted)),
        ])
    };
    modal(
        f,
        app,
        area,
        "Start a scan",
        vec![
            line(
                "q",
                "Quick",
                "as you, no sudo; root-only checks are skipped",
            ),
            line(
                "f",
                "Full",
                "with sudo: shadow, sudoers, firewall rules, LUKS",
            ),
            line(
                "d",
                "Deep",
                "full + package checksums and a whole-disk scan (slow)",
            ),
            Line::from(""),
            Line::from(Span::styled(
                "Every scan is read-only. esc to cancel.",
                t.fg(t.muted),
            )),
        ],
    );
}

fn confirm(f: &mut Frame, app: &App, area: Rect, c: &super::app::Confirm) {
    let t = &app.theme;
    let mut lines: Vec<Line> = c
        .code
        .iter()
        .map(|l| Line::from(Span::styled(l.clone(), t.fg(t.accent))))
        .collect();
    if !lines.is_empty() {
        lines.push(Line::from(""));
    }
    lines.push(Line::from(Span::styled(c.note.clone(), t.fg(t.muted))));
    lines.push(Line::from(""));
    lines.push(Line::from(vec![
        Span::styled(format!(" y {} ", c.yes), t.selected()),
        Span::raw("   "),
        Span::styled(" n Cancel ", t.fg(t.muted)),
    ]));
    modal(f, app, area, &c.title, lines);
}

fn help(f: &mut Frame, app: &App, area: Rect) {
    let t = &app.theme;
    let keys = [
        ("s / r", "scan / rescan in the same mode"),
        ("x", "run the fix of the selection (or the selected ones)"),
        ("space  *", "select one / all fixable"),
        ("a", "accept the risk (or move it back)"),
        ("/  esc", "search / clear search and selection"),
        ("1–5", "Issues, Accepted, Notes, Passed, All"),
        ("tab  [ ]", "sections pane / previous, next section"),
        ("c  e  o", "copy fix, export Markdown, full output"),
        ("h", "history: compare scans, delete"),
        ("q", "quit"),
    ];
    let mut lines: Vec<Line> = keys
        .iter()
        .map(|(k, what)| {
            Line::from(vec![
                Span::styled(format!("{k:<10}"), t.bold()),
                Span::raw((*what).to_owned()),
            ])
        })
        .collect();
    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        "Any key closes this.",
        t.fg(t.muted),
    )));
    modal(f, app, area, "Keys", lines);
}
