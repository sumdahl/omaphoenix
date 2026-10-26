//! Markdown as Textual's `Markdown` widget lays it out in the original's AI
//! screen (measured from captures): headings in the accent colour with two
//! rows above and one below, paragraphs and lists followed by a blank row,
//! fenced code on a darker band padded one row and two columns, inline
//! `code`, **bold** and *italic*. Enough for Claude's answers, which the
//! system prompt keeps to headings, paragraphs, lists and code.

use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};

use super::theme::{Pal, blend};
use super::view::wrap_spans;

/// A rendered block and the blank rows it wants above and below.
struct Block {
    lines: Vec<Line<'static>>,
    above: usize,
    below: usize,
}

/// Inline spans: `code`, **bold**, *italic*.
fn inline(p: &Pal, text: &str, base: Style) -> Vec<Span<'static>> {
    let code = Style::new()
        .fg(blend(p.yellow, p.fg, 0.6))
        .bg(blend(p.yellow, p.bg, 0.08));
    let mut out = Vec::new();
    let mut buf = String::new();
    let (mut bold, mut italic) = (false, false);
    let style = |bold: bool, italic: bool| {
        let mut s = base;
        if bold {
            s = s.add_modifier(Modifier::BOLD);
        }
        if italic {
            s = s.add_modifier(Modifier::ITALIC);
        }
        s
    };
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '`' => {
                let code_text: String = chars.by_ref().take_while(|c| *c != '`').collect();
                if !buf.is_empty() {
                    out.push(Span::styled(std::mem::take(&mut buf), style(bold, italic)));
                }
                out.push(Span::styled(code_text, code));
            }
            '*' | '_' if chars.peek() == Some(&c) => {
                chars.next();
                if !buf.is_empty() {
                    out.push(Span::styled(std::mem::take(&mut buf), style(bold, italic)));
                }
                bold = !bold;
            }
            '*' => {
                if !buf.is_empty() {
                    out.push(Span::styled(std::mem::take(&mut buf), style(bold, italic)));
                }
                italic = !italic;
            }
            _ => buf.push(c),
        }
    }
    if !buf.is_empty() {
        out.push(Span::styled(buf, style(bold, italic)));
    }
    out
}

/// Wraps with a hanging indent: `first` before the first line, spaces after.
fn hang(spans: &[Span<'static>], first: &Span<'static>, width: usize) -> Vec<Line<'static>> {
    let indent = first.content.chars().count();
    let mut lines = wrap_spans(spans, width.saturating_sub(indent).max(1));
    for (i, l) in lines.iter_mut().enumerate() {
        let lead = if i == 0 {
            first.clone()
        } else {
            Span::raw(" ".repeat(indent))
        };
        l.spans.insert(0, lead);
    }
    lines
}

