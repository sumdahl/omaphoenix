//! `secaudit-ui`: run scans, review findings, apply fixes, browse history.

use std::process::ExitCode;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    ExitCode::from(secaudit::ui::main(&args))
}
