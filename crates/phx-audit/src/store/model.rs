use serde::{Deserialize, Serialize};

use crate::contract::{Finding, Severity};

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

/// `section|msg` with digit runs masked, so counts don't split one risk.
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

/// (sev, section, msg) of an open issue, for comparing scans.
pub type IssueKey = (Severity, String, String);

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
