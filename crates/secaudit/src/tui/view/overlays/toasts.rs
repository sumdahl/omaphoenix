//! Toast notifications.

use crate::tui::app::{App, Severity};
use crate::tui::widgets::{self};
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;

pub(crate) fn toasts(buf: &mut Buffer, app: &App, full: Rect) {
    let p = &app.pal;
    let mut bottom = full.bottom().saturating_sub(1);
    for t in app.toasts.iter().rev() {
        let edge = match t.severity {
            Severity::Information => p.green,
            Severity::Warning => p.yellow,
            Severity::Error => p.red,
        };
        let h = widgets::toast(
            buf,
            p,
            Rect {
                height: bottom.saturating_sub(full.y),
                ..full
            },
            t.title.as_deref(),
            &t.body,
            edge,
        );
        bottom = bottom.saturating_sub(h + 1);
        if bottom <= full.y + h {
            break;
        }
    }
}
