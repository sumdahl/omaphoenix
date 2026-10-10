//! Copying, exporting and showing the scan log.

use super::util::{copy_to_clipboard, strip_ansi};
use super::{App, Screen, Severity};
use crate::tui::times::local_time;

impl App {
    pub(super) fn copy(&mut self) {
        let Some(f) = self.selected() else { return };
        let (text, is_fix) = if f.finding.fix.is_empty() {
            (f.finding.hint.clone(), false)
        } else {
            (f.finding.fix.clone(), true)
        };
        if text.is_empty() {
            return;
        }
        copy_to_clipboard(&text);
        self.notify(
            if is_fix {
                "Fix command copied"
            } else {
                "Advice copied"
            },
            Severity::Information,
            5,
        );
    }

    pub(super) fn open_log(&mut self) {
        let Some(run) = self.run() else { return };
        let text = std::fs::read_to_string(run.path.join("output.txt"))
            .unwrap_or_else(|_| "(no output saved for this scan)".into());
        // Textual's Log splits each line again, which drops empty ones.
        let lines = strip_ansi(&text)
            .lines()
            .filter(|l| !l.is_empty())
            .map(str::to_owned)
            .collect();
        let title = local_time(run.meta.started, "%a %-d %b %H:%M");
        self.screen = Screen::Log(lines, 0, title);
    }

    pub(super) fn export(&mut self) {
        let Some(run) = self.run() else { return };
        let started = local_time(run.meta.started, "%Y-%m-%d %H:%M");
        match run.export_markdown(&started) {
            Ok(path) => {
                let s = path.display().to_string();
                copy_to_clipboard(&s);
                let home = std::env::var("HOME").unwrap_or_default();
                let shown = if home.is_empty() {
                    s
                } else {
                    s.replacen(&home, "~", 1)
                };
                self.notify_for(
                    Some("Report saved"),
                    &format!("{shown}\nPath copied to clipboard."),
                    Severity::Information,
                    8,
                );
            }
            Err(e) => self.notify(&e.to_string(), Severity::Error, 8),
        }
    }
}
