//! Listing and loading saved runs.

use std::path::{Path, PathBuf};

use phx_audit::store::Run;

use super::Ui;
use super::format::{B, N, RED, YEL, counts, grade_color, grade_of, score_of};
use crate::tui::times::ago;

impl Ui {
    pub(super) fn runs_newest_first(&self) -> Vec<PathBuf> {
        let mut dirs: Vec<PathBuf> = std::fs::read_dir(&self.runs_dir)
            .map(|rd| {
                rd.flatten()
                    .map(|e| e.path())
                    .filter(|p| p.is_dir())
                    .collect()
            })
            .unwrap_or_default();
        dirs.sort();
        dirs.reverse();
        dirs.retain(|d| std::fs::metadata(d.join("findings.jsonl")).is_ok_and(|m| m.len() > 0));
        dirs
    }

    pub(super) fn load(dir: &Path) -> Option<Run> {
        Run::load(dir, &std::collections::BTreeMap::new()).ok()
    }

    pub(super) fn previous_run(&self, dir: &Path) -> Option<PathBuf> {
        let runs = self.runs_newest_first();
        let i = runs.iter().position(|r| r == dir)?;
        runs.get(i + 1).cloned()
    }

    pub(super) fn engine_sections() -> Vec<String> {
        phx_audit::checks::SECTIONS
            .iter()
            .map(|s| s.name)
            .filter(|s| *s != "Summary")
            .map(str::to_owned)
            .collect()
    }

    pub(super) fn run_summary_line(&self, run: &Run) -> String {
        let [f, w, ..] = counts(run);
        let s = score_of(f, w);
        let g = grade_of(s);
        format!(
            "{}{B}{s} {g}{N}    {RED}{f} fail{N}  {YEL}{w} warn{N}  {}· {} scan · {}{N}",
            grade_color(g),
            self.m,
            run.meta.mode,
            ago(run.meta.started)
        )
    }
}
