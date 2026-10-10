//! Scan history and accepted risks under `$XDG_STATE_HOME/secaudit`, in the
//! same layout the Python `secaudit-tui` and Bash `secaudit-ui` use, so all
//! of them share one history during the transition.
//!
//! - `runs/<YYYYmmdd-HHMMSS>/`: `findings.jsonl`, `meta.json`, `output.txt`,
//!   and `applied` (line numbers of fixes applied).
//! - `accepted.json`: risks the user decided to live with, keyed by section
//!   and message with digits masked.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;
use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::contract::{Finding, Severity};

/// Scans kept; older complete scans are deleted after each new one.
pub const KEEP_RUNS: usize = 30;

/// Seconds since the epoch.
#[must_use]
pub fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

/// Where state lives.
#[derive(Clone, Debug)]
pub struct Store {
    /// `$XDG_STATE_HOME/secaudit`.
    pub dir: PathBuf,
}

/// One accepted risk, as `accepted.json` stores it.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct Accepted {
    /// Section of the finding.
    pub section: String,
    /// Its title (message without a trailing colon).
    pub msg: String,
    /// When it was accepted.
    #[serde(default)]
    pub at: u64,
}

/// `meta.json` of a scan.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq, Eq)]
pub struct Meta {
    /// `quick`, `full` or `deep`.
    #[serde(default)]
    pub mode: String,
    /// Start time.
    #[serde(default)]
    pub started: u64,
    /// Host name.
    #[serde(default)]
    pub host: String,
    /// Audited user.
    #[serde(default)]
    pub user: String,
    /// Kernel release.
    #[serde(default)]
    pub kernel: String,
    /// End time; absent while running or if the scan died.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub finished: Option<u64>,
}

/// A finding inside a stored scan.
#[derive(Clone, Debug)]
pub struct Item {
    /// 1-based line in `findings.jsonl`; stable within the scan.
    pub n: usize,
    /// The finding.
    pub finding: Finding,
    /// Matched an accepted risk, so it no longer counts.
    pub accepted: bool,
}

impl Item {
    /// A fail or warn.
    #[must_use]
    pub fn is_issue(&self) -> bool {
        matches!(self.finding.sev, Severity::Fail | Severity::Warn)
    }

    /// The message without a trailing colon.
    #[must_use]
    pub fn title(&self) -> &str {
        self.finding.msg.trim_end().trim_end_matches(':')
    }

    /// Key into `accepted.json`.
    #[must_use]
    pub fn accept_key(&self) -> String {
        accept_key(&self.finding.section, &self.finding.msg)
    }
}

/// `section|msg` with digits masked, so "2 line(s)" and "3 line(s)" are the
/// same risk across scans.
#[must_use]
pub fn accept_key(section: &str, msg: &str) -> String {
    let mut masked = String::new();
    let mut in_digits = false;
    for c in msg.trim().trim_end_matches(':').chars() {
        if c.is_ascii_digit() {
            if !in_digits {
                masked.push('#');
            }
            in_digits = true;
        } else {
            masked.push(c);
            in_digits = false;
        }
    }
    format!("{section}|{masked}")
}

/// Counts by severity, with accepted issues counted apart.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Counts {
    /// Fails not accepted.
    pub fail: usize,
    /// Warnings not accepted.
    pub warn: usize,
    /// Passes.
    pub pass: usize,
    /// Notes.
    pub info: usize,
    /// Skipped checks.
    pub skip: usize,
    /// Accepted issues.
    pub accepted: usize,
}

impl Counts {
    /// `100 − 10×fail − 2×warn`, floored at 0.
    #[must_use]
    pub fn score(&self) -> u32 {
        let lost = self
            .fail
            .saturating_mul(10)
            .saturating_add(self.warn.saturating_mul(2));
        u32::try_from(100usize.saturating_sub(lost)).unwrap_or(0)
    }

    /// A ≥ 90, B ≥ 80, C ≥ 70, D ≥ 60, else F.
    #[must_use]
    pub fn grade(&self) -> char {
        match self.score() {
            90.. => 'A',
            80..=89 => 'B',
            70..=79 => 'C',
            60..=69 => 'D',
            _ => 'F',
        }
    }
}

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

