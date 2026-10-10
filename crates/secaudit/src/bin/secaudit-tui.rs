//! `secaudit-tui`: the Security Audit terminal app.

use std::process::ExitCode;

use clap::Parser;

/// Security Audit terminal app
#[derive(Parser)]
struct Args {
    /// start a scan immediately
    #[arg(long, value_parser = ["quick", "full", "deep"])]
    scan: Option<String>,
}

fn main() -> ExitCode {
    let args = Args::parse();
    match secaudit::tui::run(args.scan.as_deref()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            let _ = std::io::Write::write_fmt(
                &mut std::io::stderr(),
                format_args!("secaudit-tui: {e}\n"),
            );
            ExitCode::FAILURE
        }
    }
}
