//! Drawing, at 80×24 and up; every state is spelled in text, colour only reinforces it (DESIGN.md §12).

use ratatui::Frame;
use ratatui::layout::{Constraint, Layout};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use crate::tui::state::{Section, State};

mod audit;
mod backup;
mod overlays;
mod pulse;
mod setup;
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
        Section::Pulse => pulse::pulse(f, st, body),
        Section::Timeline => timeline::timeline(f, st, body),
        Section::Audit => audit::audit(f, st, body),
        Section::Backup => backup::backup(f, st, body),
        Section::Migrate => setup::migrate(f, st, body),
        Section::Settings => setup::settings(f, st, body),
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
        overlays::keys_overlay(f, st, body);
    }
    if let Some((q, sel)) = &st.palette {
        overlays::palette_overlay(f, st, body, q, *sel);
    }
}

fn heading(st: &State<'_>, text: &str) -> Line<'static> {
    Line::from(Span::styled(text.to_owned(), st.theme.bold()))
}
