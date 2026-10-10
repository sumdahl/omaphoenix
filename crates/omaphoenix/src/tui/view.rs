//! Drawing, at 80×24 and up. Every state is spelled in text; colour only
//! reinforces it (DESIGN.md §12).

use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::Modifier;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, ListState, Paragraph, Wrap};
use secaudit_core::Severity;

use ratatui::widgets::Clear;

use super::state::{MIGRATE_STEPS, Section, State};

fn ago(iso: &str) -> String {
    let secs = phx_core::sys::output(None, &["date", "-d", iso, "+%s"])
        .ok()
        .and_then(|s| s.parse::<u64>().ok());
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    match secs.map(|t| now.saturating_sub(t)) {
        None => iso.to_owned(),
        Some(0..60) => "just now".into(),
        Some(s @ 60..3600) => format!("{}m ago", s / 60),
        Some(s @ 3600..172_800) => format!("{}h ago", s / 3600),
        Some(s) => format!("{}d ago", s / 86_400),
    }
}

pub fn draw(f: &mut Frame, st: &State<'_>) {
    let t = &st.theme;
    let [top, body, foot] = Layout::vertical([
        Constraint::Length(2),
        Constraint::Min(5),
        Constraint::Length(1),
    ])
    .areas(f.area());
    let mut tabs = vec![Span::styled("OmaPhoenix ", t.bold().fg(t.accent))];
    for (i, s) in Section::ALL.iter().enumerate() {
        let label = format!(" {} {} ", i + 1, s.label());
        tabs.push(if *s == st.section {
            Span::styled(label, t.selected())
        } else {
            Span::styled(label, t.fg(t.muted))
        });
    }
    f.render_widget(Paragraph::new(Line::from(tabs)), top);

    match st.section {
        Section::Pulse => pulse(f, st, body),
        Section::Timeline => timeline(f, st, body),
        Section::Audit => audit(f, st, body),
        Section::Backup => backup(f, st, body),
        Section::Migrate => migrate(f, st, body),
        Section::Settings => settings(f, st, body),
    }

    let hint = match st.section {
        Section::Pulse => "enter primary action · s snapshot · d re-check · 1-6 sections · ? keys",
        Section::Timeline => "↑↓ move · enter preview · r restore · s snapshot · / search · ? keys",
        Section::Audit => "s scan · ↑↓ move · a accept · x run fix · / search · ? keys",
        Section::Backup if st.in_items => {
            "↑↓ move · space tick · a all/none · esc back · w save · q quit"
        }
        Section::Backup => "↑↓ move · enter open · space all/none · w save · 1-6 · q quit",
        Section::Migrate => "↑↓ step · enter do it · R start over · 1-6 sections · q quit",
        Section::Settings => "t timer on/off · e set up encrypted secrets · 1-6 sections · q quit",
    };
    let line = match (&st.toast, &st.confirm) {
        (_, Some((question, _))) => {
            Line::from(Span::styled(question.clone(), t.bold().fg(t.yellow)))
        }
        _ if st.searching || !st.search.is_empty() => Line::from(vec![
            Span::styled("/ ", t.bold().fg(t.accent)),
            Span::raw(st.search.clone()),
            Span::styled(
                if st.searching {
                    "▏  enter keep · esc clear"
                } else {
                    "   esc clears the search"
                },
                t.fg(t.muted),
            ),
        ]),
        (Some((msg, _)), None) => Line::from(Span::styled(msg.clone(), t.fg(t.accent))),
        _ => match st.busy_label() {
            Some(b) => Line::from(vec![
                Span::styled(format!("{b}   "), t.fg(t.accent)),
                Span::styled(hint, t.fg(t.muted)),
            ]),
            None => Line::from(Span::styled(hint, t.fg(t.muted))),
        },
    };
    f.render_widget(Paragraph::new(line), foot);
    if st.help {
        keys_overlay(f, st, body);
    }
    if let Some((q, sel)) = &st.palette {
        palette_overlay(f, st, body, q, *sel);
    }
}

/// A centred box over the body.
fn overlay(area: Rect, w: u16, h: u16) -> Rect {
    let w = w.min(area.width);
    let h = h.min(area.height);
    Rect {
        x: area.x + (area.width - w) / 2,
        y: area.y + (area.height - h) / 2,
        width: w,
        height: h,
    }
}

