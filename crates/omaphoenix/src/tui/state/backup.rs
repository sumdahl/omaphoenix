//! Backup: choosing what snapshots capture.

use phx_contract::selection::Choice;
use phx_core::App;
use ratatui::crossterm::event::{KeyCode, KeyEvent};

use super::{Msg, State, quiet};

impl State<'_> {
    pub(super) fn load_choices(&mut self) {
        if !self.jobs.insert("choices") {
            return;
        }
        let tx = self.tx.clone();
        std::thread::spawn(move || {
            let r = App::from_env(&quiet)
                .and_then(|a| a.choices())
                .map_err(|e| e.to_string());
            let _ = tx.send(Msg::Choices(r));
        });
    }

    pub(super) fn backup_key(&mut self, key: KeyEvent) {
        let Some(choices) = &mut self.choices else {
            if key.code == KeyCode::Char('r') {
                self.load_choices();
            }
            return;
        };
        if key.code == KeyCode::Char('w') {
            let plain: Vec<Choice> = choices.iter().map(|(c, _)| c.clone()).collect();
            match self.app.save_choices(&plain) {
                Ok(()) => {
                    self.dirty = false;
                    self.notify("✓ saved; the next snapshot captures exactly this");
                }
                Err(e) => self.notify(format!("save failed: {e}")),
            }
            return;
        }
        let n_cats = choices.len();
        let Some((cat, _)) = choices.get_mut(self.cat_sel) else {
            return;
        };
        let n_items = cat.items.len();
        match key.code {
            KeyCode::Up | KeyCode::Char('k') => {
                if self.in_items {
                    self.item_sel = self.item_sel.saturating_sub(1);
                } else {
                    self.cat_sel = self.cat_sel.saturating_sub(1);
                }
            }
            KeyCode::Down | KeyCode::Char('j') => {
                if self.in_items {
                    self.item_sel = (self.item_sel + 1).min(n_items.saturating_sub(1));
                } else {
                    self.cat_sel = (self.cat_sel + 1).min(n_cats.saturating_sub(1));
                }
            }
            KeyCode::Enter | KeyCode::Right | KeyCode::Char('l')
                if !self.in_items && n_items > 0 =>
            {
                self.in_items = true;
                self.item_sel = 0;
            }
            KeyCode::Esc | KeyCode::Left | KeyCode::Char('h') | KeyCode::Backspace => {
                self.in_items = false;
            }
            KeyCode::Char(' ') => {
                if self.in_items {
                    if let Some((_, on)) = cat.items.get_mut(self.item_sel) {
                        *on = !*on;
                    }
                } else {
                    let all = cat.items.iter().all(|(_, on)| *on);
                    cat.items.iter_mut().for_each(|(_, on)| *on = !all);
                }
                self.dirty = true;
            }
            KeyCode::Char('a') if self.in_items => {
                let all = cat.items.iter().all(|(_, on)| *on);
                cat.items.iter_mut().for_each(|(_, on)| *on = !all);
                self.dirty = true;
            }
            _ => {}
        }
    }
}
