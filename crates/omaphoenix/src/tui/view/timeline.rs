//! Timeline: snapshots and the last command's output.

use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, ListState, Paragraph};

use crate::tui::state::State;

pub(super) fn timeline(f: &mut Frame, st: &State<'_>, area: Rect) {
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
                    Span::raw(format!("{}  {:<10}", s.id, super::ago(&s.time))),
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
