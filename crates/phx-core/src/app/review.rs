//! Whether the user has chosen what to back up, and which detected items they haven't seen yet.

use std::collections::BTreeSet;
use std::fs;
use std::path::PathBuf;

use phx_contract::Manifest;
use phx_contract::selection::Choice;

use super::{App, AppError};
use crate::category::read_json;

/// The file in the snapshot repo listing every item the user has reviewed, as `category:id`.
pub const REVIEWED: &str = "reviewed.txt";

/// `category:id`, the key items are reviewed under.
#[must_use]
pub fn key(category: &str, id: &str) -> String {
    format!("{category}:{id}")
}

impl App<'_> {
    fn reviewed_file(&self) -> PathBuf {
        self.repo.dir.join(REVIEWED)
    }

    /// True once the user has saved a choice of what to back up.
    #[must_use]
    pub fn has_chosen(&self) -> bool {
        self.reviewed_file().is_file()
    }

    /// Every item key the user has reviewed.
    #[must_use]
    pub fn reviewed(&self) -> BTreeSet<String> {
        fs::read_to_string(self.reviewed_file())
            .unwrap_or_default()
            .lines()
            .filter(|l| !l.is_empty() && !l.starts_with('#'))
            .map(str::to_owned)
            .collect()
    }

    /// Keys of detected items the user hasn't reviewed yet.
    #[must_use]
    pub fn unreviewed(&self, choices: &[(Choice, Vec<String>)]) -> BTreeSet<String> {
        let seen = self.reviewed();
        choices
            .iter()
            .flat_map(|(c, _)| c.items.iter().map(|(id, _)| key(&c.category, id)))
            .filter(|k| !seen.contains(k))
            .collect()
    }

    pub(super) fn save_reviewed(&self, choices: &[Choice]) -> Result<(), AppError> {
        let mut text = String::from(
            "# Items reviewed in omaphoenix's Backup section; new ones are asked about.\n",
        );
        let keys: BTreeSet<String> = choices
            .iter()
            .flat_map(|c| c.items.iter().map(|(id, _)| key(&c.category, id)))
            .collect();
        for k in keys {
            text.push_str(&k);
            text.push('\n');
        }
        let tmp = self.repo.dir.join("reviewed.txt.tmp");
        fs::write(&tmp, text)?;
        fs::rename(tmp, self.reviewed_file())?;
        Ok(())
    }

    /// What still needs the user's decision, worded for them; `None` when nothing does.
    ///
    /// Uses the last snapshot's manifest, so it costs no detection.
    #[must_use]
    pub fn review_needed(&self) -> Option<String> {
        if !self.repo.exists() {
            return None;
        }
        if !self.has_chosen() {
            return Some(
                "you haven't chosen what to back up yet; everything detected is included. Run: omaphoenix select".into(),
            );
        }
        let manifest: Manifest = read_json(&self.repo.dir.join("manifest.json"))
            .ok()
            .flatten()?;
        let seen = self.reviewed();
        let new = manifest
            .items
            .iter()
            .flat_map(|(cat, items)| items.iter().map(move |i| key(cat, &i.id)))
            .filter(|k| !seen.contains(k))
            .count();
        (new > 0).then(|| {
            format!("{new} new item(s) are being backed up that you haven't reviewed. Run: omaphoenix select")
        })
    }
}

#[cfg(test)]
mod tests {
    use phx_contract::selection::Choice;

    use crate::app::App;
    use crate::store::Repo;

    #[test]
    fn saving_choices_marks_every_shown_item_reviewed() {
        let home = tempfile::tempdir().unwrap_or_else(|e| unreachable!("{e}"));
        let app = App {
            home: home.path().to_path_buf(),
            repo: Repo {
                dir: home.path().to_path_buf(),
            },
            log: &|_, _| {},
            passphrase: None,
        };
        assert!(!app.has_chosen());
        let choices = vec![Choice {
            category: "theme".into(),
            items: vec![("nord".into(), true), ("tokyo".into(), false)],
        }];
        app.save_choices(&choices)
            .unwrap_or_else(|e| unreachable!("{e}"));
        assert!(app.has_chosen());
        let detected = vec![(
            Choice {
                category: "theme".into(),
                items: vec![
                    ("nord".into(), true),
                    ("tokyo".into(), false),
                    ("rose".into(), true),
                ],
            },
            vec![String::new(); 3],
        )];
        let new: Vec<String> = app.unreviewed(&detected).into_iter().collect();
        assert_eq!(new, ["theme:rose"]);
    }
}