/// `?`: every key for this screen (DESIGN.md §7.1).
fn keys_overlay(f: &mut Frame, st: &State<'_>, body: Rect) {
    let t = &st.theme;
    let here: &[(&str, &str)] = match st.section {
        Section::Pulse => &[
            ("enter", "the primary action"),
            ("s", "snapshot now"),
            ("d", "re-check backups"),
        ],
        Section::Timeline => &[
            ("↑↓ jk", "move"),
            ("enter p", "preview"),
            ("r", "restore (asks first)"),
            ("s", "snapshot now"),
            ("/", "search"),
            ("esc", "close output"),
        ],
        Section::Audit => &[
            ("s", "scan"),
            ("R", "rescan"),
            ("↑↓ jk", "move"),
            ("a", "accept risk / undo"),
            ("x", "run fix (asks first)"),
            ("o", "full Security Audit app"),
            ("/", "search"),
        ],
        Section::Backup => &[
            ("↑↓ jk", "move"),
            ("enter → l", "open category"),
            ("esc ← h", "back"),
            ("space", "tick / all"),
            ("a", "all or none"),
            ("w", "save selection"),
        ],
        Section::Migrate => &[("↑↓ jk", "step"), ("enter", "do it"), ("R", "start over")],
        Section::Settings => &[
            ("t", "timer on/off"),
            ("e", "encrypted secrets"),
            ("i", "set up snapshots"),
        ],
    };
    let global: [(&str, &str); 6] = [
        ("1-6 tab", "sections"),
        (": ⌃k", "command palette"),
        ("u", "undo last restore"),
        ("?", "this list"),
        ("esc", "back / close"),
        ("q", "quit"),
    ];
    let mut lines = vec![Line::from(Span::styled(
        format!(" Keys: {}", st.section.label()),
        t.bold(),
    ))];
    for (k, d) in here.iter().chain(global.iter()) {
        lines.push(Line::from(vec![
            Span::styled(format!(" {k:>10} "), t.bold().fg(t.accent)),
            Span::raw((*d).to_owned()),
        ]));
    }
    let r = overlay(body, 46, u16::try_from(lines.len() + 2).unwrap_or(20));
    f.render_widget(Clear, r);
    f.render_widget(
        Paragraph::new(lines).block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(t.fg(t.accent)),
        ),
        r,
    );
}

/// `:` / `⌃k`: every action by name.
fn palette_overlay(f: &mut Frame, st: &State<'_>, body: Rect, q: &str, sel: usize) {
    let t = &st.theme;
    let actions = State::actions(q);
    let mut lines = vec![
        Line::from(vec![
            Span::styled(" : ", t.bold().fg(t.accent)),
            Span::raw(format!("{q}▏")),
        ]),
        Line::from(""),
    ];
    for (i, a) in actions.iter().enumerate() {
        let style = if i == sel {
            t.selected()
        } else {
            ratatui::style::Style::new()
        };
        lines.push(Line::from(Span::styled(format!(" {a} "), style)));
    }
    if actions.is_empty() {
        lines.push(Line::from(Span::styled(
            " no matching action",
            t.fg(t.muted),
        )));
    }
    let r = overlay(body, 50, u16::try_from(lines.len() + 2).unwrap_or(20));
    f.render_widget(Clear, r);
    f.render_widget(
        Paragraph::new(lines).block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(t.fg(t.accent)),
        ),
        r,
    );
}

