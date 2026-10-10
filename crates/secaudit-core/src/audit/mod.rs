//! The engine: sections are functions over [`Ctx`] that record into a [`Report`].

mod command;
mod ctx;
mod files;
mod report;

pub use command::{CmdOut, CommandRunner, MAX_OUTPUT, System, TIMEOUT};
pub use ctx::Ctx;
pub use files::{MAX_TEXT, read_text, walk};
pub use report::Report;

use crate::contract::Finding;

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
    run_with(ctx, sections, |_| {})
}

/// Progress of a run, for front ends that stream the report.
#[derive(Debug)]
#[non_exhaustive]
pub enum Progress<'a> {
    /// A section is starting.
    Start(&'a str),
    /// A section finished with these findings.
    Done(&'a str, &'a [Finding]),
}

/// Runs `sections` in order, reporting progress so the report can stream.
pub fn run_with(ctx: &Ctx, sections: &[Section], mut progress: impl FnMut(Progress<'_>)) -> Report {
    let mut report = Report::default();
    for s in sections {
        progress(Progress::Start(s.name));
        let start = report.findings.len();
        report.section(s.name);
        (s.run)(ctx, &mut report);
        progress(Progress::Done(
            s.name,
            report.findings.get(start..).unwrap_or_default(),
        ));
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
    use super::shell_quote;

    #[test]
    fn quotes_only_when_needed() {
        assert_eq!(shell_quote("/home/u/.netrc"), "/home/u/.netrc");
        assert_eq!(shell_quote("/home/u/my key"), "'/home/u/my key'");
        assert_eq!(shell_quote("it's"), r"'it'\''s'");
        assert_eq!(shell_quote(""), "''");
    }
}
