//! The log screen.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::text::{Line, Span};

use crate::tui::app::App;
use crate::tui::widgets::{self, put};

use super::w16;

pub(super) fn log(
    buf: &mut Buffer,
    app: &App,
    area: Rect,
    lines: &[String],
    scroll: usize,
    title: &str,
) {
    let p = &app.pal;
    widgets::topbar(
        buf,
        p,
        area,
        &app.host,
        &app.user,
        &format!("full output · {title}"),
    );
    let footer_y = area.bottom().saturating_sub(1);
    let r = Rect {
        x: area.x + 1,
        y: area.y + 1,
        width: area.width.saturating_sub(2),
        height: footer_y.saturating_sub(area.y + 1),
    };
    let widest = lines.iter().map(|l| l.chars().count()).max().unwrap_or(0);
    let hscroll = widest > usize::from(r.width) && r.height > 1;
    let h = r.height - u16::from(hscroll);
    let vscroll = lines.len() > usize::from(h);
    let w = r.width - if vscroll { 2 } else { 0 };
    for (i, l) in lines.iter().skip(scroll).take(usize::from(h)).enumerate() {
        let shown: String = l.chars().take(usize::from(w)).collect();
        put(
            buf,
            r.x,
            r.y + w16(i),
            w,
            &Line::from(Span::styled(shown, p.fg_(p.fg))),
        );
    }
    if vscroll {
        widgets::scrollbar(buf, p, r.x + w, r.y, h, lines.len(), scroll);
    }
    if hscroll {
        widgets::hscrollbar(buf, p, r.x, r.y + h, w, widest);
    }
}