/// (sev, section, msg) of an open issue, for comparing scans.
pub type IssueKey = (Severity, String, String);

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

    /// Writes `report.md`, a checklist of open issues, and returns its path.
    ///
    /// # Errors
    /// If the file can't be written.
    pub fn export_markdown(&self, started: &str) -> std::io::Result<PathBuf> {
        let issues: Vec<&Item> = self
            .items
            .iter()
            .filter(|i| i.is_issue() && !i.accepted)
            .collect();
        let mut sections: Vec<&str> = Vec::new();
        for i in &issues {
            if !sections.contains(&i.finding.section.as_str()) {
                sections.push(&i.finding.section);
            }
        }
        let c = self.counts();
        let mut out = vec![
            format!("# Security audit: {}", self.meta.host),
            String::new(),
            format!(
                "{started} · {} scan · kernel {} · score **{} ({})** · {} fail · {} warn",
                self.meta.mode,
                if self.meta.kernel.is_empty() {
                    "?"
                } else {
                    &self.meta.kernel
                },
                c.score(),
                c.grade(),
                c.fail,
                c.warn
            ),
            String::new(),
        ];
        for sec in sections {
            out.push(format!("## {sec}"));
            out.push(String::new());
            let mut in_sec: Vec<&&Item> =
                issues.iter().filter(|i| i.finding.section == sec).collect();
            in_sec.sort_by_key(|i| sev_order(i.finding.sev));
            for i in in_sec {
                let f = &i.finding;
                out.push(format!(
                    "- [ ] **{}** {}",
                    sev_name(f.sev).to_uppercase(),
                    i.title()
                ));
                if !f.detail.is_empty() {
                    out.push(String::new());
                    out.push("  ```".into());
                    out.extend(f.detail.lines().map(|l| format!("  {l}")));
                    out.push("  ```".into());
                }
                if !f.fix.is_empty() {
                    out.push(String::new());
                    out.push(format!("  Fix: `{}`", f.fix));
                }
                if !f.hint.is_empty() {
                    out.push(String::new());
                    out.push(format!("  {}", f.hint));
                }
                out.push(String::new());
            }
        }
        let path = self.path.join("report.md");
        fs::write(&path, out.join("\n"))?;
        Ok(path)
    }
}

/// fail, warn, info, skip, pass: the order findings are listed in.
#[must_use]
pub fn sev_order(sev: Severity) -> u8 {
    match sev {
        Severity::Fail => 0,
        Severity::Warn => 1,
        Severity::Skip => 3,
        Severity::Pass => 4,
        _ => 2,
    }
}

/// `fail`, `warn`, `pass`, `info` or `skip`.
#[must_use]
pub fn sev_name(sev: Severity) -> &'static str {
    match sev {
        Severity::Fail => "fail",
        Severity::Warn => "warn",
        Severity::Pass => "pass",
        Severity::Skip => "skip",
        _ => "info",
    }
}

