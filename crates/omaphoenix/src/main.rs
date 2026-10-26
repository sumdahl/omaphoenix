//! OmaPhoenix: timestamped Omarchy snapshots that survive a reinstall.

use std::io::{self, Write};
use std::process::ExitCode;

use clap::Parser;
use phx_core::sys;

mod cli;
mod commands;
mod out;
mod prompt;
mod tui;

fn main() -> ExitCode {
    // Never let git block on a credential prompt mid-restore.
    sys::set_child_env("GIT_TERMINAL_PROMPT", "0");
    // Omarchy's commands expect OMARCHY_PATH, which SSH and systemd don't set.
    if std::env::var_os("OMARCHY_PATH").is_none()
        && std::path::Path::new("/usr/share/omarchy").exists()
    {
        sys::set_child_env("OMARCHY_PATH", "/usr/share/omarchy");
    }
    let cli = cli::Cli::parse();
    if let Some(cli::Cmd::Audit { args, view }) = cli.command {
        let code = phx_audit_ui::cli::run(&args, view);
        out::report_hwm();
        return ExitCode::from(code);
    }
    let code = match commands::run(cli) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            let _ = writeln!(io::stderr(), "error: {e}");
            ExitCode::FAILURE
        }
    };
    out::report_hwm();
    code
}
