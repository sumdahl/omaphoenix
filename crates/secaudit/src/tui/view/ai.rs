//! The AI screen and its one-time setup dialog.

#![allow(
    clippy::many_single_char_names,
    reason = "drawing code: p palette, r rect, x/y/w/h"
)]
#![allow(
    clippy::too_many_lines,
    reason = "one function per screen, mirroring compose() in the original"
)]

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};

use crate::tui::app::App;
use crate::tui::widgets::{self, put};

use super::overlays::dialog;
use super::{put_lines, rounded, w16, wrap_spans};

/// `AIScreen`: the answer in a bordered scroll box, status, follow-up input.
pub(super) fn ai_view(buf: &mut Buffer, app: &App, area: Rect, v: &crate::tui::ai_screen::AiView) {
    use crate::tui::ai_screen::{Part, Status};
    let p = &app.pal;
    widgets::topbar(
        buf,
        p,
        area,
        &app.host,
        &app.user,
        &format!("AI · {} · {}", v.subject, v.via()),
    );
    let footer_y = area.bottom().saturating_sub(1);
    // #ai-box: padding 1 2 0 2
    let x = area.x + 2;
    let w = area.width.saturating_sub(4);
    put(buf, x, area.y + 2, w, &Line::from(v.title.clone()));
    put(
        buf,
        x,
        area.y + 3,
        w,
        &Line::from(Span::styled(
            "AI-generated. Review commands before running them; secaudit's own fixes are on the main screen.",
            p.fg_(p.muted).add_modifier(Modifier::ITALIC),
        )),
    );
    // Scroll box: from row 5 to above status (1), input (3), margin (1), footer.
    let top = area.y + 5;
    let bottom = footer_y.saturating_sub(5);
    if bottom <= top + 2 {
        return;
    }
    // Textual keeps a 2-column scrollbar gutter outside the scroll box.
    let r = Rect {
        x,
        y: top,
        width: w.saturating_sub(2),
        height: bottom - top,
    };
    rounded(buf, p, r, p.panel, None);
    // Inside: padding 0 1, the Markdown's own padding 0 2, a scrollbar.
    let content_x = r.x + 4;
    let content_w = usize::from(r.width.saturating_sub(10));
    let h = usize::from(r.height - 2);
    let mut lines: Vec<Line<'static>> = Vec::new();
    let question = |q: &str, lines: &mut Vec<Line<'static>>| {
        lines.push(Line::from(""));
        for (i, l) in wrap_spans(&[Span::styled(q.to_owned(), p.fg_(p.fg))], content_w + 2)
            .into_iter()
            .enumerate()
        {
            let mut spans = if i == 0 {
                vec![Span::styled("You  ", p.bold(p.accent))]
            } else {
                vec![]
            };
            spans.extend(l.spans);
            lines.push(Line::from(spans).patch_style(Style::default()));
        }
        lines.push(Line::from(""));
    };
    let indent = |l: Line<'static>| {
        let mut spans = vec![Span::raw("  ")];
        spans.extend(l.spans);
        Line::from(spans)
    };
    for part in &v.parts {
        match part {
            Part::Question(q) => question(q, &mut lines),
            Part::Answer(a) => lines.extend(
                crate::tui::markdown::render(p, a, content_w)
                    .into_iter()
                    .map(indent),
            ),
        }
    }
    if let Some(text) = v.streaming().filter(|t| !t.is_empty()) {
        lines.extend(
            crate::tui::markdown::render(p, text, content_w)
                .into_iter()
                .map(indent),
        );
    }
    let max = lines.len().saturating_sub(h);
    let off = if v.follow { max } else { v.scroll.min(max) };
    put_lines(
        buf,
        content_x - 2,
        r.y + 1,
        r.width.saturating_sub(6),
        w16(h),
        lines.get(off..).unwrap_or_default(),
    );
    if lines.len() > h {
        // Textual counts one more row in the scroll height than it draws.
        let virtual_h = lines.len() + 1;
        widgets::scrollbar(buf, p, r.right() - 4, r.y + 1, w16(h), virtual_h, off);
    }
    // Status (margin 0 1).
    let muted = p.fg_(p.muted);
    let status = match &v.status {
        Status::Thinking => Line::from(Span::styled(
            "Claude is thinking…",
            muted.add_modifier(Modifier::ITALIC),
        )),
        Status::Writing => Line::from(Span::styled(
            "Writing…",
            muted.add_modifier(Modifier::ITALIC),
        )),
        Status::Saved => Line::from(vec![
            Span::styled("saved answer", muted),
            Span::styled("  ·  ctrl+r", p.bold(p.accent)),
            Span::styled(" to ask again", muted),
        ]),
        Status::Note(note, warn) => {
            let mut s = vec![Span::styled(note.clone(), muted)];
            if let Some(wn) = warn {
                s.push(Span::styled(wn.clone(), p.fg_(p.yellow)));
            }
            Line::from(s)
        }
        Status::Error(msg, retry) => Line::from(vec![
            Span::styled("✗ ", p.bold(p.red)),
            Span::styled(msg.clone(), p.fg_(p.red)),
            Span::styled(if *retry { "  ·  ctrl+r to retry" } else { "" }, muted),
        ]),
        Status::Empty => Line::from(""),
    };
    put(buf, x + 1, bottom, w.saturating_sub(2), &status);
    widgets::input(
        buf,
        p,
        Rect {
            x,
            y: bottom + 1,
            width: w,
            height: 3,
        },
        &v.input,
        "Ask a follow-up, e.g. \"what breaks if I do this?\"  (enter to send)",
        !v.busy && v.input_focus,
    );
}

