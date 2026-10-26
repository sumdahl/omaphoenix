//! `omaphoenix audit`: the report, the audit TUI and the menu.

use std::io::{self, Write};
use std::path::PathBuf;

use clap::{Args, Subcommand};
use phx_audit::audit::Ctx;

mod audit;
mod output;

/// Flags of the report (`omaphoenix audit`); the views ignore them.
#[derive(Args, Debug)]
pub struct AuditArgs {
    /// Also verify every package file checksum and scan / for SUID/SGID (slow, several minutes)
    #[arg(long)]
    pub deep: bool,
    /// Write a plain-text copy of the output to FILE
    #[arg(long, value_name = "FILE")]
    pub report: Option<PathBuf>,
    /// Write findings as JSON lines (finding contract v1) to FILE
    #[arg(long, value_name = "FILE")]
    pub json: Option<PathBuf>,
    /// Disable coloured output
    #[arg(long)]
    pub no_color: bool,
    /// Audit this home instead of the invoking user's (e.g. a restored home)
    #[arg(long, value_name = "DIR")]
    pub home: Option<PathBuf>,
    /// Audit NAME's home folder (default: the user who ran sudo)
    #[arg(long, value_name = "NAME")]
    pub user: Option<String>,
}

/// The audit's interactive views.
#[derive(Subcommand, Debug)]
pub enum AuditView {
    /// Terminal app: scan, review, accept risks, run fixes, history
    Tui {
        /// Start a scan right away
        #[arg(long, value_parser = ["quick", "full", "deep"])]
        scan: Option<String>,
    },
    /// Menus in the terminal, gum-style: scan, review, fix, history
    Menu {
        /// `--scan quick|full|deep`, `--last`
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },
}

/// Runs the report or a view; returns the process exit code (1 when anything fails, 2 on error).
#[must_use]
pub fn run(args: &AuditArgs, view: Option<AuditView>) -> u8 {
    match view {
        Some(AuditView::Tui { scan }) => match crate::tui::run(scan.as_deref()) {
            Ok(()) => 0,
            Err(e) => {
                let _ = writeln!(io::stderr(), "omaphoenix audit: {e}");
                1
            }
        },
        Some(AuditView::Menu { args }) => crate::ui::main(&args),
        None => report(args),
    }
}

fn report(args: &AuditArgs) -> u8 {
    if let Some(name) = args.user.as_deref().filter(|_| args.home.is_none())
        && Ctx::for_user(name).is_none()
    {
        let _ = writeln!(io::stderr(), "Unknown user: {name}");
        return 2;
    }
    match audit::audit_cli(args) {
        Ok(true) => 1,
        Ok(false) => 0,
        Err(e) => {
            let _ = writeln!(io::stderr(), "omaphoenix audit: {e}");
            2
        }
    }
}