/// Renders `md` to lines `width` columns wide.
#[allow(
    clippy::too_many_lines,
    reason = "one match arm per Markdown block kind"
)]
#[must_use]
pub fn render(p: &Pal, md: &str, width: usize) -> Vec<Line<'static>> {
    let text = Style::new().fg(p.fg);
    let marker = Style::new().fg(blend(Color::Rgb(255, 255, 255), p.accent, 0.335));
    let code_bg = blend(Color::Rgb(0, 0, 0), p.bg, 0.105);
    let mut blocks: Vec<Block> = Vec::new();
    let mut para: Vec<String> = Vec::new();
    let flush = |para: &mut Vec<String>, blocks: &mut Vec<Block>| {
        if !para.is_empty() {
            let joined = para.join(" ");
            para.clear();
            blocks.push(Block {
                lines: wrap_spans(&inline(p, &joined, text), width),
                above: 0,
                below: 1,
            });
        }
    };
    let mut lines = md.lines().peekable();
    while let Some(raw) = lines.next() {
        let line = raw.trim_end();
        let trimmed = line.trim_start();
        if trimmed.starts_with("```") {
            flush(&mut para, &mut blocks);
            let mut code = Vec::new();
            for l in lines.by_ref() {
                if l.trim_start().starts_with("```") {
                    break;
                }
                code.push(l.to_owned());
            }
            let band = |s: String| {
                let n = s.chars().count();
                Line::from(vec![
                    Span::styled(
                        format!("  {s}"),
                        Style::new().fg(Color::Rgb(255, 255, 255)).bg(code_bg),
                    ),
                    Span::styled(
                        " ".repeat(width.saturating_sub(n + 2)),
                        Style::new().bg(code_bg),
                    ),
                ])
            };
            let mut out = vec![band(String::new())];
            for c in code {
                let chars: Vec<char> = c.chars().collect();
                if chars.is_empty() {
                    out.push(band(String::new()));
                }
                for chunk in chars.chunks(width.saturating_sub(4).max(1)) {
                    out.push(band(chunk.iter().collect()));
                }
            }
            out.push(band(String::new()));
            blocks.push(Block {
                lines: out,
                above: 0,
                below: 1,
            });
            continue;
        }
        if trimmed.is_empty() {
            flush(&mut para, &mut blocks);
            continue;
        }
        let hashes = trimmed.chars().take_while(|c| *c == '#').count();
        if (1..=6).contains(&hashes) && trimmed.chars().nth(hashes) == Some(' ') {
            flush(&mut para, &mut blocks);
            let title = trimmed.get(hashes + 1..).unwrap_or_default();
            let style = if hashes == 1 {
                Style::new().fg(p.accent).add_modifier(Modifier::BOLD)
            } else {
                Style::new().fg(p.accent)
            };
            blocks.push(Block {
                lines: wrap_spans(&inline(p, title, style), width),
                above: 2,
                below: 1,
            });
            continue;
        }
        let bullet = trimmed
            .strip_prefix("- ")
            .or_else(|| trimmed.strip_prefix("* "))
            .or_else(|| trimmed.strip_prefix("+ "));
        let number = trimmed
            .split_once(". ")
            .filter(|(n, _)| !n.is_empty() && n.chars().all(|c| c.is_ascii_digit()));
        if bullet.is_some() || number.is_some() {
            flush(&mut para, &mut blocks);
            let depth = (line.len() - trimmed.len()) / 2;
            let pad = " ".repeat(depth * 2);
            let (lead, body) = match (bullet, number) {
                (Some(b), _) => (format!("{pad}• "), b.to_owned()),
                (None, Some((n, b))) => (format!("{pad}{n:>2}. "), b.to_owned()),
                _ => (String::new(), String::new()),
            };
            // Continuation lines of the item belong to it.
            let mut body = body;
            while let Some(next) = lines.peek() {
                let t = next.trim_start();
                if t.is_empty()
                    || t.starts_with("- ")
                    || t.starts_with("* ")
                    || t.starts_with("```")
                    || t.starts_with('#')
                    || t.split_once(". ").is_some_and(|(n, _)| {
                        !n.is_empty() && n.chars().all(|c| c.is_ascii_digit())
                    })
                {
                    break;
                }
                body.push(' ');
                body.push_str(t);
                lines.next();
            }
            let item = hang(&inline(p, &body, text), &Span::styled(lead, marker), width);
            // Items of one list sit together; the list is followed by a blank row.
            let joins = blocks
                .last()
                .is_some_and(|b| b.below == 1 && b.above == usize::MAX);
            if joins && let Some(last) = blocks.last_mut() {
                last.lines.extend(item);
            } else {
                blocks.push(Block {
                    lines: item,
                    above: usize::MAX,
                    below: 1,
                });
            }
            continue;
        }
        if let Some(q) = trimmed.strip_prefix('>') {
            flush(&mut para, &mut blocks);
            let quote = hang(
                &inline(p, q.trim_start(), Style::new().fg(p.muted)),
                &Span::styled("▌ ", Style::new().fg(p.muted)),
                width,
            );
            blocks.push(Block {
                lines: quote,
                above: 0,
                below: 1,
            });
            continue;
        }
        if trimmed.chars().all(|c| c == '-' || c == '*' || c == '_') && trimmed.len() >= 3 {
            flush(&mut para, &mut blocks);
            blocks.push(Block {
                lines: vec![Line::from(Span::styled(
                    "─".repeat(width),
                    Style::new().fg(p.panel),
                ))],
                above: 0,
                below: 1,
            });
            continue;
        }
        para.push(trimmed.to_owned());
    }
    flush(&mut para, &mut blocks);

    // Margins collapse: the gap between two blocks is the larger of the two.
    let mut out: Vec<Line<'static>> = Vec::new();
    let mut prev_below = 0;
    for b in blocks {
        let above = if b.above == usize::MAX { 0 } else { b.above };
        for _ in 0..above.max(prev_below) {
            out.push(Line::from(""));
        }
        out.extend(b.lines);
        prev_below = b.below;
    }
    // The last block's bottom margin is not drawn.
    let _ = prev_below;
    out
}
