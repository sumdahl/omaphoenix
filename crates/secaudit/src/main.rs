//! `secaudit`: prints the audit report, or opens the TUI with `secaudit tui`.

use std::io::{self, Write};
use std::process::ExitCode;

use clap::Parser;
use secaudit::tui;
use secaudit_core::audit::Ctx;

mod cli;

use cli::{Cli, Command, audit_cli};

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

/// Prints own `VmHWM` for `scripts/peak-rss`; wait4's figure would include children.
fn report_hwm() {
    if std::env::var_os("SECAUDIT_REPORT_HWM").is_none() {
        return;
    }
    let status = std::fs::read_to_string("/proc/self/status").unwrap_or_default();
    if let Some(hwm) = status.lines().find(|l| l.starts_with("VmHWM:")) {
        let _ = writeln!(io::stderr(), "secaudit {hwm}");
    }
}
