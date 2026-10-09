//! The engine: who is audited, how findings are collected, and the score.
//!
//! Each report section is a plain function over [`Ctx`] that records
//! findings in a [`Report`], like the Bash helpers `ok`/`wn`/`bad`/`info`/`skp`.

use std::fs;
use std::path::{Path, PathBuf};

use crate::contract::{CONTRACT_SCHEMA, Finding, Severity, Target};

/// Who and what is being audited.
#[derive(Clone, Debug)]
#[non_exhaustive]
pub struct Ctx {
    /// The audited user's home. Under sudo this is the invoking user's.
    pub home: PathBuf,
    /// Running as root, so root-only checks run instead of being skipped.
    pub is_root: bool,
}

impl Ctx {
    /// The real machine: `SUDO_USER`'s home under sudo, else `$HOME`.
    #[must_use]
    pub fn from_env() -> Self {
        let home = std::env::var("SUDO_USER")
            .ok()
            .and_then(|u| passwd_home(&u))
            .or_else(|| std::env::var_os("HOME").map(PathBuf::from))
            .unwrap_or_default();
        Self {
            home,
            is_root: effective_uid() == Some(0),
        }
    }

    /// An audit of `home`, as a normal user. For tests and `--home`.
    #[must_use]
    pub fn for_home(home: impl Into<PathBuf>) -> Self {
        Self {
            home: home.into(),
            is_root: false,
        }
    }

    /// `path` with the home prefix shown as `~`, as the report prints it.
    #[must_use]
    pub fn tilde(&self, path: &Path) -> String {
        match path.strip_prefix(&self.home) {
            Ok(rest) if !self.home.as_os_str().is_empty() => format!("~/{}", rest.display()),
            _ => path.display().to_string(),
        }
    }
}

/// Home directory of `user` from `/etc/passwd`.
fn passwd_home(user: &str) -> Option<PathBuf> {
    let passwd = fs::read_to_string("/etc/passwd").ok()?;
    passwd.lines().find_map(|line| {
        let f: Vec<&str> = line.split(':').collect();
        (f.first() == Some(&user))
            .then(|| f.get(5).map(PathBuf::from))
            .flatten()
    })
}

/// Effective uid from `/proc/self/status`, without `unsafe` libc calls.
fn effective_uid() -> Option<u32> {
    let status = fs::read_to_string("/proc/self/status").ok()?;
    let uids = status.lines().find_map(|l| l.strip_prefix("Uid:"))?;
    uids.split_whitespace().nth(1)?.parse().ok()
}

/// Findings collected during a run, in report order.
#[derive(Debug, Default)]
pub struct Report {
    section: String,
    findings: Vec<Finding>,
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
    /// Sets the detail.
    pub fn detail(&mut self, text: impl Into<String>) -> &mut Self {
        self.detail = text.into();
        self
    }
}

/// One report section of the audit.
#[derive(Clone, Copy, Debug)]
#[non_exhaustive]
pub struct Section {
    /// Heading, exactly as the Bash version prints it.
    pub name: &'static str,
    /// Runs the section's checks.
    pub run: fn(&Ctx, &mut Report),
}

/// Runs `sections` in order.
#[must_use]
pub fn run(ctx: &Ctx, sections: &[Section]) -> Report {
    let mut report = Report::default();
    for s in sections {
        report.section(s.name);
        (s.run)(ctx, &mut report);
    }
    report
}

/// Quotes `s` for a shell command line, only when needed.
#[must_use]
pub fn shell_quote(s: &str) -> String {
    let safe = |c: char| c.is_ascii_alphanumeric() || "_./~@%+=:,-".contains(c);
    if !s.is_empty() && s.chars().all(safe) {
        s.to_owned()
    } else {
        format!("'{}'", s.replace('\'', r"'\''"))
    }
}

#[cfg(test)]
mod tests {
    use super::{Ctx, Report, shell_quote};
    use std::path::Path;

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

    #[test]
    fn tilde_only_replaces_the_home_prefix() {
        let ctx = Ctx::for_home("/home/u");
        assert_eq!(ctx.tilde(Path::new("/home/u/.netrc")), "~/.netrc");
        assert_eq!(ctx.tilde(Path::new("/home/user2/x")), "/home/user2/x");
    }

    #[test]
    fn quotes_only_when_needed() {
        assert_eq!(shell_quote("/home/u/.netrc"), "/home/u/.netrc");
        assert_eq!(shell_quote("/home/u/my key"), "'/home/u/my key'");
        assert_eq!(shell_quote("it's"), r"'it'\''s'");
        assert_eq!(shell_quote(""), "''");
    }
}
