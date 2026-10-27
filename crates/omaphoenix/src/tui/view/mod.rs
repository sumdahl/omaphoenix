//! Drawing, at 80×24 and up; every state is spelled in text, colour only reinforces it (DESIGN.md §12).

use ratatui::Frame;
use ratatui::layout::{Constraint, Layout};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use crate::tui::state::{Section, State};

mod audit;
mod backup;
mod backup_search;
mod overlays;
mod pulse;
mod setup;
mod status;
mod timeline;

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
    let [tabs_row, status_row, body, foot] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Min(5),
        Constraint::Length(1),
    ])
    .areas(f.area());
    f.render_widget(Paragraph::new(tabs(st)), tabs_row);
    f.render_widget(Paragraph::new(status::line(st)), status_row);
    match st.section {
        Section::Pulse => pulse::pulse(f, st, body),
        Section::Timeline => timeline::timeline(f, st, body),
        Section::Audit => audit::audit(f, st, body),
        Section::Backup => backup::backup(f, st, body),
        Section::Migrate => setup::migrate(f, st, body),
        Section::Settings => setup::settings(f, st, body),
    }
    f.render_widget(Paragraph::new(status::footer(st)), foot);
    if st.help {
        overlays::keys_overlay(f, st, body);
    }
    if let Some((q, sel)) = &st.palette {
        overlays::palette_overlay(f, st, body, q, *sel);
    }
}

fn tabs(st: &State<'_>) -> Line<'static> {
    let t = &st.theme;
    let mut tabs = vec![Span::styled("OmaPhoenix ", t.bold().fg(t.accent))];
    for (i, s) in Section::ALL.iter().enumerate() {
        let label = format!(" {} {} ", i + 1, s.label());
        tabs.push(if *s == st.section {
            Span::styled(label, t.selected())
        } else {
            Span::styled(label, t.fg(t.muted))
        });
        if *s == Section::Backup && status::needs_review(st) {
            tabs.push(Span::styled("● ", t.bold().fg(t.yellow)));
        }
    }
    Line::from(tabs)
}

fn heading(st: &State<'_>, text: &str) -> Line<'static> {
    Line::from(Span::styled(text.to_owned(), st.theme.bold()))
}