impl Store {
    /// `$XDG_STATE_HOME/secaudit`, or `~/.local/state/secaudit`.
    #[must_use]
    pub fn from_env() -> Self {
        let base = std::env::var_os("XDG_STATE_HOME")
            .filter(|v| !v.is_empty())
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|h| Path::new(&h).join(".local/state")))
            .unwrap_or_default();
        Self {
            dir: base.join("secaudit"),
        }
    }

    /// `runs/`.
    #[must_use]
    pub fn runs_dir(&self) -> PathBuf {
        self.dir.join("runs")
    }

    /// Accepted risks; empty if missing or unreadable.
    #[must_use]
    pub fn accepted(&self) -> BTreeMap<String, Accepted> {
        fs::read_to_string(self.dir.join("accepted.json"))
            .ok()
            .and_then(|t| serde_json::from_str(&t).ok())
            .unwrap_or_default()
    }

    /// Saves accepted risks atomically.
    ///
    /// # Errors
    /// If the file can't be written.
    pub fn save_accepted(&self, accepted: &BTreeMap<String, Accepted>) -> std::io::Result<()> {
        fs::create_dir_all(&self.dir)?;
        let body = serde_json::to_vec_pretty(accepted).map_err(std::io::Error::other)?;
        let tmp = self.dir.join(".accepted.json.tmp");
        fs::write(&tmp, body)?;
        fs::rename(tmp, self.dir.join("accepted.json"))
    }

    /// Complete scans, newest first.
    #[must_use]
    pub fn runs(&self) -> Vec<Run> {
        let accepted = self.accepted();
        let mut dirs: Vec<PathBuf> = fs::read_dir(self.runs_dir())
            .into_iter()
            .flatten()
            .flatten()
            .map(|e| e.path())
            .collect();
        dirs.sort();
        dirs.reverse();
        dirs.iter()
            .filter(|d| fs::metadata(d.join("findings.jsonl")).is_ok_and(|m| m.len() > 0))
            .filter_map(|d| Run::load(d, &accepted).ok())
            .filter(Run::complete)
            .collect()
    }

    /// Starts a scan directory with its `meta.json`.
    ///
    /// # Errors
    /// If the directory can't be created.
    pub fn begin(&self, stamp: &str, meta: &Meta) -> std::io::Result<PathBuf> {
        let dir = self.runs_dir().join(stamp);
        fs::create_dir_all(&dir)?;
        write_meta(&dir, meta)?;
        Ok(dir)
    }

    /// Marks a scan finished and drops scans beyond [`KEEP_RUNS`].
    ///
    /// # Errors
    /// If `meta.json` can't be written.
    pub fn finish(&self, dir: &Path, meta: &mut Meta) -> std::io::Result<()> {
        meta.finished = Some(now());
        write_meta(dir, meta)?;
        for old in self.runs().iter().skip(KEEP_RUNS) {
            let _ = fs::remove_dir_all(&old.path);
        }
        Ok(())
    }

    /// Runs a quick audit in-process (as the user, no sudo) and records it
    /// as a scan, so every front end shares the history. `stamp` names the
    /// run directory (`YYYYmmdd-HHMMSS`).
    ///
    /// # Errors
    /// If the run can't be written.
    pub fn record_quick_scan(
        &self,
        ctx: &crate::audit::Ctx,
        host: &str,
        kernel: &str,
        stamp: &str,
    ) -> std::io::Result<PathBuf> {
        self.record_quick_scan_with(ctx, host, kernel, stamp, |_, _, _| {})
    }

    /// [`Store::record_quick_scan`], reporting each section as it starts:
    /// `(sections done, sections in all, the one starting)`, for progress bars.
    ///
    /// # Errors
    /// If the run directory or its files can't be written.
    pub fn record_quick_scan_with(
        &self,
        ctx: &crate::audit::Ctx,
        host: &str,
        kernel: &str,
        stamp: &str,
        mut progress: impl FnMut(usize, usize, &str),
    ) -> std::io::Result<PathBuf> {
        let mut meta = Meta {
            mode: "quick".into(),
            started: now(),
            host: host.to_owned(),
            user: ctx.user.clone(),
            kernel: kernel.to_owned(),
            finished: None,
        };
        let dir = self.begin(stamp, &meta)?;
        let total = crate::checks::SECTIONS.len();
        let mut done = 0;
        let report = crate::audit::run_with(ctx, crate::checks::SECTIONS, |p| match p {
            crate::audit::Progress::Start(name) => progress(done, total, name),
            crate::audit::Progress::Done(..) => done += 1,
        });
        let mut f = std::io::BufWriter::new(fs::File::create(dir.join("findings.jsonl"))?);
        let mut summary = String::new();
        for finding in report.findings() {
            serde_json::to_writer(&mut f, finding).map_err(std::io::Error::other)?;
            f.write_all(b"\n")?;
            if matches!(finding.sev, Severity::Fail | Severity::Warn) {
                let _ = writeln!(
                    summary,
                    "[{}] {}: {}",
                    sev_name(finding.sev).to_uppercase(),
                    finding.section,
                    finding.msg
                );
            }
        }
        f.flush()?;
        fs::write(dir.join("output.txt"), summary)?;
        self.finish(&dir, &mut meta)?;
        Ok(dir)
    }

    /// Removes scans that never finished and are older than `min_age` seconds.
    pub fn prune_incomplete(&self, min_age: u64) {
        for e in fs::read_dir(self.runs_dir())
            .into_iter()
            .flatten()
            .flatten()
        {
            let dir = e.path();
            let meta: Meta = fs::read_to_string(dir.join("meta.json"))
                .ok()
                .and_then(|t| serde_json::from_str(&t).ok())
                .unwrap_or_default();
            let age = e
                .metadata()
                .ok()
                .and_then(|m| m.modified().ok())
                .and_then(|t| t.elapsed().ok())
                .map_or(0, |d| d.as_secs());
            if meta.finished.is_none() && age > min_age {
                let _ = fs::remove_dir_all(&dir);
            }
        }
    }
}

fn write_meta(dir: &Path, meta: &Meta) -> std::io::Result<()> {
    fs::write(
        dir.join("meta.json"),
        serde_json::to_vec(meta).map_err(std::io::Error::other)?,
    )
}