fn heading(st: &State<'_>, text: &str) -> Line<'static> {
    Line::from(Span::styled(text.to_owned(), st.theme.bold()))
}

fn pulse(f: &mut Frame, st: &State<'_>, area: Rect) {
    let t = &st.theme;
    let mut lines = vec![heading(st, "Protection")];
    if st.app.repo.exists() {
        match st.snaps.first() {
            Some(s) => lines.push(Line::from(format!(
                "  Last snapshot {} ({})",
                ago(&s.time),
                s.id
            ))),
            None => lines.push(Line::from(Span::styled("  ✗ No snapshot yet", t.fg(t.red)))),
        }
        let timer_on = !st.timer.starts_with("timer off");
        lines.push(Line::from(if timer_on {
            "  ✓ Automatic snapshots on".to_owned()
        } else {
            "  ! Automatic snapshots off (6 Settings)".to_owned()
        }));
    } else {
        lines.push(Line::from(Span::styled(
            "  ✗ Not set up: nothing is protected yet",
            t.fg(t.red),
        )));
    }
    lines.push(Line::from(""));
    lines.push(heading(st, "Health"));
    match &st.audit {
        Some(run) => {
            let c = run.counts();
            let color = match c.grade() {
                'A' | 'B' => t.green,
                'C' | 'D' => t.yellow,
                _ => t.red,
            };
            lines.push(Line::from(vec![
                Span::raw("  Audit score "),
                Span::styled(
                    format!("{} {}", c.score(), c.grade()),
                    t.fg(color).add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    format!(
                        "   {} fail · {} warn · {} accepted",
                        c.fail, c.warn, c.accepted
                    ),
                    t.fg(t.muted),
                ),
            ]));
        }
        None => lines.push(Line::from(Span::styled(
            "  No audit yet (3 Audit, then s)",
            t.fg(t.muted),
        ))),
    }
    match &st.findings {
        Some(f) => {
            let risks = f.iter().filter(|x| x.level == "risk").count();
            lines.push(Line::from(if risks == 0 {
                Span::styled(
                    "  ✓ Nothing would be lost in a reinstall".to_owned(),
                    t.fg(t.green),
                )
            } else {
                Span::styled(
                    format!("  ✗ {risks} thing(s) would be lost in a reinstall"),
                    t.fg(t.red),
                )
            }));
            for x in f.iter().filter(|x| x.level == "risk").take(4) {
                lines.push(Line::from(Span::styled(
                    format!("      {}: {}", x.area, x.msg),
                    t.fg(t.muted),
                )));
            }
        }
        None => lines.push(Line::from(Span::styled(
            "  Checking what a reinstall would lose…",
            t.fg(t.muted),
        ))),
    }
    lines.push(Line::from(""));
    let (_, action) = st.primary();
    lines.push(Line::from(vec![Span::styled(
        format!(" enter  {action} "),
        t.selected(),
    )]));
    f.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), area);
}

fn timeline(f: &mut Frame, st: &State<'_>, area: Rect) {
    let t = &st.theme;
    let show_output = !st.output.is_empty() || st.jobs.contains("command");
    let [list_area, out_area] = if show_output {
        Layout::vertical([Constraint::Percentage(40), Constraint::Percentage(60)]).areas(area)
    } else {
        [area, Rect::default()]
    };
    let snaps = st.snap_view();
    if snaps.is_empty() && !st.search.is_empty() {
        f.render_widget(
            Paragraph::new(Span::styled(
                "No snapshot matches the search.",
                t.fg(t.muted),
            )),
            list_area,
        );
    } else if snaps.is_empty() {
        f.render_widget(
            Paragraph::new(Span::styled(
                "No snapshots yet. Press s to take the first one.",
                t.fg(t.muted),
            )),
            list_area,
        );
    } else {
        let items: Vec<ListItem> = snaps
            .iter()
            .map(|s| {
                ListItem::new(Line::from(vec![
                    Span::raw(format!("{}  {:<10}", s.id, ago(&s.time))),
                    Span::styled(s.subject.clone(), t.fg(t.muted)),
                ]))
            })
            .collect();
        let mut ls = ListState::default().with_selected(Some(st.snap_sel));
        f.render_stateful_widget(
            List::new(items).highlight_style(t.selected()),
            list_area,
            &mut ls,
        );
    }
    if show_output {
        let height = usize::from(out_area.height.saturating_sub(1));
        let lines: Vec<Line> = st
            .output
            .iter()
            .rev()
            .take(height)
            .rev()
            .map(|l| Line::from(l.clone()))
            .collect();
        let block = Block::default()
            .borders(Borders::TOP)
            .title(Span::styled(format!(" {} ", st.output_title), t.bold()))
            .border_style(t.fg(t.muted));
        f.render_widget(Paragraph::new(lines).block(block), out_area);
    }
}

