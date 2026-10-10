//! Backup: what snapshots capture.

use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::text::{Line, Span};
use ratatui::widgets::{List, ListItem, ListState, Paragraph, Wrap};

use phx_core::app::review_key;

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
                let new = new_in(st, &c.category, c.items.iter().map(|(id, _)| id));
                if new > 0 {
                    spans.push(Span::styled(
                        format!("  ● {new} new"),
                        t.bold().fg(t.yellow),
                    ));
                }
                if c.category == "secret" {
                    spans.push(Span::styled(
                        if phx_core::category::secret::secrets_ready(&st.app.repo.dir) {
                            "  encrypted"
                        } else {
                            "  encrypted; 6 Settings → e first"
                        },
                        t.fg(t.muted),
                    ));
                }
                ListItem::new(Line::from(spans))
            })
            .collect();
        let [head, list_area] =
            Layout::vertical([Constraint::Length(3), Constraint::Min(3)]).areas(area);
        f.render_widget(Paragraph::new(banner(st)).wrap(Wrap { trim: true }), head);
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
            if st.unreviewed.contains(&review_key(&c.category, id)) {
                spans.push(Span::styled("  ● new", t.bold().fg(t.yellow)));
            }
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

fn banner(st: &State<'_>) -> Line<'static> {
    let t = &st.theme;
    if !st.app.has_chosen() {
        return Line::from(Span::styled(
            "Choose what OmaPhoenix backs up. Everything found is ticked; untick what you don't want, then press w. Nothing is saved until you do.",
            t.bold().fg(t.yellow),
        ));
    }
    if !st.unreviewed.is_empty() {
        return Line::from(Span::styled(
            format!(
                "{} new item(s) found since your last review, marked ● new. Tick or untick them, then press w.",
                st.unreviewed.len()
            ),
            t.bold().fg(t.yellow),
        ));
    }
    Line::from(Span::styled(
        "What snapshots capture. You're asked about anything new before it's backed up.",
        t.fg(t.muted),
    ))
}

fn new_in<'a>(st: &State<'_>, category: &str, ids: impl Iterator<Item = &'a String>) -> usize {
    ids.filter(|id| st.unreviewed.contains(&review_key(category, id)))
        .count()
}
