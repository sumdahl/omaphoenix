use std::path::Path;

use crate::contract::{CONTRACT_SCHEMA, Finding, Severity, Target};

/// Findings collected during a run, in report order.
#[derive(Debug, Default)]
pub struct Report {
    section: String,
    pub(super) findings: Vec<Finding>,
}

impl Report {
    /// Starts a report section; later findings belong to it.
    pub fn section(&mut self, name: &str) {
        name.clone_into(&mut self.section);
    }

    fn push(&mut self, sev: Severity, id: &str, msg: String) -> &mut Finding {
        let index = self.findings.len();
        self.findings.push(Finding {
            schema: CONTRACT_SCHEMA,
            id: Some(id.to_owned()),
            sev,
            section: self.section.clone(),
            msg,
            detail: String::new(),
            fix: String::new(),
            hint: String::new(),
            targets: Vec::new(),
        });
        &mut self.findings[index]
    }

    /// Records a passing check.
    pub fn ok(&mut self, id: &str, msg: impl Into<String>) -> &mut Finding {
        self.push(Severity::Pass, id, msg.into())
    }
    /// Records a warning.
    pub fn warn(&mut self, id: &str, msg: impl Into<String>) -> &mut Finding {
        self.push(Severity::Warn, id, msg.into())
    }
    /// Records a failure.
    pub fn fail(&mut self, id: &str, msg: impl Into<String>) -> &mut Finding {
        self.push(Severity::Fail, id, msg.into())
    }
    /// Records context.
    pub fn info(&mut self, id: &str, msg: impl Into<String>) -> &mut Finding {
        self.push(Severity::Info, id, msg.into())
    }
    /// Records a check that could not run.
    pub fn skip(&mut self, id: &str, msg: impl Into<String>) -> &mut Finding {
        self.push(Severity::Skip, id, msg.into())
    }

    /// The latest finding, which Bash's `detail` attaches to.
    pub fn last(&mut self) -> Option<&mut Finding> {
        self.findings.last_mut()
    }

    /// Adds findings from another run, e.g. a cached section.
    pub fn merge(&mut self, findings: impl IntoIterator<Item = Finding>) {
        self.findings.extend(findings);
    }

    /// The findings so far.
    #[must_use]
    pub fn findings(&self) -> &[Finding] {
        &self.findings
    }

    /// `100 − 10×fail − 2×warn`, floored at 0, and its letter grade.
    #[must_use]
    pub fn score(&self) -> (u32, char) {
        let count = |s| {
            u32::try_from(self.findings.iter().filter(|f| f.sev == s).count()).unwrap_or(u32::MAX)
        };
        let lost = count(Severity::Fail)
            .saturating_mul(10)
            .saturating_add(count(Severity::Warn).saturating_mul(2));
        let score = 100u32.saturating_sub(lost);
        let grade = match score {
            90.. => 'A',
            80..=89 => 'B',
            70..=79 => 'C',
            60..=69 => 'D',
            _ => 'F',
        };
        (score, grade)
    }
}

impl Finding {
    /// Adds a target the finding is about.
    pub fn target(&mut self, target: Target) -> &mut Self {
        self.targets.push(target);
        self
    }
    /// Adds a file target.
    pub fn file(&mut self, path: &Path) -> &mut Self {
        self.target(Target::File {
            path: path.display().to_string(),
        })
    }
    /// Sets the fix command.
    pub fn fix(&mut self, cmd: impl Into<String>) -> &mut Self {
        self.fix = cmd.into();
        self
    }
    /// Sets the advice.
    pub fn hint(&mut self, text: impl Into<String>) -> &mut Self {
        self.hint = text.into();
        self
    }
    /// Appends a line to the detail, as Bash's `detail` does.
    pub fn add_detail(&mut self, text: &str) -> &mut Self {
        if !text.is_empty() {
            if !self.detail.is_empty() {
                self.detail.push('\n');
            }
            self.detail.push_str(text);
        }
        self
    }
    /// Sets the detail.
    pub fn detail(&mut self, text: impl Into<String>) -> &mut Self {
        self.detail = text.into();
        self
    }
}

#[cfg(test)]
mod tests {
    use super::Report;

    #[test]
    fn score_and_grade_follow_bash() {
        let mut r = Report::default();
        assert_eq!(r.score(), (100, 'A'));
        r.fail("a.b", "x");
        r.warn("a.b", "x");
        r.info("a.b", "x");
        r.ok("a.b", "x");
        assert_eq!(r.score(), (88, 'B'));
        for _ in 0..20 {
            r.fail("a.b", "x");
        }
        assert_eq!(r.score(), (0, 'F'));
    }
}
