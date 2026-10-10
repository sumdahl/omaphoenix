//! Pulse: backup health at a glance.

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::Modifier;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Wrap};

use crate::tui::state::State;

pub(super) fn pulse(f: &mut Frame, st: &State<'_>, area: Rect) {
    let t = &st.theme;
    let mut lines = vec![super::heading(st, "Protection")];
    if st.app.repo.exists() {
        match st.snaps.first() {
            Some(s) => lines.push(Line::from(format!(
                "  Last snapshot {} ({})",
                super::ago(&s.time),
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
    lines.push(super::heading(st, "Health"));
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
