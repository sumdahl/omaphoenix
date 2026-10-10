//! Backup: what snapshots capture.

use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::text::{Line, Span};
use ratatui::widgets::{List, ListItem, ListState, Paragraph};

use crate::tui::state::State;

pub(super) fn backup(f: &mut Frame, st: &State<'_>, area: Rect) {
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
