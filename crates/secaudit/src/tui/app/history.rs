//! Keys of the history and log screens.

use std::path::PathBuf;

use ratatui::crossterm::event::{KeyCode, KeyEvent};

use crate::tui::times::local_time;

use super::{Action, App, Confirm, Modal, Screen};

impl App {
    pub(super) fn history_key(&mut self, mut sel: usize, key: KeyEvent) {
        let n = self.runs.len();
        match key.code {
            KeyCode::Esc => {
                self.history_closed(None);
                return;
            }
            KeyCode::Down => sel = (sel + 1).min(n.saturating_sub(1)),
            KeyCode::Up => sel = sel.saturating_sub(1),
            KeyCode::PageDown => sel = (sel + 10).min(n.saturating_sub(1)),
            KeyCode::PageUp => sel = sel.saturating_sub(10),
            KeyCode::Home => sel = 0,
            KeyCode::End => sel = n.saturating_sub(1),
            KeyCode::Enter => {
                let p = self.runs.get(sel).map(|r| r.path.clone());
                self.history_closed(p);
                return;
            }
            KeyCode::Delete | KeyCode::Char('d') => {
                if let Some(r) = self.runs.get(sel) {
                    let when = local_time(r.meta.started, "%a %-d %b %H:%M");
                    self.modal = Some(Modal::Confirm(Confirm {
                        title: format!("Delete the {} scan from {when}?", r.meta.mode),
                        code: Vec::new(),
                        note: String::new(),
                        ok: "Delete".into(),
                        danger: true,
                        on_ok: false,
                        action: Action::DeleteRun(r.path.clone()),
                    }));
                }
            }
            KeyCode::Char('?') => self.help = !self.help,
            KeyCode::Char('q') => self.quit = true,
            _ => {}
        }
        self.screen = Screen::History(sel);
    }

    fn history_closed(&mut self, path: Option<PathBuf>) {
        self.screen = Screen::Main;
        self.runs = self.store.runs();
        if let Some(p) = path
            && self.runs.iter().any(|r| r.path == p)
        {
            self.section = None;
            self.show_run(&p);
            return;
        }
        if self.runs.is_empty() {
            self.show_empty();
        } else if let Some(cur) = self.current.clone() {
            let p = if cur.exists() {
                cur
            } else {
                self.runs
                    .first()
                    .map(|r| r.path.clone())
                    .unwrap_or_default()
            };
            self.show_run(&p);
        }
    }

    pub(super) fn log_key(
        &mut self,
        lines: Vec<String>,
        mut scroll: usize,
        title: String,
        key: KeyEvent,
    ) {
        let page = usize::from(self.size.1.saturating_sub(3)).max(1);
        let max = lines.len().saturating_sub(page);
        match key.code {
            KeyCode::Esc | KeyCode::Char('q') => return,
            KeyCode::Down => scroll += 1,
            KeyCode::Up => scroll = scroll.saturating_sub(1),
            KeyCode::PageDown | KeyCode::Char(' ') => scroll += page,
            KeyCode::PageUp => scroll = scroll.saturating_sub(page),
            KeyCode::Home => scroll = 0,
            KeyCode::End => scroll = max,
            KeyCode::Char('?') => self.help = !self.help,
            _ => {}
        }
        self.screen = Screen::Log(lines, scroll.min(max), title);
    }
}
