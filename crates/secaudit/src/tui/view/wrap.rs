//! Rich-style word wrap of styled spans.

use ratatui::style::Style;
use ratatui::text::{Line, Span};

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
