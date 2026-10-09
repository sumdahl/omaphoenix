//! Security Audit: a read-only security audit for Arch and Omarchy.
//!
//! Status: the Rust port is in progress (issue #5); only the sections in
//! `secaudit_core::checks::SECTIONS` run so far. The Bash `secaudit` stays
//! the full audit until parity.

use std::fs::File;
use std::io::{self, BufWriter, IsTerminal, Write};
use std::path::PathBuf;
use std::process::ExitCode;

use clap::Parser;
use secaudit_core::audit::{self, Ctx};
use secaudit_core::checks::SECTIONS;
use secaudit_core::{Finding, Severity};

/// Read-only security audit. Never modifies the system.
#[derive(Parser)]
#[command(version, about)]
struct Cli {
    /// Write findings as JSON lines (finding contract v1) to FILE.
    #[arg(long, value_name = "FILE")]
    json: Option<PathBuf>,
    /// Audit this home instead of the invoking user's (e.g. a restored home).
    #[arg(long, value_name = "DIR")]
    home: Option<PathBuf>,
    /// Disable coloured output.
    #[arg(long)]
    no_color: bool,
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    let ctx = cli.home.map_or_else(Ctx::from_env, Ctx::for_home);
    let report = audit::run(&ctx, SECTIONS);
    let color = !cli.no_color && io::stdout().is_terminal();
    let result = print(report.findings(), report.score(), color).and_then(|()| match &cli.json {
        Some(path) => write_json(path, report.findings()),
        None => Ok(()),
    });
    if let Err(e) = result {
        let _ = writeln!(io::stderr(), "secaudit: {e}");
        return ExitCode::from(2);
    }
    if report.findings().iter().any(|f| f.sev == Severity::Fail) {
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    }
}

/// The human report, laid out like the Bash version's.
fn print(findings: &[Finding], (score, grade): (u32, char), color: bool) -> io::Result<()> {
    let paint = |code: &'static str| if color { code } else { "" };
    let (bold, reset, dim) = (paint("\x1b[1m\x1b[34m"), paint("\x1b[0m"), paint("\x1b[2m"));
    let mut out = BufWriter::new(io::stdout().lock());
    let mut section = "";
    for f in findings {
        if f.section != section {
            section = &f.section;
            writeln!(out, "\n{bold}== {section} =={reset}")?;
        }
        let (tag, c) = match f.sev {
            Severity::Pass => ("PASS", paint("\x1b[32m")),
            Severity::Warn => ("WARN", paint("\x1b[33m")),
            Severity::Fail => ("FAIL", paint("\x1b[31m")),
            Severity::Skip => ("SKIP", dim),
            _ => ("INFO", paint("\x1b[34m")),
        };
        writeln!(out, "  {c}[{tag}]{reset} {}", f.msg)?;
        for line in f.detail.lines() {
            writeln!(out, "         {line}")?;
        }
        if !f.fix.is_empty() {
            writeln!(out, "         {dim}> fix: {}{reset}", f.fix)?;
        }
        if !f.hint.is_empty() {
            writeln!(out, "         {dim}> {}{reset}", f.hint)?;
        }
    }
    writeln!(
        out,
        "\n{bold}== Summary =={reset}\n  Score {score}/100, grade {grade}"
    )?;
    out.flush()
}

fn write_json(path: &PathBuf, findings: &[Finding]) -> io::Result<()> {
    let mut out = BufWriter::new(File::create(path)?);
    for f in findings {
        serde_json::to_writer(&mut out, f)?;
        out.write_all(b"\n")?;
    }
    out.flush()
}

#[cfg(test)]
mod tests {
    use clap::CommandFactory;

    #[test]
    fn cli_definition_is_valid() {
        super::Cli::command().debug_assert();
    }
}
