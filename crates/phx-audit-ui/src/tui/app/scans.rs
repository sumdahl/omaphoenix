//! Starting a scan, its keys, and taking its result.

use super::{App, Screen, Severity, Suspend};
use crate::tui::scan::Scan;
use ratatui::crossterm::event::{KeyCode, KeyEvent};
use std::path::Path;
use std::process::Command;

impl App {
    /// `start_scan()`: full and deep scans need sudo first.
    pub fn start_scan(&mut self, mode: &str) {
        if !self.engine.is_file() {
            let msg = format!(
                "the audit engine (omaphoenix) was not found at {}",
                self.engine.display()
            );
            self.notify(&msg, Severity::Error, 5);
            return;
        }
        if mode == "quick" || crate::tui::scan::sudo_cached() {
            self.launch(mode);
        } else {
            self.suspend = Some(Suspend::Sudo(mode.to_owned()));
        }
    }

    pub fn launch(&mut self, mode: &str) {
        match Scan::start(&self.store, &self.engine, mode, &self.host, &self.user) {
            Ok(scan) => self.screen = Screen::Scan(Box::new(scan)),
            Err(e) => self.notify(&e.to_string(), Severity::Error, 5),
        }
    }

    pub(super) fn scan_key(&mut self, mut scan: Box<Scan>, key: KeyEvent) {
        match key.code {
            KeyCode::Esc => {
                scan.cancel();
                self.scan_done(None);
                return;
            }
            KeyCode::Char('?') => self.help = !self.help,
            KeyCode::Char('q') => {
                scan.cancel();
                self.quit = true;
            }
            _ => {}
        }
        self.screen = Screen::Scan(scan);
    }

    pub(super) fn scan_done(&mut self, path: Option<&Path>) {
        let Some(path) = path else {
            self.notify("Scan cancelled", Severity::Warning, 5);
            return;
        };
        self.runs = self.store.runs();
        if !self.runs.iter().any(|r| r.path == path) {
            return;
        }
        self.section = None;
        self.show_run(path);
        let Some(run) = self.run() else { return };
        let c = run.counts();
        let body = format!(
            "Score {} ({}) · {} fail · {} warn",
            c.score(),
            c.grade(),
            c.fail,
            c.warn
        );
        let sev = if c.fail > 0 {
            Severity::Error
        } else {
            Severity::Information
        };
        let _ = Command::new("notify-send")
            .args([
                "-a",
                "OmaPhoenix",
                "-i",
                "security-high-symbolic",
                "Security audit finished",
                &body,
            ])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn();
        self.notify_for(Some("Scan finished"), &body, sev, 5);
    }
}