/// `AISetupScreen`: one-time consent, Claude Code or an API key.
pub(super) fn ai_setup(
    buf: &mut Buffer,
    app: &App,
    full: Rect,
    s: &crate::tui::ai_screen::AiSetup,
) {
    let p = &app.pal;
    let width = 78.min(usize::from(full.width.saturating_sub(6)));
    let muted = p.fg_(p.muted);
    let plain = p.fg_(p.fg);
    let mut body: Vec<Span<'static>> = if s.mode == "claude-code" {
        vec![
            Span::styled("Explanations come from ", plain),
            Span::styled("Claude Code", p.bold(p.fg)),
            Span::styled(
                " using your existing sign-in, so no API key is needed. It runs with every tool, MCP server and hook turned off, so it can only answer.\n\n",
                plain,
            ),
            Span::styled(
                "When you ask, the finding (section, message, details and suggested fix) is sent to Claude. Your username, hostname and home path are replaced first, and the scan never collects secret values.\n\n",
                plain,
            ),
            Span::styled(
                "Nothing is sent until you press i or I. Usage counts toward your Claude plan.",
                muted,
            ),
        ]
    } else {
        vec![
            Span::styled(
                "When you ask, the finding (section, message, details and suggested fix) is sent to Anthropic's API so Claude can explain it. Your username, hostname and home path are replaced first, and the scan never collects secret values.\n\n",
                plain,
            ),
            Span::styled("Nothing is sent until you press i or I. ", muted),
            Span::styled(
                format!(
                    "Model: {}. API usage is billed to your Anthropic account.",
                    crate::ai::model()
                ),
                muted,
            ),
        ]
    };
    let asks = s.asks_key();
    if asks {
        let home = std::env::var("HOME").unwrap_or_default();
        body.push(Span::styled("\n\nPaste an API key from ", plain));
        body.push(Span::styled("console.anthropic.com", p.bold(p.accent)));
        body.push(Span::styled(" → API keys. It's saved to ", plain));
        body.push(Span::styled(
            crate::ai::key_file()
                .display()
                .to_string()
                .replacen(&home, "~", 1),
            p.fg_(p.accent),
        ));
        body.push(Span::styled(
            " readable only by you. Setting ANTHROPIC_API_KEY instead also works.",
            plain,
        ));
    }
    let mut lines = vec![
        Line::from(Span::styled("Explain findings with Claude", p.bold(p.fg))),
        Line::from(""),
    ];
    lines.extend(wrap_spans(&body, width));
    let text_h = lines.len();
    // key field (margin-top 1, 3 rows), error (margin-top 1), buttons (margin-top 1, 3 rows)
    let key_h = if asks { 4 } else { 0 };
    let inner = dialog(buf, p, full, w16(text_h + key_h + 2 + 4));
    put_lines(buf, inner.x, inner.y, inner.width, inner.height, &lines);
    let mut y = inner.y + w16(text_h);
    if asks {
        let masked: String = "•".repeat(s.key.chars().count());
        widgets::input(
            buf,
            p,
            Rect {
                x: inner.x,
                y: y + 1,
                width: inner.width,
                height: 3,
            },
            &masked,
            "sk-ant-…",
            s.focus == 0,
        );
        y += 4;
    }
    put(
        buf,
        inner.x,
        y + 1,
        inner.width,
        &Line::from(Span::styled(s.error.clone(), p.fg_(p.red))),
    );
    let by = y + 3;
    let buttons = s.buttons();
    let widths: Vec<u16> = buttons
        .iter()
        .map(|(_, l, _)| w16(l.chars().count() + 2).max(16))
        .collect();
    let total: u16 = widths.iter().sum::<u16>() + w16(buttons.len().saturating_sub(1));
    let mut bx = inner.right().saturating_sub(total);
    let first_button = usize::from(asks);
    let mut hits = app.hits.borrow_mut();
    for (i, ((id, label, primary), bw)) in buttons.iter().zip(&widths).enumerate() {
        widgets::button(
            buf,
            p,
            Rect {
                x: bx,
                y: by,
                width: *bw,
                height: 3,
            },
            label,
            *primary,
            false,
            s.focus == first_button + i,
        );
        for dy in 0..3 {
            hits.buttons.push((bx, by + dy, *bw, id));
        }
        bx += bw + 1;
    }
}
