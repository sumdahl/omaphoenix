//! Settings: timer, secrets and setup.

use phx_core::schedule;
use ratatui::crossterm::event::{KeyCode, KeyEvent};

use super::{State, Suspend};

impl State<'_> {
    pub(super) fn settings_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Char('t') => {
                let next = match self
                    .interval
                    .and_then(|i| schedule::CHOICES.iter().position(|c| *c == i))
                {
                    None => schedule::CHOICES.first().copied(),
                    Some(i) => schedule::CHOICES.get(i + 1).copied(),
                };
                match schedule::set(next) {
                    Ok(()) => self.notify(schedule::describe(next)),
                    Err(e) => self.notify(e),
                }
                self.interval = schedule::interval();
                self.timer = schedule::status();
            }
            KeyCode::Char('e') => {
                self.suspend = Some(Suspend::Cli(vec!["secrets".into(), "init".into()]));
            }
            KeyCode::Char('i') if !self.app.repo.exists() => {
                self.suspend = Some(Suspend::Cli(vec!["init".into()]));
            }
            _ => {}
        }
    }
}
