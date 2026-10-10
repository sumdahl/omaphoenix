//! Textual's widgets, drawn the way Textual draws them (glyphs taken from
//! its source and from captures of the original app).

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};

use super::theme::{Pal, blend, sev_icon_label};

/// Writes a line clipped to `width` columns.
pub fn put(buf: &mut Buffer, x: u16, y: u16, width: u16, line: &Line<'_>) {
    if width == 0 || y >= buf.area.bottom() || x >= buf.area.right() {
        return;
    }
    let width = width.min(buf.area.right() - x);
    buf.set_line(x, y, line, width);
}

/// Fills a rectangle's background.
pub fn fill(buf: &mut Buffer, area: Rect, style: Style) {
    let area = area.intersection(buf.area);
    for y in area.top()..area.bottom() {
        for x in area.left()..area.right() {
            if let Some(c) = buf.cell_mut((x, y)) {
                // Replace, not patch: nothing from below (bold) leaks through.
                c.reset();
                c.set_style(style);
            }
        }
    }
}

/// Text cut to `width` characters with an ellipsis, as `clip()`.
pub fn clip(text: &str, width: usize) -> String {
    let t = text.replace('\n', " ");
    if t.chars().count() <= width {
        return t;
    }
    let mut s: String = t.chars().take(width.saturating_sub(1)).collect();
    s.push('…');
    s
}

/// Pads with spaces to `width` characters.
pub fn pad(text: &str, width: usize) -> String {
    let n = text.chars().count();
    if n >= width {
        text.to_owned()
    } else {
        format!("{text}{}", " ".repeat(width - n))
    }
}

/// Greedy word wrap to `width` columns.
pub fn wrap(text: &str, width: usize) -> Vec<String> {
    let mut out = Vec::new();
    for para in text.split('\n') {
        let mut line = String::new();
        for word in para.split(' ') {
            let (ll, wl) = (line.chars().count(), word.chars().count());
            if ll > 0 && ll + 1 + wl > width {
                out.push(std::mem::take(&mut line));
            }
            if wl > width {
                // Hard-break long tokens (paths).
                let mut w = word.chars().peekable();
                while w.peek().is_some() {
                    let room = width - line.chars().count();
                    let chunk: String = w.by_ref().take(room.max(1)).collect();
                    line.push_str(&chunk);
                    if w.peek().is_some() {
                        out.push(std::mem::take(&mut line));
                    }
                }
                continue;
            }
            if !line.is_empty() {
                line.push(' ');
            }
            line.push_str(word);
        }
        out.push(line);
    }
    out
}

/// Textual's `Digits` font (3×3, regular weight).
pub fn digits(text: &str) -> [String; 3] {
    const FONT: &[(char, [&str; 3])] = &[
        (' ', ["   ", "   ", "   "]),
        ('0', ["╭─╮", "│ │", "╰─╯"]),
        ('1', ["╶╮ ", " │ ", "╶┴╴"]),
        ('2', ["╶─╮", "┌─┘", "╰─╴"]),
        ('3', ["╶─╮", " ─┤", "╶─╯"]),
        ('4', ["╷ ╷", "╰─┤", "  ╵"]),
        ('5', ["╭─╴", "╰─╮", "╶─╯"]),
        ('6', ["╭─╴", "├─╮", "╰─╯"]),
        ('7', ["╶─┐", "  │", "  ╵"]),
        ('8', ["╭─╮", "├─┤", "╰─╯"]),
        ('9', ["╭─╮", "╰─┤", "╶─╯"]),
        ('-', ["   ", "╶─╴", "   "]),
    ];
    let mut rows = [String::new(), String::new(), String::new()];
    for ch in text.chars() {
        let glyph = FONT
            .iter()
            .find(|(c, _)| *c == ch)
            .map_or([" ", " ", " "], |(_, g)| *g);
        for (r, g) in rows.iter_mut().zip(glyph) {
            r.push_str(g);
        }
    }
    rows
}

