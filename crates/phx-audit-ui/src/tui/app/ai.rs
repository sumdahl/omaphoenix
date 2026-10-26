//! AI explain and plan: setup gate, opening and keys of the AI screen.

use phx_audit::store::sev_name;
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use super::util::copy_to_clipboard;
use super::{App, Modal, Screen, Severity};

impl App {
    pub(super) fn explain(&mut self) {
        if let Some(n) = self
            .selected()
            .filter(|f| sev_name(f.finding.sev) != "pass")
            .map(|f| f.n)
        {
            self.with_ai(crate::tui::ai_screen::Then::Explain(n));
        }
    }

    /// `with_ai()`: runs `then` once AI is set up, after the one-time dialog if needed.
    pub(super) fn with_ai(&mut self, then: crate::tui::ai_screen::Then) {
        if crate::ai::ready() {
            self.open_ai(then);
        } else {
            self.modal = Some(Modal::AiSetup(crate::tui::ai_screen::AiSetup::new(then)));
        }
    }

    pub(super) fn open_ai(&mut self, then: crate::tui::ai_screen::Then) {
        use ratatui::text::Span;
        let Some(run) = self.run() else { return };
        let p = &self.pal;
        let bold = |s: ratatui::style::Style| s.add_modifier(ratatui::style::Modifier::BOLD);
        let (title, prompt, subject) = match then {
            crate::tui::ai_screen::Then::Explain(n) => {
                let Some(f) = run.items.iter().find(|i| i.n == n) else {
                    return;
                };
                let mut b = crate::tui::widgets::badge(p, sev_name(f.finding.sev));
                b.style = bold(b.style);
                let title = vec![
                    b,
                    Span::styled("  ", bold(p.fg_(p.fg))),
                    Span::styled(f.title().to_owned(), bold(p.fg_(p.fg))),
                    Span::styled(format!("   {}", f.finding.section), bold(p.fg_(p.muted))),
                ];
                (title, crate::ai::explain_prompt(f, run), "explain")
            }
            crate::tui::ai_screen::Then::Plan => {
                let c = run.counts();
                let n = c.fail + c.warn;
                let title = vec![
                    Span::styled("Action plan  ", bold(p.fg_(p.fg))),
                    Span::styled(
                        format!(
                            "{n} open issue{} · score {} {}",
                            if n == 1 { "" } else { "s" },
                            c.score(),
                            c.grade()
                        ),
                        bold(p.fg_(p.muted)),
                    ),
                ];
                (title, crate::ai::plan_prompt(run), "action plan")
            }
        };
        let view = crate::tui::ai_screen::AiView::open(&self.store.dir, title, prompt, subject);
        self.screen = Screen::Ai(Box::new(view));
    }

    pub(super) fn ai_key(&mut self, mut v: Box<crate::tui::ai_screen::AiView>, key: KeyEvent) {
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        match key.code {
            KeyCode::Esc => {
                v.cancel();
                return;
            }
            KeyCode::Char('r') if ctrl => v.regenerate(),
            KeyCode::Char('o') if ctrl && !v.last_answer.is_empty() => {
                copy_to_clipboard(&v.last_answer);
                self.notify("Answer copied", Severity::Information, 5);
            }
            KeyCode::Tab | KeyCode::BackTab => v.input_focus = !v.input_focus,
            KeyCode::Enter if v.input_focus => v.submit(),
            KeyCode::Backspace if v.input_focus && !v.busy => {
                v.input.pop();
            }
            KeyCode::Char(c) if v.input_focus && !v.busy && !ctrl => v.input.push(c),
            // With the answer focused, the app's keys work.
            KeyCode::Char('q') if !v.input_focus => {
                v.cancel();
                self.quit = true;
            }
            KeyCode::Char('?') if !v.input_focus => self.help = !self.help,
            KeyCode::Up => {
                v.follow = false;
                v.scroll = v.scroll.saturating_sub(1);
            }
            KeyCode::Down => v.scroll += 1,
            KeyCode::PageUp => {
                v.follow = false;
                v.scroll = v.scroll.saturating_sub(usize::from(self.size.1 / 2));
            }
            KeyCode::PageDown => v.scroll += usize::from(self.size.1 / 2),
            KeyCode::Home => {
                v.follow = false;
                v.scroll = 0;
            }
            KeyCode::End => v.follow = true,
            _ => {}
        }
        self.screen = Screen::Ai(v);
    }
}
