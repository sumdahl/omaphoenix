//! The shown run: loading it, and the sections and rows the filters leave.

use phx_audit::store::{Item, Run, sev_order};
use std::collections::BTreeSet;
use std::path::Path;

use super::{App, Focus};

impl App {
    pub fn run(&self) -> Option<&Run> {
        let cur = self.current.as_ref()?;
        self.runs.iter().find(|r| &r.path == cur)
    }

    pub fn show_run(&mut self, path: &Path) {
        if self.current.as_deref() != Some(path) {
            self.marked.clear();
        }
        self.current = Some(path.to_path_buf());
        let sections = self.sections();
        if self.section.as_ref().is_some_and(|s| !sections.contains(s)) {
            self.section = None;
        }
        self.section_cursor = self
            .section
            .as_ref()
            .and_then(|s| sections.iter().position(|x| x == s))
            .map_or(0, |i| i + 1);
        self.focus = Focus::Table;
        self.clamp_row();
    }

    pub(super) fn show_empty(&mut self) {
        self.current = None;
        self.focus = Focus::Welcome(0);
    }

    /// Sections present in the run, in engine order, then any others.
    pub fn sections(&self) -> Vec<String> {
        let Some(run) = self.run() else {
            return Vec::new();
        };
        let present: BTreeSet<&str> = run
            .items
            .iter()
            .map(|i| i.finding.section.as_str())
            .collect();
        let order: Vec<&str> = phx_audit::checks::SECTIONS.iter().map(|s| s.name).collect();
        let mut out: Vec<String> = order
            .iter()
            .filter(|s| present.contains(**s))
            .map(|s| (*s).to_owned())
            .collect();
        out.extend(
            present
                .iter()
                .filter(|s| !order.contains(s))
                .map(|s| (*s).to_owned()),
        );
        out
    }

    /// `visible()`: tab, section and search applied, worst first.
    pub fn visible(&self) -> Vec<&Item> {
        let Some(run) = self.run() else {
            return Vec::new();
        };
        let q = self.search.trim().to_lowercase();
        let mut rows: Vec<&Item> = run
            .items
            .iter()
            .filter(|f| self.tab.holds(f))
            .filter(|f| {
                self.section
                    .as_ref()
                    .is_none_or(|s| &f.finding.section == s)
            })
            .filter(|f| {
                let fd = &f.finding;
                q.is_empty()
                    || format!(
                        "{}\n{}\n{}\n{}\n{}",
                        fd.msg, fd.section, fd.detail, fd.fix, fd.hint
                    )
                    .to_lowercase()
                    .contains(&q)
            })
            .collect();
        rows.sort_by_key(|f| (sev_order(f.finding.sev), f.n));
        rows
    }

    pub fn selected(&self) -> Option<&Item> {
        self.visible().get(self.row).copied()
    }

    pub(super) fn clamp_row(&mut self) {
        let n = self.visible().len();
        self.row = self.row.min(n.saturating_sub(1));
        self.detail_scroll = 0;
    }

    /// Keeps the same finding selected after the view changes (`refresh_table()`), else the same position.
    pub(super) fn refresh(&mut self, keep: Option<usize>) {
        if let Some(n) = keep
            && let Some(i) = self.visible().iter().position(|f| f.n == n)
        {
            self.row = i;
        }
        self.clamp_row();
    }

    fn previous_of(&self, path: &Path) -> Option<&Run> {
        let i = self.runs.iter().position(|r| r.path == path)?;
        self.runs.get(i + 1)
    }

    pub fn previous_of_run(&self, run: &Run) -> Option<&Run> {
        self.previous_of(&run.path)
    }
}
