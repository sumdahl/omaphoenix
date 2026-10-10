//! Migrate: the move-to-a-new-machine wizard.

use ratatui::crossterm::event::{KeyCode, KeyEvent};

use super::{MIGRATE_STEPS, Migration, Section, State};

impl State<'_> {
    pub fn migrate_done(&self) -> [bool; 5] {
        self.migration.done()
    }

    pub(super) fn migrate_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Down | KeyCode::Char('j') => {
                self.migrate_sel = (self.migrate_sel + 1).min(MIGRATE_STEPS.len() - 1);
            }
            KeyCode::Up | KeyCode::Char('k') => {
                self.migrate_sel = self.migrate_sel.saturating_sub(1);
            }
            KeyCode::Enter | KeyCode::Char(' ') => match self.migrate_sel {
                0 => {
                    self.section = Section::Audit;
                    self.run_audit();
                }
                4 => self.run_captured("Re-audit", &["reaudit"]),
                1 => {
                    self.migration.cleaned = true;
                    self.save_migration();
                    self.section = Section::Audit;
                }
                2 => self.take_snapshot(),
                _ => {
                    self.migration.restored_note_seen = true;
                    self.save_migration();
                }
            },
            KeyCode::Char('R') => {
                self.migration = Migration::default();
                self.save_migration();
                self.notify("Migration progress cleared");
            }
            _ => {}
        }
    }
}
