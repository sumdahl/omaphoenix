//! Centred boxes over the body: the keys panel and the command palette.

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};

use crate::tui::state::{Section, State};

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
pub(super) fn keys_overlay(f: &mut Frame, st: &State<'_>, body: Rect) {
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
            ("t", "next schedule"),
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
pub(super) fn palette_overlay(f: &mut Frame, st: &State<'_>, body: Rect, q: &str, sel: usize) {
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
