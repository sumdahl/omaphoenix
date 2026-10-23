//! The command palette.

use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use super::{Section, State, Suspend};

/// Command palette entries: every action by name (DESIGN.md §7.1).
pub const ACTIONS: [&str; 16] = [
    "Take snapshot",
    "Scan (quick audit)",
    "Preview selected snapshot",
    "Restore selected snapshot",
    "Undo last restore",
    "Save backup selection",
    "Turn automatic snapshots on/off",
    "Set up encrypted secrets",
    "Open the Security Audit app",
    "Go to Pulse",
    "Go to Timeline",
    "Go to Audit",
    "Go to Backup",
    "Go to Migrate",
    "Go to Settings",
    "Keys",
];

impl State<'_> {
    /// Actions matching the palette query, best first.
    pub fn actions(q: &str) -> Vec<&'static str> {
        phx_core::fuzzy::rank(q, &ACTIONS, |a: &&str| (*a).to_owned())
            .into_iter()
            .filter_map(|(i, _)| ACTIONS.get(i).copied())
            .collect()
    }

    pub(super) fn palette_key(&mut self, mut q: String, mut sel: usize, key: KeyEvent) {
        let n = Self::actions(&q).len();
        match key.code {
            KeyCode::Esc => return,
            KeyCode::Down => sel = (sel + 1).min(n.saturating_sub(1)),
            KeyCode::Up => sel = sel.saturating_sub(1),
            KeyCode::Backspace => {
                q.pop();
                sel = 0;
            }
            KeyCode::Enter => {
                if let Some(a) = Self::actions(&q).get(sel).copied() {
                    self.act(a);
                }
                return;
            }
            KeyCode::Char(c) => {
                q.push(c);
                sel = 0;
            }
            _ => {}
        }
        self.palette = Some((q, sel));
    }

    /// Does a palette action through the same paths as its key.
    fn act(&mut self, a: &str) {
        let press = |st: &mut Self, s: Section, c: KeyCode| {
            st.switch(s);
            st.on_key(KeyEvent::new(c, KeyModifiers::NONE));
        };
        match a {
            "Take snapshot" => self.take_snapshot(),
            "Scan (quick audit)" => press(self, Section::Audit, KeyCode::Char('s')),
            "Preview selected snapshot" => press(self, Section::Timeline, KeyCode::Enter),
            "Restore selected snapshot" => press(self, Section::Timeline, KeyCode::Char('r')),
            "Undo last restore" => self.ask_undo(),
            "Save backup selection" => press(self, Section::Backup, KeyCode::Char('w')),
            "Turn automatic snapshots on/off" => press(self, Section::Settings, KeyCode::Char('t')),
            "Set up encrypted secrets" => press(self, Section::Settings, KeyCode::Char('e')),
            "Open the Security Audit app" => self.suspend = Some(Suspend::SecauditTui),
            "Keys" => self.help = true,
            other => {
                if let Some(s) = Section::ALL
                    .iter()
                    .find(|s| other == format!("Go to {}", s.label()))
                {
                    self.switch(*s);
                }
            }
        }
    }
}
