use std::path::PathBuf;

use clap::{Parser, Subcommand};

mod audit;
mod output;

pub(crate) use audit::audit_cli;

/// Read-only security audit. Never modifies the system. Run with sudo for
/// full coverage (shadow, sudoers, root-owned configs).
#[derive(Parser)]
#[command(version, about)]
pub(crate) struct Cli {
    /// Also verify every package file checksum and scan / for SUID/SGID
    /// (slow, several minutes).
    #[arg(long)]
    pub(crate) deep: bool,
    /// Write a plain-text copy of the output to FILE.
    #[arg(long, value_name = "FILE")]
    pub(crate) report: Option<PathBuf>,
    /// Write findings as JSON lines (finding contract v1) to FILE.
    #[arg(long, value_name = "FILE")]
    pub(crate) json: Option<PathBuf>,
    /// Disable coloured output.
    #[arg(long)]
    pub(crate) no_color: bool,
    /// Audit this home instead of the invoking user's (e.g. a restored home).
    #[arg(long, value_name = "DIR")]
    pub(crate) home: Option<PathBuf>,
    /// Audit NAME's home folder (default: the user who ran sudo).
    #[arg(long, value_name = "NAME")]
    pub(crate) user: Option<String>,
    #[command(subcommand)]
    pub(crate) command: Option<Command>,
}

#[derive(Subcommand)]
pub(crate) enum Command {
    /// Interactive app: scan, review, accept risks, apply fixes, history.
    Tui {
        /// Start a scan right away.
        #[arg(long, value_parser = ["quick", "full", "deep"])]
        scan: Option<String>,
    },
}

#[cfg(test)]
mod tests {
    use clap::CommandFactory;

    #[test]
    fn cli_definition_is_valid() {
        super::Cli::command().debug_assert();
    }
}
