//! Settings: timer, secrets and setup.

use phx_core::schedule;
use ratatui::crossterm::event::{KeyCode, KeyEvent};

use super::{State, Suspend};

impl State<'_> {
    pub(super) fn settings_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Char('t') => {
                let off = self.timer.starts_with("timer off");
                let r = if off {
                    schedule::on("30m")
                } else {
                    schedule::off()
                };
                if let Err(e) = r {
                    self.notify(e);
                }
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
