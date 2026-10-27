use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};

use super::{Accepted, Counts, IssueKey, Item, Meta};
use crate::contract::{Finding, Severity};

/// A stored scan.
#[derive(Clone, Debug)]
pub struct Run {
    /// Its directory.
    pub path: PathBuf,
    /// `meta.json`.
    pub meta: Meta,
    /// Findings in report order.
    pub items: Vec<Item>,
}

impl Run {
    /// Reads a scan directory. Unparseable lines are skipped.
    ///
    /// # Errors
    /// If `findings.jsonl` can't be read.
    pub fn load(path: &Path, accepted: &BTreeMap<String, Accepted>) -> std::io::Result<Self> {
        let meta = fs::read_to_string(path.join("meta.json"))
            .ok()
            .and_then(|t| serde_json::from_str(&t).ok())
            .unwrap_or_default();
        let file = fs::File::open(path.join("findings.jsonl"))?;
        let mut items = Vec::new();
        for (i, line) in BufReader::new(file).lines().enumerate() {
            let line = line?;
            if line.trim().is_empty() {
                continue;
            }
            if let Ok(finding) = Finding::parse_line(&line) {
                items.push(Item {
                    n: i + 1,
                    finding,
                    accepted: false,
                });
            }
        }
        let mut run = Self {
            path: path.to_path_buf(),
            meta,
            items,
        };
        run.recount(accepted);
        Ok(run)
    }

    /// Re-applies accepted risks.
    pub fn recount(&mut self, accepted: &BTreeMap<String, Accepted>) {
        for item in &mut self.items {
            item.accepted = item.is_issue() && accepted.contains_key(&item.accept_key());
        }
    }

    /// Counts with accepted issues set apart.
    #[must_use]
    pub fn counts(&self) -> Counts {
        let mut c = Counts::default();
        for item in &self.items {
            let slot = if item.accepted {
                &mut c.accepted
            } else {
                match item.finding.sev {
                    Severity::Fail => &mut c.fail,
                    Severity::Warn => &mut c.warn,
                    Severity::Pass => &mut c.pass,
                    Severity::Skip => &mut c.skip,
                    _ => &mut c.info,
                }
            };
            *slot += 1;
        }
        c
    }

    /// Finished (not running, not crashed).
    #[must_use]
    pub fn complete(&self) -> bool {
        self.meta.finished.is_some()
    }

    /// Open issues, for "new" and "resolved" between scans.
    #[must_use]
    pub fn issue_keys(&self) -> BTreeSet<IssueKey> {
        self.items
            .iter()
            .filter(|i| i.is_issue() && !i.accepted)
            .map(|i| {
                (
                    i.finding.sev,
                    i.finding.section.clone(),
                    i.finding.msg.clone(),
                )
            })
            .collect()
    }

    /// Line numbers of fixes applied in this scan.
    #[must_use]
    pub fn applied(&self) -> BTreeSet<usize> {
        fs::read_to_string(self.path.join("applied"))
            .unwrap_or_default()
            .split_whitespace()
            .filter_map(|n| n.parse().ok())
            .collect()
    }

    /// Records that the fix of finding `n` ran successfully.
    ///
    /// # Errors
    /// If the file can't be written.
    pub fn mark_applied(&self, n: usize) -> std::io::Result<()> {
        let mut f = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(self.path.join("applied"))?;
        writeln!(f, "{n}")
    }
}