/// Textual's `Sparkline`: `width` buckets of `height` rows, max per bucket,
/// coloured from `min` to `max` by height.
pub fn sparkline(
    data: &[u32],
    width: usize,
    height: usize,
    min: Color,
    max: Color,
) -> Vec<Line<'static>> {
    const BARS: [char; 8] = ['▁', '▂', '▃', '▄', '▅', '▆', '▇', '█'];
    if width == 0 || height == 0 {
        return Vec::new();
    }
    if data.is_empty() {
        let mut v = vec![Line::from(""); height - 1];
        v.push(Line::from(Span::styled(
            "▁".repeat(width),
            Style::new().fg(min),
        )));
        return v;
    }
    if data.len() == 1 {
        return vec![Line::from(Span::styled("█".repeat(width), Style::new().fg(max))); height];
    }
    let (lo, hi) = (
        data.iter().copied().min().unwrap_or(0),
        data.iter().copied().max().unwrap_or(0),
    );
    let extent = f64::from((hi - lo).max(1));
    // Buckets as Fraction(len, width) partitions; empty ones are skipped.
    let n = data.len();
    let buckets: Vec<u32> = (0..width)
        .filter_map(|b| {
            let (s, e) = (n * b / width, n * (b + 1) / width);
            data.get(s..e).and_then(|p| p.iter().copied().max())
        })
        .collect();
    let segments = BARS.len() * height - 1;
    let mut lines = Vec::new();
    for row in (0..height).rev() {
        let (low, high) = (row * BARS.len(), (row + 1) * BARS.len());
        let mut spans = Vec::new();
        #[allow(clippy::cast_precision_loss, reason = "bucket counts are small")]
        let step = buckets.len() as f64 / width as f64;
        let mut idx = 0.0_f64;
        for _ in 0..width {
            #[allow(
                clippy::cast_possible_truncation,
                clippy::cast_sign_loss,
                reason = "non-negative index"
            )]
            let v = buckets.get(idx as usize).copied().unwrap_or(lo);
            let ratio = f64::from(v - lo) / extent;
            #[allow(
                clippy::cast_possible_truncation,
                clippy::cast_sign_loss,
                clippy::cast_precision_loss,
                reason = "0..=segments"
            )]
            let bar = (ratio * segments as f64) as usize;
            #[allow(clippy::cast_possible_truncation, reason = "0..=1")]
            let colour = blend(max, min, ratio as f32);
            if bar < low {
                spans.push(Span::raw(" "));
            } else if bar >= high {
                spans.push(Span::styled("█", Style::new().fg(colour)));
            } else {
                spans.push(Span::styled(
                    BARS.get(bar % BARS.len())
                        .map_or(" ".to_owned(), char::to_string),
                    Style::new().fg(colour),
                ));
            }
            idx += step;
        }
        lines.push(Line::from(spans));
    }
    lines
}

/// A severity badge, as `sev_badge()`: "✗ FAIL".
pub fn badge(p: &Pal, sev: &str) -> Span<'static> {
    let (icon, label) = sev_icon_label(sev);
    Span::styled(format!("{icon} {label}"), p.bold(p.sev(sev)))
}

/// Tabs: labels with 1-cell padding, and Textual's underline bar beneath.
/// When they overflow, the strip scrolls to centre the active tab, as
/// Textual's `Tabs` does. Returns each tab's visible x range, for clicks.
pub fn tabs(
    buf: &mut Buffer,
    p: &Pal,
    area: Rect,
    labels: &[String],
    active: usize,
    focused: bool,
) -> Vec<(u16, u16)> {
    let width = usize::from(area.width);
    let mut ranges = Vec::new();
    let mut cells: Vec<(char, Style)> = Vec::new();
    for (i, l) in labels.iter().enumerate() {
        let start = cells.len();
        let style = if i == active {
            let s = p.fg_(p.fg);
            if focused {
                s.add_modifier(Modifier::BOLD)
            } else {
                s
            }
        } else {
            p.fg_(p.tab_dim)
        };
        cells.push((' ', Style::new()));
        cells.extend(l.chars().map(|c| (c, style)));
        cells.push((' ', Style::new()));
        ranges.push((start, cells.len()));
    }
    let total = cells.len();
    let (s, e) = ranges.get(active).copied().unwrap_or((0, 0));
    let offset = if total > width {
        usize::midpoint(s, e)
            .saturating_sub(width / 2)
            .min(total - width)
    } else {
        0
    };
    let row: Vec<Span> = cells
        .iter()
        .skip(offset)
        .take(width)
        .map(|(c, st)| Span::styled(c.to_string(), *st))
        .collect();
    put(buf, area.x, area.y, area.width, &Line::from(row));
    // Underline: dim bar, the active range bright with half-cell ends.
    let mut bar = Vec::new();
    for cx in offset..offset + width {
        let (glyph, colour) = if cx == s {
            ("╸", p.bar_dim)
        } else if cx > s && cx + 1 < e {
            ("━", p.accent)
        } else if cx + 1 == e {
            ("╺", p.bar_dim)
        } else {
            ("━", p.bar_dim)
        };
        bar.push(Span::styled(glyph, p.fg_(colour)));
    }
    put(buf, area.x, area.y + 1, area.width, &Line::from(bar));
    let to_screen =
        |c: usize| area.x + u16::try_from(c.saturating_sub(offset).min(width)).unwrap_or(0);
    ranges
        .into_iter()
        .map(|(a, b)| (to_screen(a), to_screen(b)))
        .collect()
}

