use std::fs::File;
use std::io::{self, IsTerminal, Write};
use std::path::Path;

use phx_audit::Severity;
use phx_audit::audit::{self, Ctx, Progress};
use phx_audit::checks::SECTIONS;

use super::AuditArgs;
use super::output::{Out, Style, print_finding};

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

/// Runs and prints the audit; returns whether anything failed.
pub(super) fn audit_cli(cli: &AuditArgs) -> io::Result<bool> {
    let mut ctx = match (&cli.home, &cli.user) {
        (Some(home), _) => Ctx::for_home(home.clone()),
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
        let me = std::env::current_exe()
            .map_or_else(|_| "omaphoenix".into(), |p| p.display().to_string());
        out.line(&format!(
            "  {}Re-run with sudo for full coverage: sudo {me} audit{}",
            st.dim, st.reset
        ))?;
    }
    if let Some(path) = &cli.report {
        out.line(&format!("  Report written to {}", path.display()))?;
    }
    Ok(count(Severity::Fail) > 0)
}
