//! Marking findings, running their fixes, and accepting them.

use super::{Action, App, Confirm, Modal, Severity};
use phx_audit::store::{Accepted, Item, Run, now};
use std::collections::BTreeSet;
use std::process::Command;

impl App {
    pub(super) fn toggle_mark(&mut self) {
        let Some(f) = self.selected() else { return };
        if f.finding.fix.is_empty() || f.accepted {
            self.notify(
                "Only findings with an automatic fix can be selected.",
                Severity::Warning,
                3,
            );
            return;
        }
        let n = f.n;
        if !self.marked.remove(&n) {
            self.marked.insert(n);
        }
        let len = self.visible().len();
        self.row = (self.row + 1).min(len.saturating_sub(1));
    }

    pub(super) fn mark_all(&mut self) {
        let applied = self.run().map(Run::applied).unwrap_or_default();
        let fixable: BTreeSet<usize> = self
            .visible()
            .iter()
            .filter(|f| !f.finding.fix.is_empty() && !f.accepted && !applied.contains(&f.n))
            .map(|f| f.n)
            .collect();
        if fixable.is_empty() {
            self.notify(
                "Nothing in this view has an automatic fix.",
                Severity::Warning,
                3,
            );
            return;
        }
        if fixable.is_subset(&self.marked) {
            self.marked = &self.marked - &fixable;
        } else {
            self.marked.extend(fixable);
        }
    }

    fn fix_targets(&self) -> Vec<&Item> {
        let Some(run) = self.run() else {
            return Vec::new();
        };
        if !self.marked.is_empty() {
            return run
                .items
                .iter()
                .filter(|f| self.marked.contains(&f.n))
                .collect();
        }
        self.selected()
            .filter(|f| !f.finding.fix.is_empty() && !f.accepted)
            .into_iter()
            .collect()
    }

    pub(super) fn run_fix(&mut self) {
        let targets = self.fix_targets();
        if targets.is_empty() {
            self.notify(
                "No automatic fix for this finding. See How to address.",
                Severity::Warning,
                5,
            );
            return;
        }
        let many = targets.len() > 1;
        let mut code = Vec::new();
        for (i, f) in targets.iter().enumerate() {
            if many {
                code.push(format!("{}. {}", i + 1, f.title()));
            }
            code.push(format!("$ {}", f.finding.fix));
            if many && i + 1 < targets.len() {
                code.push(String::new());
            }
        }
        let mut note = "Runs in this terminal. You'll see the output, then return here.".to_owned();
        if targets.iter().any(|f| f.finding.fix.contains("sudo")) {
            note.push_str(" sudo asks for your password once.");
        }
        let applied = self.run().map(Run::applied).unwrap_or_default();
        let again = targets.iter().filter(|f| applied.contains(&f.n)).count();
        if again > 0 {
            note = format!(
                "{again} of these {} already applied in this scan. {note}",
                if many { "were" } else { "was" }
            );
        }
        let title = if many {
            format!("Run {} fixes?", targets.len())
        } else {
            format!(
                "Run this fix?  {}",
                targets.first().map_or("", |f| f.title())
            )
        };
        let ok = if many {
            format!("Run {}", targets.len())
        } else {
            "Run".into()
        };
        let ns = targets.iter().map(|f| f.n).collect();
        self.modal = Some(Modal::Confirm(Confirm {
            title,
            code,
            note,
            ok,
            danger: false,
            on_ok: false,
            action: Action::RunFixes(ns),
        }));
    }

    /// Runs the fixes in the plain terminal (the loop has left the TUI).
    pub fn run_fixes(&mut self, ns: &[usize]) {
        use std::io::Write;
        let Some(run) = self.run().cloned() else {
            return;
        };
        let targets: Vec<&Item> = run.items.iter().filter(|f| ns.contains(&f.n)).collect();
        let many = targets.len() > 1;
        let mut out = std::io::stdout();
        let mut results = Vec::new();
        for (i, f) in targets.iter().enumerate() {
            let step = if many {
                format!("[{}/{}] ", i + 1, targets.len())
            } else {
                String::new()
            };
            let _ = writeln!(
                out,
                "\n  \x1b[1m{step}{}\x1b[0m\n  \x1b[2m$ {}\x1b[0m",
                f.title(),
                f.finding.fix
            );
            let rc = Command::new("bash")
                .arg("-c")
                .arg(&f.finding.fix)
                .status()
                .ok()
                .and_then(|s| s.code())
                .unwrap_or(1);
            let _ = writeln!(
                out,
                "{}",
                if rc == 0 {
                    "  \x1b[32m✓ done\x1b[0m".to_owned()
                } else {
                    format!("  \x1b[31m✗ exit status {rc}\x1b[0m")
                }
            );
            results.push((f.n, rc));
        }
        let ok_n = results.iter().filter(|(_, rc)| *rc == 0).count();
        let _ = write!(
            out,
            "\n  {ok_n} of {} succeeded.\n  Press Enter to return to Security Audit… ",
            results.len()
        );
        let _ = out.flush();
        let mut line = String::new();
        let _ = std::io::stdin().read_line(&mut line);
        for (n, rc) in &results {
            if *rc == 0 {
                let _ = run.mark_applied(*n);
                self.marked.remove(n);
            }
        }
        let failed = results.len() - ok_n;
        if failed > 0 {
            self.notify(
                &format!(
                    "{failed} fix{} failed; they stay selected.",
                    if failed == 1 { "" } else { "es" }
                ),
                Severity::Error,
                5,
            );
        } else {
            let title = format!(
                "{} fix{} applied",
                results.len(),
                if many { "es" } else { "" }
            );
            self.notify_for(
                Some(&title),
                "Press r to rescan and confirm the issues are gone.",
                Severity::Information,
                5,
            );
        }
    }

    pub(super) fn accept(&mut self) {
        let Some(f) = self.selected() else { return };
        let (n, was, key, title, section) = (
            f.n,
            f.accepted,
            f.accept_key(),
            f.title().to_owned(),
            f.finding.section.clone(),
        );
        let msg = if was {
            self.accepted.remove(&key);
            format!("Moved back to Issues: {title}")
        } else {
            self.accepted.insert(
                key,
                Accepted {
                    section,
                    msg: title.clone(),
                    at: now(),
                },
            );
            self.marked.remove(&n);
            format!("Accepted, no longer scored: {title}")
        };
        let _ = self.store.save_accepted(&self.accepted);
        for r in &mut self.runs {
            r.recount(&self.accepted);
        }
        self.refresh(Some(n));
        self.notify(
            &format!("{msg}\nPress a again to undo."),
            Severity::Information,
            4,
        );
    }
}
