//! Audit: the quick scan, its findings, fixes and accepted risks.

use std::path::PathBuf;

use phx_audit::store::{Accepted, Store};
use ratatui::crossterm::event::{KeyCode, KeyEvent};

use super::{Msg, State, Suspend};

impl State<'_> {
    /// Saved in the Security Audit app's history so both show the same scans.
    pub(super) fn run_audit(&mut self) {
        if !self.jobs.insert("audit") {
            return;
        }
        let tx = self.tx.clone();
        let store = self.audit_store.clone();
        self.audit_progress = Some((0, phx_audit::checks::SECTIONS.len(), String::new()));
        std::thread::spawn(move || {
            let progress = tx.clone();
            let r = audit_now(&store, |done, total, section| {
                let _ = progress.send(Msg::AuditProgress(done, total, section.to_owned()));
            });
            let _ = tx.send(Msg::Audited(r));
        });
    }

    pub fn audit_items(&self) -> Vec<&phx_audit::store::Item> {
        let Some(run) = &self.audit else {
            return Vec::new();
        };
        let issues: Vec<_> = run.items.iter().filter(|i| i.is_issue()).collect();
        let mut items: Vec<_> = phx_core::fuzzy::filter(&self.search, &issues, |i| {
            format!(
                "{} {} {}",
                i.finding.msg, i.finding.section, i.finding.detail
            )
        })
        .into_iter()
        .filter_map(|i| issues.get(i).copied())
        .collect();
        items.sort_by_key(|i| (i.accepted, phx_audit::store::sev_order(i.finding.sev), i.n));
        items
    }

    pub(super) fn audit_key(&mut self, key: KeyEvent) {
        let n = self.audit_items().len();
        match key.code {
            // DESIGN.md §7.1: s scans, R rescans; r means restore elsewhere.
            KeyCode::Char('s' | 'R') => self.run_audit(),
            KeyCode::Down | KeyCode::Char('j') => {
                self.audit_sel = (self.audit_sel + 1).min(n.saturating_sub(1));
            }
            KeyCode::Up | KeyCode::Char('k') => self.audit_sel = self.audit_sel.saturating_sub(1),
            KeyCode::Char('a') => self.toggle_accept(),
            KeyCode::Char('x') => {
                let fix = self
                    .audit_items()
                    .get(self.audit_sel)
                    .map(|i| i.finding.fix.clone())
                    .filter(|f| !f.is_empty());
                match fix {
                    // Fixes always confirm (DESIGN.md §7.1, AGENTS.md).
                    Some(fix) => {
                        self.confirm = Some((
                            format!("Run this fix in the terminal?  $ {fix}  [y/N]"),
                            Suspend::Fix(fix),
                        ));
                    }
                    None => self.notify("No automatic fix for this finding; see its advice"),
                }
            }
            KeyCode::Char('o') => {
                self.suspend = Some(Suspend::AuditTui);
            }
            _ => {}
        }
    }

    fn toggle_accept(&mut self) {
        let Some(item) = self.audit_items().get(self.audit_sel).copied() else {
            return;
        };
        let key = item.accept_key();
        let (title, section, was) = (
            item.title().to_owned(),
            item.finding.section.clone(),
            item.accepted,
        );
        if was {
            self.accepted.remove(&key);
        } else {
            let at = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |d| d.as_secs());
            self.accepted.insert(
                key,
                Accepted {
                    section,
                    msg: title.clone(),
                    at,
                },
            );
        }
        if let Err(e) = self.audit_store.save_accepted(&self.accepted) {
            self.notify(format!("Couldn't save: {e}"));
            return;
        }
        if let Some(run) = &mut self.audit {
            run.recount(&self.accepted);
        }
        self.notify(if was {
            format!("Back to risks: {title}")
        } else {
            format!("Accepted, no longer scored: {title}")
        });
    }
}

fn audit_now(store: &Store, progress: impl FnMut(usize, usize, &str)) -> Result<PathBuf, String> {
    let ctx = phx_audit::audit::Ctx::from_env();
    let host = std::fs::read_to_string("/proc/sys/kernel/hostname").unwrap_or_default();
    let kernel = std::fs::read_to_string("/proc/sys/kernel/osrelease").unwrap_or_default();
    store
        .record_quick_scan_with(
            &ctx,
            host.trim(),
            kernel.trim(),
            &phx_core::category::stamp(),
            progress,
        )
        .map_err(|e| e.to_string())
}
