//! Migrate and Settings.

use phx_core::schedule;
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Wrap};

use crate::tui::state::{MIGRATE_STEPS, State};

pub(super) fn migrate(f: &mut Frame, st: &State<'_>, area: Rect) {
    let t = &st.theme;
    let done = st.migrate_done();
    let mut lines = vec![
        super::heading(st, "Migrate to a new machine"),
        Line::from(Span::styled(
            "Audit → clean → snapshot → restore → re-audit. Quit any time; progress is kept.",
            t.fg(t.muted),
        )),
        Line::from(""),
    ];
    for (i, step) in MIGRATE_STEPS.iter().enumerate() {
        let mark = if done.get(i).copied().unwrap_or(false) {
            "✓"
        } else {
            " "
        };
        let cursor = if i == st.migrate_sel { "›" } else { " " };
        let style = if i == st.migrate_sel {
            t.selected()
        } else {
            ratatui::style::Style::new()
        };
        lines.push(Line::from(Span::styled(
            format!("{cursor} {mark} {}. {step}", i + 1),
            style,
        )));
    }
    lines.push(Line::from(""));
    let explain = phx_core::migrate::explain(st.migrate_sel, &st.app.repo.remote());
    lines.extend(explain.lines().map(|l| Line::from(l.to_owned())));
    if st.output_title == "Re-audit" {
        lines.push(Line::from(""));
        lines.extend(st.output.iter().map(|l| Line::from(l.clone())));
    }
    f.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), area);
}

fn interval_choices(st: &State<'_>) -> Line<'static> {
    let t = &st.theme;
    let mut spans = vec![Span::raw("  ")];
    let options = std::iter::once(None).chain(schedule::CHOICES.iter().map(|c| Some(*c)));
    for o in options {
        let label = o.unwrap_or("manual");
        spans.push(if o == st.interval {
            Span::styled(format!("[{label}]"), t.selected())
        } else {
            Span::styled(format!(" {label} "), t.fg(t.muted))
        });
        spans.push(Span::raw(" "));
    }
    Line::from(spans)
}

pub(super) fn settings(f: &mut Frame, st: &State<'_>, area: Rect) {
    let t = &st.theme;
    let repo = &st.app.repo;
    let remote = repo.remote();
    let secrets = phx_core::category::secret::secrets_ready(&repo.dir);
    let lines = vec![
        super::heading(st, "Schedule"),
        Line::from(format!("  {}", schedule::describe(st.interval))),
        interval_choices(st),
        Line::from(Span::styled(
            "  t switches to the next choice. Manual: press s whenever you want a snapshot.",
            t.fg(t.muted),
        )),
        Line::from(""),
        super::heading(st, "Snapshot repo"),
        Line::from(format!(
            "  {}{}",
            repo.dir.display(),
            if repo.exists() {
                ""
            } else {
                "  (not set up: i)"
            }
        )),
        Line::from(if remote.is_empty() {
            "  ! no remote: snapshots die with this disk".to_owned()
        } else {
            format!("  remote {remote}")
        }),
        Line::from(""),
        super::heading(st, "Secrets"),
        Line::from(if secrets {
            "  ✓ SSH keys travel encrypted".to_owned()
        } else {
            "  ! SSH keys are not in snapshots (e sets up encryption)".to_owned()
        }),
        Line::from(""),
        super::heading(st, "Audit before backup"),
        Line::from(
            "  Risky files the audit finds are left out of snapshots (fail: excluded, warn: kept and noted).",
        ),
    ];
    f.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), area);
}
