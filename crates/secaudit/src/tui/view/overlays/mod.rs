//! Modal overlays: the shared dialog box and the confirm dialog.

mod chooser;
mod help;
mod toasts;

use crate::tui::app::App;
use crate::tui::theme::Pal;
use crate::tui::view::{put_lines, rounded, w16, wrap_spans};
use crate::tui::widgets::{self, pad, wrap};
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::text::{Line, Span};

pub(crate) use chooser::{chooser, palette};
pub(crate) use help::help_panel;
pub(crate) use toasts::toasts;

/// `#dialog`: width 84, round accent border, padding 1 2, on `$surface`.
pub(super) fn dialog(buf: &mut Buffer, p: &Pal, full: Rect, content_h: u16) -> Rect {
    let w = 84.min(full.width);
    let h = (content_h + 4).min(full.height);
    let r = Rect {
        x: full.x + (full.width - w) / 2,
        y: full.y + (full.height - h) / 2,
        width: w,
        height: h,
    };
    rounded(buf, p, r, p.accent, Some(p.surface));
    Rect {
        x: r.x + 3,
        y: r.y + 2,
        width: w.saturating_sub(6),
        height: h.saturating_sub(4),
    }
}

pub(super) fn confirm(buf: &mut Buffer, app: &App, full: Rect, c: &crate::tui::app::Confirm) {
    let p = &app.pal;
    let width = 78.min(usize::from(full.width.saturating_sub(6)));
    let mut lines = wrap_spans(&[Span::styled(c.title.clone(), p.bold(p.fg))], width);
    lines.push(Line::from(""));
    if !c.code.is_empty() {
        let mut code = Vec::new();
        for l in &c.code {
            let st = if l.starts_with("$ ") {
                p.fg_(p.accent)
            } else {
                p.bold(p.fg)
            };
            for wl in wrap(l, width.saturating_sub(2)) {
                code.push(Line::from(Span::styled(
                    pad(&format!(" {wl}"), width),
                    st.bg(p.boost),
                )));
            }
        }
        code.truncate(16);
        lines.extend(code);
    }
    if !c.note.is_empty() {
        lines.push(Line::from(""));
        lines.extend(wrap_spans(
            &[Span::styled(c.note.clone(), p.fg_(p.muted_on(p.surface)))],
            width,
        ));
    }
    let inner = dialog(buf, p, full, w16(lines.len()) + 4);
    put_lines(buf, inner.x, inner.y, inner.width, inner.height, &lines);
    let by = inner.y + w16(lines.len()) + 1;
    let ok = format!("{}  y", c.ok);
    let (w1, w2) = (16u16, w16(ok.chars().count() + 4).max(16));
    let x2 = inner.right().saturating_sub(w2);
    let x1 = x2.saturating_sub(1 + w1);
    widgets::button(
        buf,
        p,
        Rect {
            x: x1,
            y: by,
            width: w1,
            height: 3,
        },
        "Cancel  n",
        false,
        false,
        !c.on_ok,
    );
    widgets::button(
        buf,
        p,
        Rect {
            x: x2,
            y: by,
            width: w2,
            height: 3,
        },
        &ok,
        !c.danger,
        c.danger,
        c.on_ok,
    );
    let mut hits = app.hits.borrow_mut();
    for dy in 0..3 {
        hits.buttons.push((x1, by + dy, w1, "no"));
        hits.buttons.push((x2, by + dy, w2, "yes"));
    }
}