/// Textual's `Input` with its tall border (3 rows).
pub fn input(buf: &mut Buffer, p: &Pal, area: Rect, value: &str, placeholder: &str, focused: bool) {
    let w = usize::from(area.width);
    if w < 6 || area.height < 3 {
        return;
    }
    let edge = if focused { p.accent } else { p.input_border };
    let inner = w - 2;
    let bg = if focused { p.input_focus } else { p.input_bg };
    // The left edge is a ¾ block in the screen colour over the border colour.
    let left = Span::styled("▊", Style::new().fg(p.bg).bg(edge));
    let right = Span::styled("▎", p.fg_(edge).bg(p.bg));
    let row = |fill: &str| {
        Line::from(vec![
            left.clone(),
            Span::styled(fill.repeat(inner), p.fg_(edge).bg(bg)),
            right.clone(),
        ])
    };
    let mut mid = vec![left.clone(), Span::styled("  ", Style::new().bg(bg))];
    if value.is_empty() {
        let st = p.fg_(p.placeholder).bg(bg);
        let text: String = placeholder
            .chars()
            .chain(std::iter::repeat(' '))
            .take(inner - 2)
            .collect();
        if focused {
            // The cursor sits on the placeholder's first cell.
            let mut chars = text.chars();
            mid.push(Span::styled(
                chars.next().map_or(" ".into(), String::from),
                st.add_modifier(Modifier::REVERSED),
            ));
            mid.push(Span::styled(chars.collect::<String>(), st));
        } else {
            mid.push(Span::styled(text, st));
        }
    } else {
        let st = p.fg_(p.fg).bg(bg);
        let room = inner - 3;
        let shown: String = value
            .chars()
            .rev()
            .take(room)
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect();
        let used = shown.chars().count();
        mid.push(Span::styled(shown, st));
        // The cursor: a reversed cell after the text.
        mid.push(Span::styled(
            " ",
            if focused {
                st.add_modifier(Modifier::REVERSED)
            } else {
                st
            },
        ));
        mid.push(Span::styled(" ".repeat(inner - 2 - used - 1), st));
    }
    mid.push(right.clone());
    put(buf, area.x, area.y, area.width, &row("▔"));
    put(buf, area.x, area.y + 1, area.width, &Line::from(mid));
    put(buf, area.x, area.y + 2, area.width, &row("▁"));
}

/// A full Textual button (3 rows: ▔ top, label, ▁ bottom).
pub fn button(
    buf: &mut Buffer,
    p: &Pal,
    area: Rect,
    label: &str,
    primary: bool,
    danger: bool,
    focused: bool,
) {
    let w = usize::from(area.width);
    let bg = if danger {
        p.red
    } else if primary {
        p.accent
    } else {
        p.button_bg
    };
    let fg = if primary || danger { p.bg } else { p.fg };
    let mut text_style = Style::new().fg(fg).bg(bg).add_modifier(Modifier::BOLD);
    if focused {
        text_style = text_style.add_modifier(Modifier::REVERSED);
    }
    let lbl = clip(label, w);
    let left = (w.saturating_sub(lbl.chars().count())) / 2;
    let row = pad(&format!("{}{lbl}", " ".repeat(left)), w);
    put(
        buf,
        area.x,
        area.y,
        area.width,
        &Line::from(Span::styled(
            "▔".repeat(w),
            Style::new()
                .fg(blend(Color::Rgb(255, 255, 255), bg, 0.2))
                .bg(bg),
        )),
    );
    put(
        buf,
        area.x,
        area.y + 1,
        area.width,
        &Line::from(Span::styled(row, text_style)),
    );
    put(
        buf,
        area.x,
        area.y + 2,
        area.width,
        &Line::from(Span::styled(
            "▁".repeat(w),
            Style::new().fg(blend(Color::Rgb(0, 0, 0), bg, 0.3)).bg(bg),
        )),
    );
}

