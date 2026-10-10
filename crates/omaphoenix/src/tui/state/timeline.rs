//! Timeline: snapshots, preview, restore and undo.

use std::process::{Command, Stdio};

use phx_core::store::Snapshot;
use ratatui::crossterm::event::{KeyCode, KeyEvent};

use super::{Msg, Section, State, Suspend};

impl State<'_> {
    pub(super) fn run_captured(&mut self, title: &str, args: &[&str]) {
        if self.jobs.contains("command") {
            self.notify("Still running the last command; wait for it to finish");
            return;
        }
        let Ok(exe) = std::env::current_exe() else {
            return;
        };
        let child = Command::new(exe)
            .args(args)
            // The TUI shows the result; no desktop notification on top.
            .env("OMAPHOENIX_NO_NOTIFY", "1")
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn();
        let mut child = match child {
            Ok(c) => c,
            Err(e) => {
                self.notify(format!("couldn't start: {e}"));
                return;
            }
        };
        self.output.clear();
        title.clone_into(&mut self.output_title);
        self.jobs.insert("command");
        let (out, err) = (child.stdout.take(), child.stderr.take());
        let tx = self.tx.clone();
        let tx_err = self.tx.clone();
        std::thread::spawn(move || {
            use std::io::BufRead;
            if let Some(e) = err {
                for l in std::io::BufReader::new(e).lines().map_while(Result::ok) {
                    let _ = tx_err.send(Msg::Line(l));
                }
            }
        });
        std::thread::spawn(move || {
            use std::io::BufRead;
            if let Some(o) = out {
                for l in std::io::BufReader::new(o).lines().map_while(Result::ok) {
                    let _ = tx.send(Msg::Line(l));
                }
            }
            let ok = child.wait().is_ok_and(|s| s.success());
            let _ = tx.send(Msg::Done(ok));
        });
    }

    pub(super) fn take_snapshot(&mut self) {
        if !self.app.repo.exists() {
            self.notify("Not set up yet: press enter on Pulse, or run omaphoenix init");
            return;
        }
        if !self.app.has_chosen() {
            self.open_backup(
                "First choose what to back up: untick what you don't want, then press w",
            );
            return;
        }
        if self.jobs.contains("choices") {
            self.notify("Checking this machine for new items first; try again in a moment");
            return;
        }
        if !self.unreviewed.is_empty() {
            self.ask_snapshot = Some(format!(
                "{} new item(s) you haven't reviewed would be backed up. b review them · y snapshot anyway · esc cancel",
                self.unreviewed.len()
            ));
            return;
        }
        self.run_snapshot();
    }

    pub(super) fn run_snapshot(&mut self) {
        if self.timer_running {
            self.notify("The automatic snapshot is running right now; it will finish on its own");
            return;
        }
        self.section = Section::Timeline;
        self.run_captured("Snapshot", &["snapshot"]);
    }

    pub fn snap_view(&self) -> Vec<&Snapshot> {
        let q = self.search.to_lowercase();
        self.snaps
            .iter()
            .filter(|s| {
                q.is_empty()
                    || format!("{} {}", s.id, s.subject)
                        .to_lowercase()
                        .contains(&q)
            })
            .collect()
    }

    pub(super) fn ask_undo(&mut self) {
        if !self.app.repo.exists() {
            self.notify("Nothing to undo: not set up yet");
            return;
        }
        self.confirm = Some((
            "Undo the last restore? Files come back; installed packages are listed, not removed. [y/N]".into(),
            Suspend::Cli(vec!["undo".into()]),
        ));
    }

    pub(super) fn timeline_key(&mut self, key: KeyEvent) {
        let n = self.snap_view().len();
        match key.code {
            KeyCode::Down | KeyCode::Char('j') => {
                self.snap_sel = (self.snap_sel + 1).min(n.saturating_sub(1));
            }
            KeyCode::Up | KeyCode::Char('k') => self.snap_sel = self.snap_sel.saturating_sub(1),
            KeyCode::Char('s') => self.take_snapshot(),
            KeyCode::Enter | KeyCode::Char('p') => {
                if let Some(s) = self.snap_view().get(self.snap_sel).copied() {
                    let id = s.id.clone();
                    self.run_captured(
                        &format!("Preview of {id}"),
                        &["restore", "--dry-run", "--at", &id],
                    );
                }
            }
            KeyCode::Char('r') => {
                if let Some(id) = self.snap_view().get(self.snap_sel).map(|s| s.id.clone()) {
                    self.confirm = Some((
                        format!(
                            "Restore snapshot {id} onto this machine? Replaced files are kept as .pre-restore. [y/N]"
                        ),
                        Suspend::Cli(vec!["restore".into(), "--at".into(), id]),
                    ));
                }
            }
            KeyCode::Esc => self.output.clear(),
            _ => {}
        }
    }
}