fn audit(f: &mut Frame, st: &State<'_>, area: Rect) {
    let t = &st.theme;
    let Some(run) = &st.audit else {
        f.render_widget(
            Paragraph::new(vec![
                Line::from("No audit yet."),
                Line::from(Span::styled(
                    "Press s for a quick audit (as you, no sudo). It never changes anything.",
                    t.fg(t.muted),
                )),
            ]),
            area,
        );
        return;
    };
    let c = run.counts();
    let items = st.audit_items();
    let [head, list_area, detail_area] = Layout::vertical([
        Constraint::Length(2),
        Constraint::Percentage(45),
        Constraint::Min(4),
    ])
    .areas(area);
    f.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(format!("Score {} {}", c.score(), c.grade()), t.bold()),
            Span::styled(
                format!(
                    "   {} fail · {} warn · {} accepted · {} scan",
                    c.fail, c.warn, c.accepted, run.meta.mode
                ),
                t.fg(t.muted),
            ),
        ])),
        head,
    );
    if items.is_empty() {
        f.render_widget(
            Paragraph::new(Span::styled("No risks. Nice.", t.fg(t.green))),
            list_area,
        );
        return;
    }
    let rows: Vec<ListItem> = items
        .iter()
        .map(|i| {
            let (tag, color) = match (i.accepted, i.finding.sev) {
                (true, _) => ("✓ ACCEPTED", t.muted),
                (_, Severity::Fail) => ("✕ FAIL", t.red),
                _ => ("! WARN", t.yellow),
            };
            ListItem::new(Line::from(vec![
                Span::styled(
                    format!("{tag:<10}"),
                    t.fg(color).add_modifier(Modifier::BOLD),
                ),
                Span::raw(format!("  {}", i.title())),
            ]))
        })
        .collect();
    let mut ls = ListState::default().with_selected(Some(st.audit_sel));
    f.render_stateful_widget(
        List::new(rows).highlight_style(t.selected()),
        list_area,
        &mut ls,
    );
    if let Some(i) = items.get(st.audit_sel) {
        let fd = &i.finding;
        let mut lines = vec![
            Line::from(Span::styled(fd.section.clone(), t.fg(t.muted))),
            Line::from(Span::styled(i.title().to_owned(), t.bold())),
        ];
        lines.extend(
            fd.detail
                .lines()
                .take(4)
                .map(|l| Line::from(format!("  {l}"))),
        );
        if !fd.fix.is_empty() {
            lines.push(Line::from(Span::styled(
                format!("$ {}", fd.fix),
                t.fg(t.accent),
            )));
        }
        if !fd.hint.is_empty() {
            lines.push(Line::from(fd.hint.clone()));
        }
        let block = Block::default()
            .borders(Borders::TOP)
            .border_style(t.fg(t.muted));
        f.render_widget(
            Paragraph::new(lines)
                .block(block)
                .wrap(Wrap { trim: false }),
            detail_area,
        );
    }
}

