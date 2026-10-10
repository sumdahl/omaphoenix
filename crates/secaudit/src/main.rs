//! Security Audit: a read-only security audit for Arch and Omarchy.
//!
//! `secaudit` prints the report, like the Bash engine it replaces, and
//! accepts the same options. `secaudit tui` opens the interactive app.

use std::fs::File;
use std::io::{self, IsTerminal, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clap::{Parser, Subcommand};
use secaudit_core::audit::{self, Ctx, Progress};
use secaudit_core::checks::SECTIONS;
use secaudit_core::{Finding, Severity};

use secaudit::tui;

/// Read-only security audit. Never modifies the system. Run with sudo for
/// full coverage (shadow, sudoers, root-owned configs).
#[derive(Parser)]
#[command(version, about)]
struct Cli {
    /// Also verify every package file checksum and scan / for SUID/SGID
    /// (slow, several minutes).
    #[arg(long)]
    deep: bool,
    /// Write a plain-text copy of the output to FILE.
    #[arg(long, value_name = "FILE")]
    report: Option<PathBuf>,
    /// Write findings as JSON lines (finding contract v1) to FILE.
    #[arg(long, value_name = "FILE")]
    json: Option<PathBuf>,
    /// Disable coloured output.
    #[arg(long)]
    no_color: bool,
    /// Audit this home instead of the invoking user's (e.g. a restored home).
    #[arg(long, value_name = "DIR")]
    home: Option<PathBuf>,
    /// Audit NAME's home folder (default: the user who ran sudo).
    #[arg(long, value_name = "NAME")]
    user: Option<String>,
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand)]
enum Command {
    /// Interactive app: scan, review, accept risks, apply fixes, history.
    Tui {
        /// Start a scan right away.
        #[arg(long, value_parser = ["quick", "full", "deep"])]
        scan: Option<String>,
    },
}

/// Colours, or empty strings when output isn't a terminal.
struct Style {
    red: &'static str,
    green: &'static str,
    yellow: &'static str,
    blue: &'static str,
    dim: &'static str,
    bold: &'static str,
    reset: &'static str,
}

impl Style {
    fn new(color: bool) -> Self {
        let c = |code| if color { code } else { "" };
        Self {
            red: c("\x1b[31m"),
            green: c("\x1b[32m"),
            yellow: c("\x1b[33m"),
            blue: c("\x1b[34m"),
            dim: c("\x1b[2m"),
            bold: c("\x1b[1m"),
            reset: c("\x1b[0m"),
        }
    }
}

/// Writes to the terminal and, with `--report`, to a colourless copy.
struct Out {
    report: Option<File>,
}

impl Out {
    fn line(&mut self, s: &str) -> io::Result<()> {
        let mut stdout = io::stdout().lock();
        writeln!(stdout, "{s}")?;
        if let Some(f) = &mut self.report {
            writeln!(f, "{}", strip_ansi(s))?;
        }
        Ok(())
    }
}

fn strip_ansi(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        if c == '\x1b' {
            for c in chars.by_ref() {
                if c.is_ascii_alphabetic() {
                    break;
                }
            }
        } else {
            out.push(c);
        }
    }
    out
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    if let Some(Command::Tui { scan }) = cli.command {
        return match tui::run(scan.as_deref()) {
            Ok(()) => ExitCode::SUCCESS,
            Err(e) => {
                let _ = writeln!(io::stderr(), "secaudit: {e}");
                ExitCode::from(1)
            }
        };
    }
    if let Some(name) = cli.user.as_deref().filter(|_| cli.home.is_none())
        && Ctx::for_user(name).is_none()
    {
        let _ = writeln!(io::stderr(), "Unknown user: {name}");
        return ExitCode::from(2);
    }
    let code = match audit_cli(&cli) {
        Ok(failed) if failed => ExitCode::FAILURE,
        Ok(_) => ExitCode::SUCCESS,
        Err(e) => {
            let _ = writeln!(io::stderr(), "secaudit: {e}");
            ExitCode::from(2)
        }
    };
    report_hwm();
    code
}

/// With `SECAUDIT_REPORT_HWM` set, prints this process's own peak RSS
/// (`VmHWM`) to stderr, for `scripts/peak-rss`: wait4's figure also counts
/// children such as `checkupdates`.
fn report_hwm() {
    if std::env::var_os("SECAUDIT_REPORT_HWM").is_none() {
        return;
    }
    let status = std::fs::read_to_string("/proc/self/status").unwrap_or_default();
    if let Some(hwm) = status.lines().find(|l| l.starts_with("VmHWM:")) {
        let _ = writeln!(io::stderr(), "secaudit {hwm}");
    }
}

fn command_line(argv: &[&str]) -> String {
    std::process::Command::new(argv[0])
        .args(&argv[1..])
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_owned())
        .unwrap_or_default()
}

fn os_name() -> String {
    std::fs::read_to_string("/etc/os-release")
        .unwrap_or_default()
        .lines()
        .find_map(|l| l.strip_prefix("PRETTY_NAME="))
        .map(|v| v.trim_matches('"').to_owned())
        .unwrap_or_default()
}

