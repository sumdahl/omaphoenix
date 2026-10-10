//! Background jobs: applying their results, and the footer's busy label.

use std::time::Duration;

use super::{Msg, State};

impl State<'_> {
    /// Applies finished background work; true when the screen changed.
    pub fn tick(&mut self) -> bool {
        let mut changed = false;
        if self
            .toast
            .as_ref()
            .is_some_and(|(_, at)| at.elapsed() > Duration::from_secs(5))
        {
            self.toast = None;
            changed = true;
        }
        if self.last_poll.elapsed() > Duration::from_secs(2) {
            self.last_poll = std::time::Instant::now();
            let running = phx_core::schedule::running();
            if running != self.timer_running {
                self.timer_running = running;
                if !running {
                    self.snaps = self.app.repo.list().unwrap_or_default();
                    self.load_doctor();
                }
                changed = true;
            }
        }
        while let Ok(msg) = self.rx.try_recv() {
            changed = true;
            match msg {
                Msg::Doctor(f) => {
                    self.findings = Some(f);
                    self.jobs.remove("doctor");
                }
                Msg::Choices(Ok(c)) => {
                    self.unreviewed = self.app.unreviewed(&c);
                    self.choices = Some(c);
                    self.jobs.remove("choices");
                }
                Msg::Choices(Err(e)) => {
                    self.jobs.remove("choices");
                    self.notify(e);
                }
                Msg::AuditProgress(done, total, section) => {
                    self.audit_progress = Some((done, total, section));
                }
                Msg::Audited(r) => {
                    self.jobs.remove("audit");
                    self.audit_progress = None;
                    match r {
                        Ok(dir) => {
                            self.audit = self
                                .audit_store
                                .runs()
                                .into_iter()
                                .find(|run| run.path == dir);
                            self.audit_sel = 0;
                            if !self.migration.audited {
                                self.migration.audited = true;
                                self.save_migration();
                            }
                            if let Some(run) = &self.audit {
                                let c = run.counts();
                                self.notify(format!(
                                    "Audit done: score {} ({}), {} fail, {} warn",
                                    c.score(),
                                    c.grade(),
                                    c.fail,
                                    c.warn
                                ));
                            }
                        }
                        Err(e) => self.notify(format!("Audit failed: {e}")),
                    }
                }
                Msg::Line(l) => self.output.push(l),
                Msg::Done(ok) => {
                    self.jobs.remove("command");
                    self.snaps = self.app.repo.list().unwrap_or_default();
                    if ok
                        && self.output_title.starts_with("Snapshot")
                        && let Some(s) = self.snaps.first()
                    {
                        self.migration.snapshot.clone_from(&s.id);
                        self.save_migration();
                    }
                }
            }
        }
        changed
    }

    pub fn busy_label(&self) -> Option<String> {
        if self.jobs.is_empty() {
            return None;
        }
        let names: Vec<String> = self
            .jobs
            .iter()
            .map(|j| match *j {
                "doctor" => "checking backups".to_owned(),
                "choices" => "detecting items".to_owned(),
                "audit" => match &self.audit_progress {
                    Some((done, total, section)) if !section.is_empty() => {
                        format!(
                            "auditing {} {section} ({}/{total})",
                            bar(*done, *total),
                            done + 1
                        )
                    }
                    _ => "auditing".to_owned(),
                },
                "command" if self.output_title.starts_with("Snapshot") => {
                    let (done, total) = snapshot_progress(&self.output);
                    let pct = done * 100 / total.max(1);
                    match self.current_step() {
                        Some(step) => format!("snapshot {} {pct}% · {step}", bar(done, total)),
                        None => format!("snapshot {} {pct}%", bar(done, total)),
                    }
                }
                _ => match self.current_step() {
                    Some(step) => format!("{} · {step}", self.output_title),
                    None => self.output_title.clone(),
                },
            })
            .collect();
        Some(format!("⋯ {}", names.join(", ")))
    }
}

impl State<'_> {
    /// The last line the running command printed, as its current step.
    fn current_step(&self) -> Option<String> {
        self.output
            .iter()
            .rev()
            .map(|l| l.trim().trim_start_matches("▸ ").trim())
            .find(|l| !l.is_empty())
            .map(str::to_owned)
    }
}

fn bar(done: usize, total: usize) -> String {
    let filled = (done * 10).checked_div(total).unwrap_or(0).min(10);
    format!("{}{}", "▰".repeat(filled), "▱".repeat(10 - filled))
}

/// Finished steps in a snapshot's output: the audit, each category, then saving.
fn snapshot_progress(output: &[String]) -> (usize, usize) {
    const STEPS: usize = 13;
    let done = output
        .iter()
        .filter(|l| {
            let t = l.trim_start();
            t.contains(" captured (")
                || (t.starts_with("audit ") && t.contains(" score "))
                || t.contains("saving the snapshot")
        })
        .count();
    (done.min(STEPS), STEPS)
}
