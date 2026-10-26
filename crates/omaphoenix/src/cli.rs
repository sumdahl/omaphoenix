use clap::{Parser, Subcommand};

/// Timestamped Omarchy snapshots that survive a reinstall
#[derive(Parser)]
#[command(name = "omaphoenix", version, about)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Cmd>,
}

#[derive(Subcommand)]
pub enum Cmd {
    /// Open the terminal app (also what bare `omaphoenix` does on a terminal)
    Tui,
    /// Open the desktop app (omaphoenix-gui, installed alongside)
    Gui,
    /// Create the local snapshot repo, optionally with a private GitHub remote
    Init {
        /// Existing git remote URL for snapshots
        #[arg(long, default_value = "")]
        remote: String,
        /// Create this private GitHub repo (owner/name) as the remote
        #[arg(long, default_value = "")]
        github: String,
    },
    /// Choose what snapshots capture (opens the Backup screen)
    Select {
        /// Keep everything detected, without asking (scripts)
        #[arg(long)]
        all: bool,
    },
    /// Capture the current setup; commits only if something changed
    Snapshot,
    /// Show the snapshot timeline
    List,
    /// Apply a snapshot to this machine (latest unless --at)
    ///
    /// Name items to restore only those, e.g. a config dir from last Tuesday:
    ///   omaphoenix restore .config/hypr --at 2026-10-06 --only config
    /// Items are globs over item ids; a directory selects everything under it.
    /// Use --dry-run with --at to see how a snapshot differs from this machine.
    Restore {
        /// Items to restore (globs over item ids)
        items: Vec<String>,
        /// Snapshot id or timestamp prefix, e.g. 2026-10-09T14
        #[arg(long, default_value = "")]
        at: String,
        /// Show what would change, change nothing
        #[arg(long)]
        dry_run: bool,
        /// Restore only these categories (pkg-repo,pkg-aur,plugin,theme,config,local,bin,pkg-other,secret,etc,service)
        #[arg(long, value_delimiter = ',')]
        only: Vec<String>,
        /// Also restore hardware-specific settings onto different hardware
        #[arg(long)]
        all_hardware: bool,
        /// Don't restart the Omarchy shell afterwards (TTY/SSH restores)
        #[arg(long)]
        no_restart: bool,
        /// Clone snapshots from this git URL first (fresh machine)
        #[arg(long, default_value = "")]
        from: String,
    },
    /// Reverse the last restore (files, plugins, themes, /etc, services)
    ///
    /// Replaced files get their previous version back. Files and directories
    /// the restore created go to an undo trash, never deleted. A file you
    /// edited after the restore is left alone. Installed packages are listed
    /// with the command to remove them, not removed.
    Undo {
        /// Show what would be reversed, change nothing
        #[arg(long)]
        dry_run: bool,
    },
    /// Security audit of this machine: the report, or `tui` / `menu`
    ///
    /// Read-only; never changes the system. Run with sudo for full coverage.
    /// Audit engine by Manish Karki.
    Audit {
        #[command(flatten)]
        args: phx_audit_ui::cli::AuditArgs,
        #[command(subcommand)]
        view: Option<phx_audit_ui::cli::AuditView>,
    },
    /// What would be lost if this machine were reinstalled right now
    Doctor {
        /// Machine-readable output
        #[arg(long)]
        json: bool,
        /// Desktop notification about backup health only (login hook), at
        /// most once a day per problem
        #[arg(long)]
        notify: bool,
    },
    /// After a restore: audit this machine and compare it with the restored snapshot's audit
    Reaudit,
    /// Encrypted SSH keys and credentials (opt-in)
    Secrets {
        #[command(subcommand)]
        command: SecretsCmd,
    },
    /// Automatic snapshots on a systemd user timer
    Timer {
        #[command(subcommand)]
        command: TimerCmd,
    },
}

#[derive(Subcommand)]
pub enum SecretsCmd {
    /// Create the vault key and include ~/.ssh in snapshots, encrypted
    Init,
}

#[derive(Subcommand)]
pub enum TimerCmd {
    /// Snapshot every interval (15m, 1h, 30m, 6h, daily; default 30m)
    On {
        /// Interval
        #[arg(default_value = "30m")]
        interval: String,
    },
    /// Stop automatic snapshots
    Off,
    /// Show the timer state and next run
    Status,
}

#[cfg(test)]
mod tests {
    use clap::CommandFactory;

    #[test]
    fn cli_definition_is_valid() {
        super::Cli::command().debug_assert();
    }
}
