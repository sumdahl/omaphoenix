//! The activity line under the tabs, and the footer: what is happening, and what keys do.

use ratatui::text::{Line, Span};

use crate::tui::state::{Section, State};

pub(super) fn needs_review(st: &State<'_>) -> bool {
    st.app.repo.exists() && (!st.app.has_chosen() || !st.unreviewed.is_empty())
}

/// Always says what is going on: the running job and its step, or that nothing is.
pub(super) fn line(st: &State<'_>) -> Line<'static> {
    let t = &st.theme;
    if let Some(busy) = st.busy_label() {
        return Line::from(Span::styled(busy, t.bold().fg(t.accent)));
    }
    if st.timer_running {
        return Line::from(Span::styled(
            "⋯ the automatic snapshot is running in the background",
            t.bold().fg(t.accent),
        ));
    }
    if !st.app.repo.exists() {
        return Line::from(Span::styled(
            "Not set up yet: nothing is backed up. Press enter on Pulse to start.",
            t.fg(t.yellow),
        ));
    }
    let mut spans = vec![Span::styled("✓ idle", t.fg(t.green))];
    if !st.app.has_chosen() {
        spans.push(Span::styled(
            " · ● choose what to back up (4)",
            t.bold().fg(t.yellow),
        ));
    } else if !st.unreviewed.is_empty() {
        spans.push(Span::styled(
            format!(" · ● {} new item(s) to review (4)", st.unreviewed.len()),
            t.bold().fg(t.yellow),
        ));
    }
    match st.snaps.first() {
        Some(s) => spans.push(Span::styled(
            format!(" · last snapshot {}", super::ago(&s.time)),
            t.fg(t.muted),
        )),
        None => spans.push(Span::styled(" · no snapshot yet", t.fg(t.muted))),
    }
    let timer = st.timer.split(':').next().unwrap_or_default();
    spans.push(Span::styled(format!(" · {timer}"), t.fg(t.muted)));
    Line::from(spans)
}

/// A question when one is open, else the latest message, else the keys for this section.
pub(super) fn footer(st: &State<'_>) -> Line<'static> {
    let t = &st.theme;
    if let Some(q) = st
        .ask_snapshot
        .as_ref()
        .or(st.confirm.as_ref().map(|(q, _)| q))
    {
        return Line::from(Span::styled(q.clone(), t.bold().fg(t.yellow)));
    }
    if st.searching || !st.search.is_empty() {
        return Line::from(vec![
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
        ]);
    }
    if let Some((msg, _)) = &st.toast {
        return Line::from(Span::styled(msg.clone(), t.fg(t.accent)));
    }
    Line::from(Span::styled(hint(st), t.fg(t.muted)))
}

fn hint(st: &State<'_>) -> &'static str {
    match st.section {
        Section::Pulse => "enter primary action · s snapshot · d re-check · 1-6 sections · ? keys",
        Section::Timeline => "↑↓ move · enter preview · r restore · s snapshot · / search · ? keys",
        Section::Audit => "s scan · ↑↓ move · a accept · x run fix · / search · ? keys",
        Section::Backup if st.in_items => {
            "↑↓ move · space tick · a all/none · esc back · w save · q quit"
        }
        Section::Backup => "↑↓ move · enter open · space all/none · w save · 1-6 · q quit",
        Section::Migrate => "↑↓ step · enter do it · R start over · 1-6 sections · q quit",
        Section::Settings => "t timer on/off · e set up encrypted secrets · 1-6 sections · q quit",
    }
}
