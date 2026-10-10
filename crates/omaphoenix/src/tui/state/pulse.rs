//! Pulse: the health summary and its one primary action.

use phx_core::App;
use ratatui::crossterm::event::{KeyCode, KeyEvent};

use super::{Msg, Section, State, Suspend, quiet};

impl State<'_> {
    pub(super) fn load_doctor(&mut self) {
        if !self.jobs.insert("doctor") {
            return;
        }
        let tx = self.tx.clone();
        std::thread::spawn(move || {
            let findings = App::from_env(&quiet)
                .map(|a| a.doctor())
                .unwrap_or_default();
            let _ = tx.send(Msg::Doctor(findings));
        });
    }

    pub fn primary(&self) -> (&'static str, String) {
        if !self.app.repo.exists() {
            return ("i", "Set up snapshots".into());
        }
        let risks = self.audit.as_ref().map_or(0, |r| r.counts().fail);
        if risks > 0 {
            return (
                "3",
                format!(
                    "Review {risks} audit risk{}",
                    if risks == 1 { "" } else { "s" }
                ),
            );
        }
        if self.migration.audited && self.migration.snapshot.is_empty() {
            return ("5", "Continue migration".into());
        }
        ("s", "Take snapshot".into())
    }

    pub(super) fn pulse_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Enter | KeyCode::Char(' ') => match self.primary().0 {
                "3" => self.switch(Section::Audit),
                "5" => self.switch(Section::Migrate),
                "i" => self.suspend = Some(Suspend::Cli(vec!["init".into()])),
                _ => self.take_snapshot(),
            },
            KeyCode::Char('s') => self.take_snapshot(),
            KeyCode::Char('d' | 'r') => self.load_doctor(),
            _ => {}
        }
    }
}
