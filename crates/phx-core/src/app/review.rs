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
        // The last snapshot's items count too: one that vanished before this save was never on screen.
        let mut keys = self.snapshot_keys();
        keys.extend(
            choices
                .iter()
                .flat_map(|c| c.items.iter().map(|(id, _)| key(&c.category, id))),
        );
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
        let seen = self.reviewed();
        let new = self
            .snapshot_keys()
            .iter()
            .filter(|k| !seen.contains(*k))
            .count();
        (new > 0).then(|| {
            format!("{new} item(s) in the last snapshot haven't been reviewed yet. Run: omaphoenix select")
        })
    }

    /// Item keys in the last snapshot's manifest; empty before the first one.
    fn snapshot_keys(&self) -> BTreeSet<String> {
        let manifest: Option<Manifest> = read_json(&self.repo.dir.join("manifest.json"))
            .ok()
            .flatten();
        manifest
            .into_iter()
            .flat_map(|m| m.items)
            .flat_map(|(cat, items)| items.into_iter().map(move |i| key(&cat, &i.id)))
            .collect()
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

    #[test]
    fn an_item_that_vanished_before_the_save_is_not_reported() {
        let home = tempfile::tempdir().unwrap_or_else(|e| unreachable!("{e}"));
        let app = App {
            home: home.path().to_path_buf(),
            repo: Repo {
                dir: home.path().to_path_buf(),
            },
            log: &|_, _| {},
            passphrase: None,
        };
        std::fs::create_dir_all(home.path().join(".git")).unwrap_or_else(|e| unreachable!("{e}"));
        std::fs::write(
            home.path().join("manifest.json"),
            r#"{"format":1,"items":{"theme":[{"id":"nord"},{"id":"gone"}]}}"#,
        )
        .unwrap_or_else(|e| unreachable!("{e}"));
        let now = vec![Choice {
            category: "theme".into(),
            items: vec![("nord".into(), true)],
        }];
        app.save_choices(&now)
            .unwrap_or_else(|e| unreachable!("{e}"));
        assert_eq!(app.review_needed(), None);
    }
}