fn backup(f: &mut Frame, st: &State<'_>, area: Rect) {
    let t = &st.theme;
    let Some(choices) = &st.choices else {
        let msg = if st.jobs.contains("choices") {
            "detecting what is on this machine…"
        } else {
            "Press r to detect what is on this machine."
        };
        f.render_widget(Paragraph::new(Span::styled(msg, t.fg(t.muted))), area);
        return;
    };
    let count = |c: &phx_contract::selection::Choice| c.items.iter().filter(|(_, on)| *on).count();
    if !st.in_items {
        let mut lines = vec![
            Line::from(Span::styled(
                "What snapshots capture. New items in a ticked category are included automatically.",
                t.fg(t.muted),
            )),
            Line::from(""),
        ];
        let rows: Vec<ListItem> = choices
            .iter()
            .map(|(c, _)| {
                let n = count(c);
                let box_ = if n == c.items.len() && n > 0 {
                    "[x]"
                } else if n > 0 {
                    "[-]"
                } else {
                    "[ ]"
                };
                let mut spans = vec![Span::raw(format!(
                    "{box_} {:<10} {n} of {}",
                    c.category,
                    c.items.len()
                ))];
                if c.category == "secret" {
                    spans.push(Span::styled(
                        "  encrypted; 6 Settings → e first",
                        t.fg(t.muted),
                    ));
                }
                ListItem::new(Line::from(spans))
            })
            .collect();
        let [head, list_area] =
            Layout::vertical([Constraint::Length(2), Constraint::Min(3)]).areas(area);
        lines.truncate(1);
        f.render_widget(Paragraph::new(lines), head);
        let mut ls = ListState::default().with_selected(Some(st.cat_sel));
        f.render_stateful_widget(
            List::new(rows).highlight_style(t.selected()),
            list_area,
            &mut ls,
        );
        return;
    }
    let Some((c, notes)) = choices.get(st.cat_sel) else {
        return;
    };
    let [head, list_area] =
        Layout::vertical([Constraint::Length(2), Constraint::Min(3)]).areas(area);
    f.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(c.category.clone(), t.bold()),
            Span::styled(
                format!("  {} of {} captured", count(c), c.items.len()),
                t.fg(t.muted),
            ),
        ])),
        head,
    );
    let rows: Vec<ListItem> = c
        .items
        .iter()
        .zip(notes)
        .map(|((id, on), note)| {
            let mut spans = vec![Span::raw(format!(
                "{} {id}",
                if *on { "[x]" } else { "[ ]" }
            ))];
            if !note.is_empty() {
                spans.push(Span::styled(format!("  {note}"), t.fg(t.muted)));
            }
            ListItem::new(Line::from(spans))
        })
        .collect();
    let mut ls = ListState::default().with_selected(Some(st.item_sel));
    f.render_stateful_widget(
        List::new(rows).highlight_style(t.selected()),
        list_area,
        &mut ls,
    );
}

fn migrate(f: &mut Frame, st: &State<'_>, area: Rect) {
    let t = &st.theme;
    let done = st.migrate_done();
    let mut lines = vec![
        heading(st, "Migrate to a new machine"),
        Line::from(Span::styled(
            "Audit → clean → snapshot → restore → re-audit. Quit any time; progress is kept.",
            t.fg(t.muted),
        )),
        Line::from(""),
    ];
    for (i, step) in MIGRATE_STEPS.iter().enumerate() {
        let mark = if done.get(i).copied().unwrap_or(false) {
            "✓"
        } else {
            " "
        };
        let cursor = if i == st.migrate_sel { "›" } else { " " };
        let style = if i == st.migrate_sel {
            t.selected()
        } else {
            ratatui::style::Style::new()
        };
        lines.push(Line::from(Span::styled(
            format!("{cursor} {mark} {}. {step}", i + 1),
            style,
        )));
    }
    lines.push(Line::from(""));
    let explain = phx_core::migrate::explain(st.migrate_sel, &st.app.repo.remote());
    lines.extend(explain.lines().map(|l| Line::from(l.to_owned())));
    if st.output_title == "Re-audit" {
        lines.push(Line::from(""));
        lines.extend(st.output.iter().map(|l| Line::from(l.clone())));
    }
    f.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), area);
}

fn settings(f: &mut Frame, st: &State<'_>, area: Rect) {
    let t = &st.theme;
    let repo = &st.app.repo;
    let remote = repo.remote();
    let secrets = phx_core::category::secret::secrets_ready(&repo.dir);
    let lines = vec![
        heading(st, "Schedule"),
        Line::from(format!("  {}", st.timer.lines().next().unwrap_or_default())),
        Line::from(Span::styled(
            "  t turns automatic snapshots on (every 30m) or off",
            t.fg(t.muted),
        )),
        Line::from(""),
        heading(st, "Snapshot repo"),
        Line::from(format!(
            "  {}{}",
            repo.dir.display(),
            if repo.exists() {
                ""
            } else {
                "  (not set up: i)"
            }
        )),
        Line::from(if remote.is_empty() {
            "  ! no remote: snapshots die with this disk".to_owned()
        } else {
            format!("  remote {remote}")
        }),
        Line::from(""),
        heading(st, "Secrets"),
        Line::from(if secrets {
            "  ✓ SSH keys travel encrypted".to_owned()
        } else {
            "  ! SSH keys are not in snapshots (e sets up encryption)".to_owned()
        }),
        Line::from(""),
        heading(st, "Audit before backup"),
        Line::from(
            "  Risky files the audit finds are left out of snapshots (fail: excluded, warn: kept and noted).",
        ),
    ];
    f.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), area);
}
