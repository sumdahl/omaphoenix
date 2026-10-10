//! The one-time AI setup dialog: consent, Claude Code or an API key.

#![allow(
    clippy::too_many_lines,
    reason = "one function per screen, mirroring compose() in the original"
)]

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::text::{Line, Span};

use crate::tui::app::App;
use crate::tui::widgets::{self, put};

use super::overlays::dialog;
use super::{put_lines, w16, wrap_spans};

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