/// A compact button (one row, " label ").
pub fn compact(p: &Pal, label: &str, primary: bool) -> Span<'static> {
    let (bg, fg) = if primary {
        (p.accent, blend(Color::Rgb(0, 0, 0), p.accent, 0.87))
    } else {
        (p.button_bg, p.fg)
    };
    Span::styled(
        format!("  {label}  "),
        Style::new().fg(fg).bg(bg).add_modifier(Modifier::BOLD),
    )
}

/// The one-line top bar, as `TopBar`.
pub fn topbar(buf: &mut Buffer, p: &Pal, area: Rect, host: &str, user: &str, right: &str) {
    fill(buf, Rect { height: 1, ..area }, Style::new().bg(p.panel));
    let left = Line::from(vec![
        Span::styled(" ◆ ", p.bold(p.accent)),
        Span::styled("Security Audit", p.bold(p.fg)),
        Span::styled(format!("   {host} · {user}"), p.fg_(p.muted)),
    ]);
    let lw = u16::try_from(left.width()).unwrap_or(0);
    put(buf, area.x, area.y, area.width, &left);
    let rw = u16::try_from(right.chars().count()).unwrap_or(0);
    let rx = area.right().saturating_sub(rw + 1).max(area.x + lw + 1);
    put(
        buf,
        rx,
        area.y,
        area.right().saturating_sub(rx),
        &Line::from(Span::styled(right.to_owned(), p.fg_(p.muted))),
    );
}

/// One footer binding.
#[allow(clippy::struct_field_names, reason = "Textual's names: key, label")]
pub struct Key {
    pub key: &'static str,
    pub label: &'static str,
    pub enabled: bool,
}

/// Textual's `Footer`: " key label " pairs, "▏^p palette" at the right.
pub fn footer(buf: &mut Buffer, p: &Pal, area: Rect, keys: &[Key]) {
    let row = Rect {
        y: area.bottom().saturating_sub(1),
        height: 1,
        ..area
    };
    fill(buf, row, Style::new().bg(p.panel));
    let mut spans = Vec::new();
    for k in keys {
        let key_style = if k.enabled {
            p.bold(p.accent)
        } else {
            p.bold(p.key_dim)
        };
        let label_style = if k.enabled {
            p.fg_(p.fg)
        } else {
            p.fg_(p.label_dim)
        };
        spans.push(Span::styled(format!(" {} ", k.key), key_style.bg(p.panel)));
        spans.push(Span::styled(
            format!("{} ", k.label),
            label_style.bg(p.panel),
        ));
    }
    let palette = Line::from(vec![
        Span::styled("▏", p.fg_(p.divider).bg(p.panel)),
        Span::styled("^p", p.bold(p.accent).bg(p.panel)),
        Span::styled(" palette", p.fg_(p.fg).bg(p.panel)),
    ]);
    let pw = 12;
    put(
        buf,
        row.x,
        row.y,
        row.width.saturating_sub(pw),
        &Line::from(spans),
    );
    if row.width > pw {
        put(buf, row.right() - pw, row.y, pw - 1, &palette);
    }
}

/// A Textual toast, bottom-right above the footer.
#[allow(clippy::many_single_char_names, reason = "geometry: x, y, w, h")]
pub fn toast(
    buf: &mut Buffer,
    p: &Pal,
    area: Rect,
    title: Option<&str>,
    body: &str,
    edge: Color,
) -> u16 {
    let w = (area.width / 2).clamp(20, 60);
    let inner = usize::from(w - 3);
    let mut lines: Vec<Line> = Vec::new();
    if let Some(t) = title {
        lines.push(Line::from(Span::styled(t.to_owned(), p.bold(p.fg))));
    }
    for l in wrap(body, inner) {
        lines.push(Line::from(Span::styled(l, p.fg_(p.fg))));
    }
    let h = u16::try_from(lines.len()).unwrap_or(1) + 2;
    let x = area.right().saturating_sub(w + 1);
    let y = area.bottom().saturating_sub(h + 1);
    let r = Rect {
        x,
        y,
        width: w,
        height: h,
    };
    fill(buf, r, Style::new().bg(p.panel));
    for dy in 0..h {
        put(
            buf,
            x,
            y + dy,
            1,
            &Line::from(Span::styled("▌", p.fg_(edge).bg(p.panel))),
        );
    }
    for (i, l) in lines.iter().enumerate() {
        let dy = u16::try_from(i).unwrap_or(0) + 1;
        put(buf, x + 2, y + dy, w - 3, l);
    }
    h
}

