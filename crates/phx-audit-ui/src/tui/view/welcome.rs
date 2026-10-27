//! The welcome screen shown before the first scan.

use crate::tui::app::{App, Focus};
use crate::tui::view::{put_lines, rounded, w16, wrap_spans};
use crate::tui::widgets::{self};
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::text::{Line, Span};

pub(super) fn welcome(buf: &mut Buffer, app: &App, area: Rect) {
    let p = &app.pal;
    widgets::topbar(buf, p, area, &app.host, &app.user, "");
    let bw = 80.min(area.width);
    let inner = usize::from(bw.saturating_sub(8));
    let mut lines = vec![
        Line::from(Span::styled("No scans yet", p.bold(p.fg))),
        Line::from(""),
    ];
    lines.extend(wrap_spans(
        &[Span::styled(
            "Security Audit checks packages, boot and disk encryption, kernel hardening, firewall, network exposure, SSH, accounts, permissions, persistence, containers, secrets in your home folder and rootkit indicators.",
            p.fg_(p.fg),
        )],
        inner,
    ));
    lines.push(Line::from(""));
    lines.extend(wrap_spans(
        &[Span::styled(
            "Everything is read-only. A full scan uses sudo only to read root-owned files.",
            p.fg_(p.muted),
        )],
        inner,
    ));
    let bh = w16(lines.len()) + 2 + 2 + 1 + 3;
    let body = Rect {
        y: area.y + 1,
        height: area.height.saturating_sub(2),
        ..area
    };
    let x = body.x + body.width.saturating_sub(bw) / 2;
    let y = body.y + body.height.saturating_sub(bh) / 2;
    rounded(
        buf,
        p,
        Rect {
            x,
            y,
            width: bw,
            height: bh,
        },
        p.accent,
        None,
    );
    put_lines(buf, x + 4, y + 2, w16(inner), w16(lines.len()), &lines);
    // Buttons: 16 wide, 1 apart, centred.
    let by = y + 2 + w16(lines.len()) + 1;
    let total = 16 * 3 + 2;
    let mut bx = x + 4 + w16(inner).saturating_sub(total) / 2;
    let focus = if let Focus::Welcome(i) = app.focus {
        Some(i)
    } else {
        None
    };
    for (i, (label, id)) in [
        ("Full scan", "btn-full"),
        ("Quick scan", "btn-quick"),
        ("Deep scan", "btn-deep"),
    ]
    .into_iter()
    .enumerate()
    {
        widgets::button(
            buf,
            p,
            Rect {
                x: bx,
                y: by,
                width: 16,
                height: 3,
            },
            label,
            i == 0,
            false,
            focus == Some(i),
        );
        let mut hits = app.hits.borrow_mut();
        for dy in 0..3 {
            hits.buttons.push((bx, by + dy, 16, id));
        }
        bx += 17;
    }
}
