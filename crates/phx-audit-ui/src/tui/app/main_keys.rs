//! Main-screen keys: bindings, focus, table, sections, tabs and search.

use ratatui::crossterm::event::{KeyCode, KeyEvent};

use super::{App, Focus, Modal, Screen, Tab};

impl App {
    pub(super) fn main_key(&mut self, key: KeyEvent) {
        // The search box takes printable keys while focused.
        if self.focus == Focus::Search {
            match key.code {
                KeyCode::Enter | KeyCode::Down | KeyCode::Tab => self.focus = Focus::Table,
                KeyCode::Esc => self.clear_search(),
                KeyCode::BackTab => self.focus = Focus::Tabs,
                KeyCode::Backspace => {
                    self.search.pop();
                    self.refresh(self.selected().map(|f| f.n));
                }
                KeyCode::Char(c) => {
                    self.search.push(c);
                    let keep = self.selected().map(|f| f.n);
                    self.refresh(keep);
                }
                _ => {}
            }
            return;
        }
        if let Focus::Welcome(i) = self.focus {
            match key.code {
                KeyCode::Tab | KeyCode::Right => {
                    self.focus = Focus::Welcome((i + 1) % 3);
                    return;
                }
                KeyCode::BackTab | KeyCode::Left => {
                    self.focus = Focus::Welcome((i + 2) % 3);
                    return;
                }
                KeyCode::Enter | KeyCode::Char(' ') => {
                    self.start_scan(["full", "quick", "deep"].get(i).copied().unwrap_or("full"));
                    return;
                }
                _ => {}
            }
        }
        if self.global_key(key) {
            return;
        }
        match self.focus {
            Focus::Table => self.table_key(key),
            Focus::Sections => self.sections_key(key),
            Focus::Tabs => match key.code {
                KeyCode::Left => self.cycle_tab(false),
                KeyCode::Right => self.cycle_tab(true),
                _ => {}
            },
            Focus::Detail => match key.code {
                KeyCode::Down => self.detail_scroll += 1,
                KeyCode::Up => self.detail_scroll = self.detail_scroll.saturating_sub(1),
                KeyCode::PageDown => self.detail_scroll += 8,
                KeyCode::PageUp => self.detail_scroll = self.detail_scroll.saturating_sub(8),
                KeyCode::Home => self.detail_scroll = 0,
                _ => {}
            },
            _ => {}
        }
    }

    /// App bindings (`BINDINGS`), active on the main screen.
    fn global_key(&mut self, key: KeyEvent) -> bool {
        let has_run = self.current.is_some();
        match key.code {
            KeyCode::Char('q') => self.quit = true,
            KeyCode::Char('?') => self.help = !self.help,
            KeyCode::Char('s') => self.modal = Some(Modal::Chooser(0)),
            KeyCode::Char('r') if has_run => {
                let mode = self
                    .run()
                    .map_or("full".to_owned(), |r| r.meta.mode.clone());
                self.start_scan(&mode);
            }
            KeyCode::Char('h') if !self.runs.is_empty() => self.screen = Screen::History(0),
            KeyCode::Char('/') if has_run => self.focus = Focus::Search,
            KeyCode::Char(c @ '1'..='5') if has_run => {
                let i = usize::from(c as u8 - b'1');
                if let Some(t) = Tab::ALL.get(i) {
                    self.set_tab(*t);
                }
            }
            KeyCode::Char('x') => self.run_fix(),
            KeyCode::Char(' ') if self.can_mark() => self.toggle_mark(),
            KeyCode::Char('*') if self.can_mark() => self.mark_all(),
            KeyCode::Char('a') if self.can_accept() => self.accept(),
            KeyCode::Char('i') if self.can_explain() => self.explain(),
            KeyCode::Char('I') if has_run => self.with_ai(crate::tui::ai_screen::Then::Plan),
            KeyCode::Char('c') if self.can_copy() => self.copy(),
            KeyCode::Char('o') if has_run => self.open_log(),
            KeyCode::Char('e') if has_run => self.export(),
            KeyCode::Esc if has_run => self.clear_search(),
            KeyCode::Tab => self.focus_next(true),
            KeyCode::BackTab => self.focus_next(false),
            _ => return false,
        }
        true
    }

    fn focus_next(&mut self, forward: bool) {
        if self.current.is_none() {
            return;
        }
        let order = [
            Focus::Sections,
            Focus::Tabs,
            Focus::Search,
            Focus::Table,
            Focus::Detail,
        ];
        let i = order.iter().position(|f| *f == self.focus).unwrap_or(3);
        let n = order.len();
        self.focus = order
            .get(if forward {
                (i + 1) % n
            } else {
                (i + n - 1) % n
            })
            .copied()
            .unwrap_or(Focus::Table);
    }

    fn table_key(&mut self, key: KeyEvent) {
        let n = self.visible().len();
        let page = usize::from(self.size.1 / 3).max(1);
        let before = self.row;
        match key.code {
            KeyCode::Down | KeyCode::Char('j') => {
                self.row = (self.row + 1).min(n.saturating_sub(1));
            }
            KeyCode::Up | KeyCode::Char('k') => self.row = self.row.saturating_sub(1),
            KeyCode::PageDown => self.row = (self.row + page).min(n.saturating_sub(1)),
            KeyCode::PageUp => self.row = self.row.saturating_sub(page),
            KeyCode::Home => self.row = 0,
            KeyCode::End => self.row = n.saturating_sub(1),
            // `row_selected`: run the fix, else focus the detail.
            KeyCode::Enter => {
                if self.selected().is_some_and(|f| !f.finding.fix.is_empty()) {
                    self.run_fix();
                } else {
                    self.focus = Focus::Detail;
                }
            }
            _ => {}
        }
        if self.row != before {
            self.detail_scroll = 0;
        }
    }

    pub(super) fn sections_key(&mut self, key: KeyEvent) {
        let n = self.sections().len() + 1;
        match key.code {
            KeyCode::Down => self.section_cursor = (self.section_cursor + 1).min(n - 1),
            KeyCode::Up => self.section_cursor = self.section_cursor.saturating_sub(1),
            KeyCode::Enter => {
                self.focus = Focus::Table;
                return;
            }
            _ => return,
        }
        // `section_highlighted`: the filter follows the highlight.
        let s = self
            .section_cursor
            .checked_sub(1)
            .and_then(|i| self.sections().get(i).cloned());
        if s != self.section {
            self.section = s;
            let keep = self.selected().map(|f| f.n);
            self.refresh(keep);
        }
    }

    pub(super) fn set_tab(&mut self, t: Tab) {
        if t != self.tab {
            let keep = self.selected().map(|f| f.n);
            self.tab = t;
            self.refresh(keep);
        }
    }

    fn cycle_tab(&mut self, forward: bool) {
        let i = Tab::ALL.iter().position(|t| *t == self.tab).unwrap_or(0);
        let next = if forward { (i + 1) % 5 } else { (i + 4) % 5 };
        if let Some(t) = Tab::ALL.get(next) {
            self.set_tab(*t);
        }
    }

    fn clear_search(&mut self) {
        if !self.marked.is_empty() && self.focus != Focus::Search {
            self.marked.clear();
        } else if !self.search.is_empty() {
            self.search.clear();
            let keep = self.selected().map(|f| f.n);
            self.refresh(keep);
        }
        self.focus = Focus::Table;
    }
}
