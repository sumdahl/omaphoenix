use std::fs;

use phx_contract::selection;

use super::{App, AppError};
use crate::category::{self, Env};

impl App<'_> {
    /// Every category's detected items, ticked per the current rules, with their notes.
    ///
    /// # Errors
    /// When not initialised or detection fails.
    pub fn choices(&self) -> Result<Vec<(selection::Choice, Vec<String>)>, AppError> {
        if !self.repo.exists() {
            return Err(AppError::NotInitialised);
        }
        let rules = self.rules()?;
        let quiet = |_: &str, _: &str| {};
        let env = Env::new(&self.home, &self.repo.dir, &quiet);
        let mut out = Vec::new();
        for c in category::all() {
            let items = c
                .detect(&env)
                .map_err(|e| AppError::Other(format!("{}: {e}", c.name())))?;
            let notes = items.iter().map(|i| i.note.clone()).collect();
            let choice = selection::Choice {
                category: c.name().to_owned(),
                items: items
                    .into_iter()
                    .map(|i| {
                        let on = rules.selected(c.name(), &i.id);
                        (i.id, on)
                    })
                    .collect(),
            };
            out.push((choice, notes));
        }
        Ok(out)
    }

    /// Rewrites `selection.conf` to capture exactly what is ticked, and marks every shown item reviewed.
    ///
    /// # Errors
    /// On I/O failure.
    pub fn save_choices(&self, choices: &[selection::Choice]) -> Result<(), AppError> {
        let tmp = self.repo.dir.join("selection.conf.tmp");
        fs::write(&tmp, selection::build(choices))?;
        fs::rename(tmp, self.selection_file())?;
        self.save_reviewed(choices)
    }
}