/// Creates FILE (and its directory), owned by the invoking user under sudo.
fn create_for_user(path: &Path) -> io::Result<File> {
    if let Some(dir) = path.parent().filter(|d| !d.as_os_str().is_empty()) {
        std::fs::create_dir_all(dir)?;
    }
    let file = File::create(path)?;
    if let Ok(user) = std::env::var("SUDO_USER") {
        let _ = std::process::Command::new("chown")
            .arg(format!("{user}:"))
            .arg(path)
            .status();
    }
    Ok(file)
}

/// Runs the audit, printing as each section finishes. Returns whether anything failed.
fn audit_cli(cli: &Cli) -> io::Result<bool> {
    let mut ctx = match (&cli.home, &cli.user) {
        (Some(home), _) => Ctx::for_home(home.clone()),
        // main() has already checked that the user exists.
        (None, Some(name)) => Ctx::for_user(name).unwrap_or_else(Ctx::from_env),
        (None, None) => Ctx::from_env(),
    };
    ctx.deep = cli.deep;
    let st = Style::new(!cli.no_color && io::stdout().is_terminal());
    let mut out = Out {
        report: cli.report.as_deref().map(create_for_user).transpose()?,
    };
    let mut json = cli.json.as_deref().map(create_for_user).transpose()?;

    let host = command_line(&["hostname"]);
    let root = if ctx.is_root { "yes" } else { "no" };
    let date = command_line(&["date", "-Is"]);
    out.line(&format!(
        "{}Security audit{}  host={host}  user={}  root={root}  {date}",
        st.bold, st.reset, ctx.user
    ))?;
    out.line(&format!(
        "{}Kernel {} | {}{}",
        st.dim,
        command_line(&["uname", "-r"]),
        os_name(),
        st.reset
    ))?;

    let mut io_error = None;
    let report = audit::run_with(&ctx, SECTIONS, |progress| {
        let mut emit = || -> io::Result<()> {
            let findings = match progress {
                Progress::Start(name) => {
                    out.line(&format!("\n{}{}== {name} =={}", st.bold, st.blue, st.reset))?;
                    return io::stdout().flush();
                }
                Progress::Done(_, findings) => findings,
                _ => return Ok(()),
            };
            for f in findings {
                print_finding(&mut out, &st, f)?;
                if let Some(j) = &mut json {
                    serde_json::to_writer(&mut *j, f)?;
                    j.write_all(b"\n")?;
                }
            }
            io::stdout().flush()
        };
        if let Err(e) = emit() {
            io_error.get_or_insert(e);
        }
    });
    if let Some(e) = io_error {
        return Err(e);
    }

    let count = |s: Severity| report.findings().iter().filter(|f| f.sev == s).count();
    out.line(&format!(
        "\n{}{}== Summary =={}",
        st.bold, st.blue, st.reset
    ))?;
    out.line(&format!(
        "  {}PASS {}{}   {}WARN {}{}   {}FAIL {}{}   {}SKIP {}{}",
        st.green,
        count(Severity::Pass),
        st.reset,
        st.yellow,
        count(Severity::Warn),
        st.reset,
        st.red,
        count(Severity::Fail),
        st.reset,
        st.dim,
        count(Severity::Skip),
        st.reset
    ))?;
    let (score, grade) = report.score();
    out.line(&format!("  Score {score}/100, grade {grade}"))?;
    if !ctx.is_root {
        let me = std::env::args().next().unwrap_or_else(|| "secaudit".into());
        out.line(&format!(
            "  {}Re-run with sudo for full coverage: sudo {me}{}",
            st.dim, st.reset
        ))?;
    }
    if let Some(path) = &cli.report {
        out.line(&format!("  Report written to {}", path.display()))?;
    }
    Ok(count(Severity::Fail) > 0)
}

fn print_finding(out: &mut Out, st: &Style, f: &Finding) -> io::Result<()> {
    let (tag, color) = match f.sev {
        Severity::Pass => ("PASS", st.green),
        Severity::Warn => ("WARN", st.yellow),
        Severity::Fail => ("FAIL", st.red),
        Severity::Skip => ("SKIP", st.dim),
        _ => ("INFO", st.blue),
    };
    out.line(&format!("  {color}[{tag}]{} {}", st.reset, f.msg))?;
    for line in f.detail.lines() {
        out.line(&format!("         {line}"))?;
    }
    if !f.fix.is_empty() {
        out.line(&format!("         {}> fix: {}{}", st.dim, f.fix, st.reset))?;
    }
    if !f.hint.is_empty() {
        out.line(&format!("         {}> {}{}", st.dim, f.hint, st.reset))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use clap::CommandFactory;

    #[test]
    fn cli_definition_is_valid() {
        super::Cli::command().debug_assert();
    }

    #[test]
    fn strips_colours_for_the_report_copy() {
        assert_eq!(
            super::strip_ansi("\x1b[1m\x1b[34m== A ==\x1b[0m"),
            "== A =="
        );
    }
}
