//! Modal keys, confirm outcomes and the command palette.

use ratatui::crossterm::event::{KeyCode, KeyEvent};

use super::{Action, App, CHOOSER, Confirm, Modal, Screen, Severity, Suspend};

impl App {
    pub(super) fn modal_key(&mut self, modal: Modal, key: KeyEvent) {
        match modal {
            Modal::AiSetup(mut setup) => {
                let on_key_field = setup.asks_key() && setup.focus == 0;
                match key.code {
                    KeyCode::Esc => {}
                    KeyCode::Tab | KeyCode::Right if !on_key_field || key.code == KeyCode::Tab => {
                        setup.focus = (setup.focus + 1) % setup.slots();
                        self.modal = Some(Modal::AiSetup(setup));
                    }
                    KeyCode::BackTab | KeyCode::Left
                        if !on_key_field || key.code == KeyCode::BackTab =>
                    {
                        setup.focus = (setup.focus + setup.slots() - 1) % setup.slots();
                        self.modal = Some(Modal::AiSetup(setup));
                    }
                    KeyCode::Enter if on_key_field => self.setup_button(setup, "ai-yes"),
                    KeyCode::Enter | KeyCode::Char(' ') if !on_key_field => {
                        let id = setup.focused_button().unwrap_or("ai-no");
                        self.setup_button(setup, id);
                    }
                    KeyCode::Backspace if on_key_field => {
                        setup.key.pop();
                        self.modal = Some(Modal::AiSetup(setup));
                    }
                    KeyCode::Char(c) if on_key_field => {
                        setup.key.push(c);
                        self.modal = Some(Modal::AiSetup(setup));
                    }
                    _ => self.modal = Some(Modal::AiSetup(setup)),
                }
            }
            Modal::Confirm(mut c) => match key.code {
                KeyCode::Char('y') => self.confirmed(c),
                KeyCode::Char('n') | KeyCode::Esc => {}
                KeyCode::Tab | KeyCode::BackTab | KeyCode::Left | KeyCode::Right => {
                    c.on_ok = !c.on_ok;
                    self.modal = Some(Modal::Confirm(c));
                }
                KeyCode::Enter | KeyCode::Char(' ') => {
                    if c.on_ok {
                        self.confirmed(c);
                    }
                }
                _ => self.modal = Some(Modal::Confirm(c)),
            },
            Modal::Chooser(i) => match key.code {
                KeyCode::Esc => {}
                KeyCode::Char('f') => self.start_scan("full"),
                KeyCode::Char('q') => self.start_scan("quick"),
                KeyCode::Char('d') => self.start_scan("deep"),
                KeyCode::Down => self.modal = Some(Modal::Chooser((i + 1).min(2))),
                KeyCode::Up => self.modal = Some(Modal::Chooser(i.saturating_sub(1))),
                KeyCode::Enter => self.start_scan(CHOOSER.get(i).map_or("full", |m| m.0)),
                _ => self.modal = Some(Modal::Chooser(i)),
            },
            Modal::Palette(mut q, mut i) => {
                let n = self.commands(&q).len();
                match key.code {
                    KeyCode::Esc => return,
                    KeyCode::Down => i = (i + 1).min(n.saturating_sub(1)),
                    KeyCode::Up => i = i.saturating_sub(1),
                    KeyCode::Backspace => {
                        q.pop();
                        i = 0;
                    }
                    KeyCode::Enter => {
                        if let Some(cmd) = self.commands(&q).get(i).map(|c| c.0) {
                            self.run_command(cmd);
                        }
                        return;
                    }
                    KeyCode::Char(c) => {
                        q.push(c);
                        i = 0;
                    }
                    _ => {}
                }
                self.modal = Some(Modal::Palette(q, i));
            }
        }
    }

    pub(super) fn confirmed(&mut self, c: Confirm) {
        match c.action {
            Action::RunFixes(ns) => self.suspend = Some(Suspend::Fixes(ns)),
            Action::DeleteRun(path) => {
                let _ = std::fs::remove_dir_all(&path);
                self.runs = self.store.runs();
                self.notify("Scan deleted", Severity::Information, 5);
                if let Screen::History(sel) = self.screen {
                    self.screen = Screen::History(sel.min(self.runs.len().saturating_sub(1)));
                }
            }
        }
    }

    /// Palette commands matching `query` (subsequence, any case), sorted by name: (name, help).
    pub fn commands(&self, query: &str) -> Vec<(&'static str, &'static str)> {
        let mut all = vec![
            (
                "Deep scan",
                "Full scan plus package integrity and whole-disk SUID",
            ),
            ("Full scan", "Run every check with sudo"),
            (
                "Keys",
                if self.help {
                    "Hide the keys and widget help panel"
                } else {
                    "Show help for the focused widget and a summary of available keys"
                },
            ),
            ("Quick scan", "User-level checks without sudo"),
            ("Quit", "Quit the application as soon as possible"),
        ];
        if !self.runs.is_empty() {
            all.push(("Scan history", "Past scans, trend and changes"));
        }
        if self.current.is_some() {
            all.push(("Export Markdown report", "Checklist of issues with fixes"));
            all.push(("Full output", "Raw text output of this scan"));
            all.push((
                "AI action plan",
                "Claude prioritizes every open issue in this scan",
            ));
        }
        if self.can_explain() {
            all.push((
                "Explain with AI",
                "Claude explains the selected finding and how to fix it",
            ));
        }
        all.sort_by_key(|c| c.0);
        let q = query.to_lowercase();
        all.into_iter()
            .filter(|(name, _)| {
                let mut it = name.to_lowercase().chars().collect::<Vec<_>>().into_iter();
                q.chars().all(|qc| it.any(|c| c == qc))
            })
            .collect()
    }

    fn run_command(&mut self, cmd: &str) {
        match cmd {
            "Deep scan" => self.start_scan("deep"),
            "Full scan" => self.start_scan("full"),
            "Quick scan" => self.start_scan("quick"),
            "Keys" => self.help = !self.help,
            "Quit" => self.quit = true,
            "Scan history" => self.screen = Screen::History(0),
            "Export Markdown report" => self.export(),
            "Full output" => self.open_log(),
            "AI action plan" => self.with_ai(crate::tui::ai_screen::Then::Plan),
            "Explain with AI" => self.explain(),
            _ => {}
        }
    }

    pub(super) fn setup_button(&mut self, mut setup: crate::tui::ai_screen::AiSetup, id: &str) {
        match id {
            "ai-yes" => {
                if setup.enable() {
                    self.open_ai(setup.then);
                } else {
                    self.modal = Some(Modal::AiSetup(setup));
                }
            }
            "ai-use-api" => {
                setup.switch("api");
                self.modal = Some(Modal::AiSetup(setup));
            }
            "ai-use-cc" => {
                setup.switch("claude-code");
                self.modal = Some(Modal::AiSetup(setup));
            }
            _ => {}
        }
    }
}
