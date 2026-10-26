//! Audit: the last scan's findings.

use phx_audit::Severity;
use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::Modifier;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, ListState, Paragraph, Wrap};

use crate::tui::state::State;

pub(super) fn audit(f: &mut Frame, st: &State<'_>, area: Rect) {
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
