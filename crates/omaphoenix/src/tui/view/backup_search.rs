//! Backup's search results: every category's matching items, best first, matched letters highlighted.

use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::Modifier;
use ratatui::text::{Line, Span};
use ratatui::widgets::{List, ListItem, ListState, Paragraph};

use crate::tui::state::{Hit, State};

pub(super) fn results(f: &mut Frame, st: &State<'_>, area: Rect) {
    let t = &st.theme;
    let hits = st.backup_hits();
    let [head, list_area] =
        Layout::vertical([Constraint::Length(2), Constraint::Min(3)]).areas(area);
    let summary = match hits.len() {
        0 => format!(
            "No item matches \u{201c}{}\u{201d}. Backspace to widen, esc to clear.",
            st.search
        ),
        n => format!("{n} match(es) across all categories. Space ticks or unticks; w saves."),
    };
    f.render_widget(Paragraph::new(Span::styled(summary, t.fg(t.muted))), head);
    let rows: Vec<ListItem> = hits.iter().map(|h| ListItem::new(row(st, h))).collect();
    let mut ls = ListState::default().with_selected((!hits.is_empty()).then_some(st.search_sel));
    f.render_stateful_widget(
        List::new(rows).highlight_style(t.selected()),
        list_area,
        &mut ls,
    );
}

fn row(st: &State<'_>, h: &Hit) -> Line<'static> {
    let t = &st.theme;
    let on = st
        .choices
        .as_ref()
        .and_then(|c| c.get(h.cat))
        .and_then(|(c, _)| c.items.get(h.item))
        .is_some_and(|(_, on)| *on);
    let mut spans = vec![Span::raw(if on { "[x] " } else { "[ ] " })];
    let marked = t.bold().fg(t.accent).add_modifier(Modifier::UNDERLINED);
    let mut run = String::new();
    let mut run_hit = false;
    for (i, c) in h.key.chars().enumerate() {
        let hit = h.m.positions.binary_search(&i).is_ok();
        if hit != run_hit && !run.is_empty() {
            spans.push(Span::styled(
                std::mem::take(&mut run),
                if run_hit {
                    marked
                } else {
                    ratatui::style::Style::new()
                },
            ));
        }
        run_hit = hit;
        run.push(c);
    }
    if !run.is_empty() {
        spans.push(Span::styled(
            run,
            if run_hit {
                marked
            } else {
                ratatui::style::Style::new()
            },
        ));
    }
    if st.unreviewed.contains(&h.key) {
        spans.push(Span::styled("  ● new", t.bold().fg(t.yellow)));
    }
    Line::from(spans)
}
