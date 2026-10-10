//! Global keys and dispatch to the current section.

use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use super::{Section, State};

impl State<'_> {
    pub fn on_key(&mut self, key: KeyEvent) {
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
            self.quit = true;
            return;
        }
        if let Some((_, work)) = self.confirm.take() {
            if key.code == KeyCode::Char('y') {
                self.suspend = Some(work);
            }
            return;
        }
        if let Some((q, sel)) = self.palette.take() {
            self.palette_key(q, sel, key);
            return;
        }
        if self.searching {
            self.search_key(key);
            return;
        }
        if self.help {
            self.help = false;
            if matches!(key.code, KeyCode::Esc | KeyCode::Char('?')) {
                return;
            }
        }
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        let was_armed = std::mem::take(&mut self.quit_armed);
        match key.code {
            KeyCode::Char('?') => {
                self.help = true;
                return;
            }
            KeyCode::Char(':') => {
                self.palette = Some((String::new(), 0));
                return;
            }
            KeyCode::Char('k') if ctrl => {
                self.palette = Some((String::new(), 0));
                return;
            }
            KeyCode::Char('/') => {
                if matches!(self.section, Section::Timeline | Section::Audit) {
                    self.searching = true;
                } else {
                    self.notify("/ searches the Timeline and Audit lists");
                }
                return;
            }
            KeyCode::Char('u') => {
                self.ask_undo();
                return;
            }
            KeyCode::Esc if !self.search.is_empty() => {
                self.search.clear();
                return;
            }
            KeyCode::Char('q') => {
                if self.dirty && !was_armed {
                    self.quit_armed = true;
                    self.notify("Unsaved backup selection: w saves, q again quits without saving");
                } else {
                    self.quit = true;
                }
                return;
            }
            KeyCode::Char(c @ '1'..='6') => {
                let i = usize::from(c as u8 - b'1');
                if let Some(s) = Section::ALL.get(i) {
                    self.switch(*s);
                }
                return;
            }
            KeyCode::Tab | KeyCode::BackTab => {
                let i = Section::ALL
                    .iter()
                    .position(|s| *s == self.section)
                    .unwrap_or(0);
                let n = Section::ALL.len();
                let next = if key.code == KeyCode::Tab {
                    (i + 1) % n
                } else {
                    (i + n - 1) % n
                };
                if let Some(s) = Section::ALL.get(next) {
                    self.switch(*s);
                }
                return;
            }
            _ => {}
        }
        match self.section {
            Section::Pulse => self.pulse_key(key),
            Section::Timeline => self.timeline_key(key),
            Section::Audit => self.audit_key(key),
            Section::Backup => self.backup_key(key),
            Section::Migrate => self.migrate_key(key),
            Section::Settings => self.settings_key(key),
        }
    }

    fn search_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Enter => self.searching = false,
            KeyCode::Esc => {
                self.searching = false;
                self.search.clear();
            }
            KeyCode::Backspace => {
                self.search.pop();
            }
            KeyCode::Char(c) => self.search.push(c),
            _ => {}
        }
        self.snap_sel = 0;
        self.audit_sel = 0;
    }

    pub(super) fn switch(&mut self, s: Section) {
        if s != self.section {
            self.search.clear();
        }
        self.section = s;
        if s == Section::Backup && self.choices.is_none() {
            self.load_choices();
        }
        if s == Section::Pulse && self.findings.is_none() {
            self.load_doctor();
        }
    }
}
