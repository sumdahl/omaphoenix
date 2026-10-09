//! Security Audit: a read-only security audit for Arch and Omarchy.
//!
//! Status: skeleton (issue #2). Commands arrive with the roadmap tasks.

use clap::Parser;

/// Security Audit (skeleton)
#[derive(Parser)]
#[command(version, about)]
struct Cli {}

fn main() {
    let _cli = Cli::parse();
}

#[cfg(test)]
mod tests {
    use clap::CommandFactory;

    #[test]
    fn cli_definition_is_valid() {
        super::Cli::command().debug_assert();
    }
}