/// Textual's `ScrollBarRender.render_bar`: one glyph and style per cell,
/// with eighth-cell glyphs at the thumb's ends.
pub fn bar_cells(
    p: &Pal,
    size: usize,
    virtual_size: usize,
    position: usize,
    vertical: bool,
) -> Vec<(&'static str, Style)> {
    const V: [&str; 8] = ["▁", "▂", "▃", "▄", "▅", "▆", "▇", " "];
    const H: [&str; 8] = ["▉", "▊", "▋", "▌", "▍", "▎", "▏", " "];
    let bars = if vertical { V } else { H };
    let (back, bar) = (p.track, p.thumb);
    let blank = Style::new().bg(back);
    if size == 0 || virtual_size <= size {
        return vec![(" ", blank); size];
    }
    #[allow(clippy::cast_precision_loss, reason = "terminal sizes")]
    let (sz, vs) = (size as f64, virtual_size as f64);
    let thumb = (sz / (vs / sz)).max(1.0);
    #[allow(clippy::cast_precision_loss, reason = "terminal sizes")]
    let pos = (sz - thumb) * (position as f64 / (vs - sz));
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "non-negative cell counts"
    )]
    let (start, end) = {
        let start = (pos * 8.0) as usize;
        (start, start + (thumb * 8.0).ceil() as usize)
    };
    let (si, sb) = (start / 8, start % 8);
    let (ei, eb) = (end / 8, end % 8);
    let mut cells = vec![(" ", blank); size];
    for c in cells.iter_mut().take(ei).skip(si) {
        *c = (" ", Style::new().bg(bar));
    }
    // The head and tail glyphs; "reverse" swaps the two colours.
    let (fwd, rev) = (Style::new().fg(bar).bg(back), Style::new().fg(back).bg(bar));
    if let Some(c) = cells.get_mut(si)
        && let Some(g) = bars.get(7 - sb).filter(|g| **g != " ")
    {
        *c = (g, if vertical { fwd } else { rev });
    }
    if let Some(c) = cells.get_mut(ei)
        && let Some(g) = bars.get(7 - eb).filter(|g| **g != " ")
    {
        *c = (g, if vertical { rev } else { fwd });
    }
    cells
}

/// A vertical scrollbar, 2 columns wide, over `height` rows of `total`.
pub fn scrollbar(
    buf: &mut Buffer,
    p: &Pal,
    x: u16,
    y: u16,
    height: u16,
    total: usize,
    offset: usize,
) {
    for (i, (g, st)) in bar_cells(p, usize::from(height), total, offset, true)
        .into_iter()
        .enumerate()
    {
        put(
            buf,
            x,
            y + u16::try_from(i).unwrap_or(0),
            2,
            &Line::from(Span::styled(g.repeat(2), st)),
        );
    }
}

/// A horizontal scrollbar row, `width` columns over `total`.
pub fn hscrollbar(buf: &mut Buffer, p: &Pal, x: u16, y: u16, width: u16, total: usize) {
    let spans: Vec<Span> = bar_cells(p, usize::from(width), total, 0, false)
        .into_iter()
        .map(|(g, st)| Span::styled(g, st))
        .collect();
    put(buf, x, y, width, &Line::from(spans));
}

#[cfg(test)]
mod tests {
    #[test]
    fn digits_use_textuals_font() {
        let d = super::digits("62");
        assert_eq!(
            d,
            [
                "╭─╴╶─╮".to_owned(),
                "├─╮┌─┘".to_owned(),
                "╰─╯╰─╴".to_owned()
            ],
            "as the original renders 62"
        );
    }

    #[test]
    fn clip_and_wrap() {
        assert_eq!(
            super::clip("SUID/SGID binaries not owned by any package", 38),
            "SUID/SGID binaries not owned by any p…"
        );
        assert_eq!(super::wrap("aa bb cc", 5), ["aa bb", "cc"]);
    }
}
