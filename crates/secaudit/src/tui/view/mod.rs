//! Drawing, laid out cell for cell as the Python app's Textual CSS lays it out.

mod ai;
mod history;
mod log;
mod main_screen;
mod overlays;
mod scan;

use ratatui::Frame;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};

use super::app::{App, Focus, Hits, Modal, Screen};
use super::theme::{Pal, blend};
use super::widgets::{self, Key, fill, put};

use ai::{ai_setup, ai_view};
use history::history;
use log::log;
use main_screen::{main_screen, welcome};
use overlays::{chooser, confirm, help_panel, palette, toasts};
use scan::scan_screen;

/// Width of the keys panel (`?`), its border included.
const HELP_W: u16 = 39;

fn w16(n: usize) -> u16 {
    u16::try_from(n).unwrap_or(u16::MAX)
}

fn mmss(sec: u64) -> String {
    format!("{}:{:02}", sec / 60, sec % 60)
}

/// Rich-style word wrap of styled text to `width` columns.
pub fn wrap_spans(spans: &[Span<'_>], width: usize) -> Vec<Line<'static>> {
    let width = width.max(1);
    let mut out = Vec::new();
    let mut para: Vec<(char, Style)> = Vec::new();
    let mut paras = Vec::new();
    for s in spans {
        for c in s.content.chars() {
            if c == '\n' {
                paras.push(std::mem::take(&mut para));
            } else {
                para.push((c, s.style));
            }
        }
    }
    paras.push(para);
    for p in paras {
        // Words with their trailing spaces.
        let mut words: Vec<Vec<(char, Style)>> = Vec::new();
        let mut cur = Vec::new();
        let mut in_space = false;
        for cell in p {
            if cell.0 != ' ' && in_space {
                words.push(std::mem::take(&mut cur));
                in_space = false;
            }
            if cell.0 == ' ' {
                in_space = true;
            }
            cur.push(cell);
        }
        words.push(cur);
        let mut line: Vec<(char, Style)> = Vec::new();
        for word in words {
            let bare = word.iter().rposition(|c| c.0 != ' ').map_or(0, |i| i + 1);
            if !line.is_empty() && line.len() + bare > width {
                while line.last().is_some_and(|c| c.0 == ' ') {
                    line.pop();
                }
                out.push(to_line(&std::mem::take(&mut line)));
            }
            for cell in word {
                if line.len() >= width {
                    if cell.0 == ' ' {
                        continue;
                    }
                    out.push(to_line(&std::mem::take(&mut line)));
                }
                line.push(cell);
            }
        }
        out.push(to_line(&line));
    }
    out
}

fn to_line(cells: &[(char, Style)]) -> Line<'static> {
    let mut spans: Vec<Span<'static>> = Vec::new();
    let mut text = String::new();
    let mut style = None;
    for (c, s) in cells {
        if style != Some(*s) && !text.is_empty() {
            spans.push(Span::styled(
                std::mem::take(&mut text),
                style.unwrap_or_default(),
            ));
        }
        style = Some(*s);
        text.push(*c);
    }
    if !text.is_empty() {
        spans.push(Span::styled(text, style.unwrap_or_default()));
    }
    Line::from(spans)
}

/// Draws `lines` from `y`, at most `h` rows.
fn put_lines(buf: &mut Buffer, x: u16, y: u16, w: u16, h: u16, lines: &[Line<'_>]) {
    for (i, l) in lines.iter().take(usize::from(h)).enumerate() {
        put(buf, x, y + w16(i), w, l);
    }
}

fn hline(buf: &mut Buffer, p: &Pal, x: u16, y: u16, w: u16) {
    put(
        buf,
        x,
        y,
        w,
        &Line::from(Span::styled("─".repeat(usize::from(w)), p.fg_(p.panel))),
    );
}

fn vline(buf: &mut Buffer, p: &Pal, x: u16, y: u16, h: u16) {
    for dy in 0..h {
        put(
            buf,
            x,
            y + dy,
            1,
            &Line::from(Span::styled("│", p.fg_(p.panel))),
        );
    }
}

/// Keeps `sel` inside a window of `h` rows starting at `offset`.
fn scrolled(offset: &std::cell::Cell<usize>, sel: usize, h: usize, total: usize) -> usize {
    let mut o = offset.get();
    if sel < o {
        o = sel;
    } else if h > 0 && sel >= o + h {
        o = sel + 1 - h;
    }
    o = o.min(total.saturating_sub(h));
    offset.set(o);
    o
}

pub fn draw(f: &mut Frame, app: &App) {
    let full = f.area();
    let buf = f.buffer_mut();
    let p = &app.pal;
    fill(buf, full, Style::new().bg(p.bg).fg(p.fg));
    *app.hits.borrow_mut() = Hits::default();
    let area = if app.help && full.width > HELP_W + 20 {
        Rect {
            width: full.width - HELP_W,
            ..full
        }
    } else {
        full
    };
    let footer_y = area.bottom().saturating_sub(1);
    match &app.screen {
        Screen::Main => match app.run() {
            Some(run) => main_screen(buf, app, area, run),
            None => welcome(buf, app, area),
        },
        Screen::Scan(scan) => scan_screen(buf, app, area, scan),
        Screen::History(sel) => history(buf, app, area, *sel),
        Screen::Log(lines, scroll, title) => log(buf, app, area, lines, *scroll, title),
        Screen::Ai(v) => ai_view(buf, app, area, v),
    }
    widgets::footer(
        buf,
        p,
        Rect {
            y: footer_y,
            height: 1,
            ..area
        },
        &footer_keys(app),
    );
    if app.help && area.width < full.width {
        help_panel(
            buf,
            app,
            Rect {
                x: area.right(),
                width: full.width - area.width,
                ..full
            },
        );
    }
    if let Some(m) = &app.modal {
        dim(buf, p, full);
        match m {
            Modal::Confirm(c) => confirm(buf, app, full, c),
            Modal::Chooser(i) => chooser(buf, app, full, *i),
            Modal::Palette(q, i) => palette(buf, app, full, q, *i),
            Modal::AiSetup(setup) => ai_setup(buf, app, full, setup),
        }
    }
    toasts(buf, app, full);
}

/// `background: $background 60%` over the screen behind a modal.
fn dim(buf: &mut Buffer, p: &Pal, area: Rect) {
    if !p.color {
        return;
    }
    for y in area.top()..area.bottom() {
        for x in area.left()..area.right() {
            if let Some(c) = buf.cell_mut((x, y)) {
                let fg = if c.fg == Color::Reset { p.fg } else { c.fg };
                let bg = if c.bg == Color::Reset { p.bg } else { c.bg };
                c.fg = blend(p.bg, fg, 0.6);
                c.bg = blend(p.bg, bg, 0.6);
            }
        }
    }
}

fn footer_keys(app: &App) -> Vec<Key> {
    let k = |key, label, enabled| Key {
        key,
        label,
        enabled,
    };
    if app.modal.is_some() {
        return vec![k("?", "Keys", true), k("q", "Quit", true)];
    }
    match &app.screen {
        Screen::Ai(v) => {
            let mut keys = vec![
                k("esc", "Back", true),
                k("^r", "Regenerate", true),
                k("^o", "Copy answer", true),
            ];
            // The input takes printable keys; with the answer focused, ? and q work.
            if !v.input_focus {
                keys.push(k("?", "Keys", true));
                keys.push(k("q", "Quit", true));
            }
            keys
        }
        Screen::Scan(_) => vec![
            k("esc", "Cancel scan", true),
            k("?", "Keys", true),
            k("q", "Quit", true),
        ],
        Screen::History(_) => vec![
            k("esc", "Back", true),
            k("del", "Delete scan", true),
            k("?", "Keys", true),
            k("q", "Quit", true),
        ],
        Screen::Log(..) => vec![k("esc", "Back", true), k("?", "Keys", true)],
        Screen::Main if app.focus == Focus::Search => vec![k("/", "Search", true)],
        Screen::Main => vec![
            k("s", "Scan", true),
            k("x", "Fix", app.can_run_fix()),
            k("space", "Select", app.can_mark()),
            k("a", "Accept risk", app.can_accept()),
            k("i", "Explain (AI)", app.can_explain()),
            k("/", "Search", app.current.is_some()),
            k("h", "History", !app.runs.is_empty()),
            k("?", "Keys", true),
            k("q", "Quit", true),
        ],
    }
}

/// A round border box, optionally filled.
fn rounded(buf: &mut Buffer, p: &Pal, r: Rect, edge: Color, bg: Option<Color>) {
    if r.width < 2 || r.height < 2 {
        return;
    }
    let st = bg.map_or_else(|| p.fg_(edge), |b| p.fg_(edge).bg(b));
    if let Some(b) = bg {
        fill(buf, r, Style::new().bg(b).fg(p.fg));
    }
    let w = usize::from(r.width - 2);
    put(
        buf,
        r.x,
        r.y,
        r.width,
        &Line::from(Span::styled(format!("╭{}╮", "─".repeat(w)), st)),
    );
    for dy in 1..r.height - 1 {
        put(buf, r.x, r.y + dy, 1, &Line::from(Span::styled("│", st)));
        put(
            buf,
            r.right() - 1,
            r.y + dy,
            1,
            &Line::from(Span::styled("│", st)),
        );
    }
    put(
        buf,
        r.x,
        r.bottom() - 1,
        r.width,
        &Line::from(Span::styled(format!("╰{}╯", "─".repeat(w)), st)),
    );
}

#[cfg(test)]
mod tests {
    use ratatui::text::Span;

    #[test]
    fn wraps_like_rich() {
        let l = super::wrap_spans(
            &[Span::raw(
                "Deep scan of omarchy-laptop-1 · took 1:26 · 25 notes · 0 skipped",
            )],
            56,
        );
        let t: Vec<String> = l.iter().map(ToString::to_string).collect();
        assert_eq!(
            t,
            [
                "Deep scan of omarchy-laptop-1 · took 1:26 · 25 notes · 0",
                "skipped"
            ]
        );
    }
}