#[cfg(test)]
mod tests {
    use super::{Accepted, KEEP_RUNS, Meta, Store, accept_key};
    use std::collections::BTreeMap;
    use std::fs;

    fn store() -> (tempfile::TempDir, Store) {
        let t = tempfile::tempdir().unwrap_or_else(|e| unreachable!("{e}"));
        let s = Store {
            dir: t.path().join("secaudit"),
        };
        (t, s)
    }

    const LINES: &str = concat!(
        r#"{"sev":"fail","section":"Secrets & credentials in home","msg":"~/.netrc perms 644 (readable by others, want 600)","detail":"","fix":"chmod 600 ~/.netrc","hint":""}"#,
        "\n",
        r#"{"sev":"warn","section":"Secrets & credentials in home","msg":"~/.zsh_history: 2 line(s) look like secrets/tokens (values not shown)","detail":"","fix":"","hint":""}"#,
        "\n",
        r#"{"sev":"pass","section":"SSH","msg":"sshd not enabled","detail":"","fix":"","hint":""}"#,
        "\n"
    );

    fn scan(s: &Store, stamp: &str, findings: &str) -> std::path::PathBuf {
        let mut meta = Meta {
            mode: "quick".into(),
            started: 1,
            host: "h".into(),
            ..Meta::default()
        };
        let dir = s.begin(stamp, &meta).unwrap_or_default();
        fs::write(dir.join("findings.jsonl"), findings).ok();
        s.finish(&dir, &mut meta).ok();
        dir
    }

    #[test]
    fn accept_key_masks_numbers_like_python() {
        assert_eq!(
            accept_key("S", "~/.zsh_history: 12 line(s) look like secrets:"),
            "S|~/.zsh_history: # line(s) look like secrets"
        );
        assert_eq!(accept_key("S", "a 1 b 22"), accept_key("S", "a 3 b 4"));
    }

    #[test]
    fn runs_score_with_accepted_risks() {
        let (_t, s) = store();
        scan(&s, "20261010-010000", LINES);
        let runs = s.runs();
        assert_eq!(runs.len(), 1);
        let c = runs[0].counts();
        assert_eq!(
            (c.fail, c.warn, c.pass, c.score(), c.grade()),
            (1, 1, 1, 88, 'B')
        );

        let mut acc = BTreeMap::new();
        let warn = &runs[0].items[1];
        acc.insert(
            warn.accept_key(),
            Accepted {
                section: warn.finding.section.clone(),
                msg: warn.title().into(),
                at: 1,
            },
        );
        s.save_accepted(&acc).ok();
        let c = s.runs()[0].counts();
        assert_eq!((c.warn, c.accepted, c.score()), (0, 1, 90));
    }

    #[test]
    fn keeps_thirty_and_ignores_unfinished() {
        let (_t, s) = store();
        for i in 0..KEEP_RUNS + 2 {
            scan(&s, &format!("20261010-{i:06}"), LINES);
        }
        s.begin("20991231-000000", &Meta::default()).ok();
        let runs = s.runs();
        assert_eq!(runs.len(), KEEP_RUNS);
        assert!(
            runs[0]
                .path
                .ends_with(format!("20261010-{:06}", KEEP_RUNS + 1)),
            "newest first"
        );
    }

    #[test]
    fn compares_and_records_fixes_and_exports() {
        let (_t, s) = store();
        scan(&s, "20261010-000001", LINES);
        let fixed = LINES.lines().skip(1).collect::<Vec<_>>().join("\n");
        scan(&s, "20261010-000002", &fixed);
        let runs = s.runs();
        let resolved: Vec<_> = runs[1]
            .issue_keys()
            .difference(&runs[0].issue_keys())
            .cloned()
            .collect();
        assert_eq!(resolved.len(), 1);
        assert!(resolved[0].2.starts_with("~/.netrc"));

        runs[1].mark_applied(1).ok();
        assert!(runs[1].applied().contains(&1));
        let report = runs[1]
            .export_markdown("2026-10-10 01:00")
            .unwrap_or_default();
        let md = fs::read_to_string(report).unwrap_or_default();
        assert!(
            md.contains("- [ ] **FAIL** ~/.netrc perms 644 (readable by others, want 600)"),
            "{md}"
        );
        assert!(md.contains("  Fix: `chmod 600 ~/.netrc`"));
    }
}
