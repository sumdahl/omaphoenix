//! OmaPhoenix: timestamped Omarchy snapshots that survive a reinstall.
//!
//! The command line of the Go version, same commands, flags and output, on
//! the Rust engine (`phx-core`). With no command on a terminal it opens the
//! TUI.

use std::fmt::Write as _;
use std::io::{self, IsTerminal, Write};
use std::process::ExitCode;

use clap::{Parser, Subcommand};
use phx_core::store::Repo;
use phx_core::{App, RestoreOptions, StageReport, schedule, sys};
use zeroize::Zeroizing;

mod tui;

/// Timestamped Omarchy snapshots that survive a reinstall
#[derive(Parser)]
#[command(name = "omaphoenix", version, about)]
struct Cli {
    #[command(subcommand)]
    command: Option<Cmd>,
}

#[derive(Subcommand)]
enum Cmd {
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
enum SecretsCmd {
    /// Create the vault key and include ~/.ssh in snapshots, encrypted
    Init,
}

#[derive(Subcommand)]
enum TimerCmd {
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

/// Prints like the Go version: warnings to stderr, steps with a marker.
fn log(level: &str, msg: &str) {
    let _ = match level {
        "warn" => writeln!(io::stderr(), "  ! {msg}"),
        "step" => writeln!(io::stdout(), "▸ {msg}"),
        _ => writeln!(io::stdout(), "  {msg}"),
    };
}

fn say(msg: &str) {
    let _ = writeln!(io::stdout(), "{msg}");
}

/// A desktop notification when nobody watches the output (timer runs,
/// update hooks), so a failure doesn't go unseen.
fn notify(title: &str, body: &str) {
    if io::stdout().is_terminal() || !sys::has("notify-send") {
        return;
    }
    let _ = sys::output(
        None,
        &[
            "notify-send",
            "-u",
            "critical",
            "-a",
            "omaphoenix",
            title,
            body,
        ],
    );
}

/// Reads a line from the terminal without echo, via crossterm raw mode.
fn read_password(prompt: &str) -> Result<Zeroizing<String>, String> {
    use ratatui::crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
    use ratatui::crossterm::terminal;
    let _ = write!(io::stderr(), "{prompt}");
    let _ = io::stderr().flush();
    terminal::enable_raw_mode().map_err(|e| e.to_string())?;
    let mut s = Zeroizing::new(String::new());
    let result = loop {
        match event::read() {
            Ok(Event::Key(k)) if k.kind == KeyEventKind::Press => match k.code {
                KeyCode::Enter => break Ok(()),
                KeyCode::Char('c') if k.modifiers.contains(KeyModifiers::CONTROL) => {
                    break Err("cancelled".to_owned());
                }
                KeyCode::Backspace => {
                    s.pop();
                }
                KeyCode::Char(c) => s.push(c),
                _ => {}
            },
            Ok(_) => {}
            Err(e) => break Err(e.to_string()),
        }
    };
    let _ = terminal::disable_raw_mode();
    let _ = writeln!(io::stderr());
    result.map(|()| s)
}

/// `OMAPHOENIX_PASSPHRASE` (scripts, tests), else the terminal without echo.
fn passphrase(confirm: bool) -> Result<Zeroizing<String>, String> {
    if let Ok(p) = std::env::var("OMAPHOENIX_PASSPHRASE")
        && !p.is_empty()
    {
        return Ok(Zeroizing::new(p));
    }
    if !io::stdin().is_terminal() {
        return Err(
            "no terminal to ask for the secrets passphrase; set OMAPHOENIX_PASSPHRASE".into(),
        );
    }
    let p = read_password("secrets passphrase: ")?;
    if !confirm {
        return Ok(p);
    }
    if p.chars().count() < 12 {
        return Err("use at least 12 characters; this passphrase guards your SSH keys".into());
    }
    if *read_password("repeat passphrase: ")? != *p {
        return Err("passphrases don't match".into());
    }
    Ok(p)
}

fn main() -> ExitCode {
    // SAFETY-free env setup: these run before any thread is spawned.
    // Never let git block on a credential prompt mid-restore.
    set_env("GIT_TERMINAL_PROMPT", "0");
    // Omarchy's commands expect OMARCHY_PATH from the login environment;
    // over SSH or from a systemd unit there is none.
    if std::env::var_os("OMARCHY_PATH").is_none()
        && std::path::Path::new("/usr/share/omarchy").exists()
    {
        set_env("OMARCHY_PATH", "/usr/share/omarchy");
    }
    let cli = Cli::parse();
    let code = match run(cli) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            let _ = writeln!(io::stderr(), "error: {e}");
            ExitCode::FAILURE
        }
    };
    report_hwm();
    code
}

/// With `OMAPHOENIX_REPORT_HWM` set, prints this process's own peak RSS
/// (`VmHWM`) to stderr. `scripts/peak-rss` uses it: wait4's figure also
/// counts children, and `npm ls` alone peaks at ~126 MB.
fn report_hwm() {
    if std::env::var_os("OMAPHOENIX_REPORT_HWM").is_none() {
        return;
    }
    let status = std::fs::read_to_string("/proc/self/status").unwrap_or_default();
    if let Some(hwm) = status.lines().find(|l| l.starts_with("VmHWM:")) {
        let _ = writeln!(io::stderr(), "omaphoenix {hwm}");
    }
}

/// Replaces this process with `omaphoenix-gui` from the same directory.
fn open_gui() -> Result<(), String> {
    use std::os::unix::process::CommandExt;
    let gui = std::env::current_exe()
        .ok()
        .and_then(|e| e.parent().map(|d| d.join("omaphoenix-gui")))
        .filter(|g| g.is_file())
        .ok_or("the desktop app isn't installed; rerun install.sh and choose it (or use: omaphoenix tui)")?;
    Err(format!(
        "{}: {}",
        gui.display(),
        std::process::Command::new(&gui).exec()
    ))
}

/// Sets a variable for this process and its children.
fn set_env(key: &str, value: &str) {
    // ponytail: std::env::set_var is unsafe in edition 2024 and unsafe is
    // forbidden; children get these through Command::env instead. Recorded
    // here so sys can add them.
    sys::set_child_env(key, value);
}

#[allow(
    clippy::too_many_lines,
    reason = "one match arm per command, as the Go cobra tree"
)]
fn run(cli: Cli) -> Result<(), String> {
    if matches!(cli.command, Some(Cmd::Gui)) {
        return open_gui();
    }
    let ask = || passphrase(false);
    let mut app = App::from_env(&log).map_err(|e| e.to_string())?;
    app.passphrase = Some(&ask);
    let Some(cmd) = cli.command else {
        if !io::stdout().is_terminal() || !io::stdin().is_terminal() {
            use clap::CommandFactory;
            let _ = Cli::command().print_help();
            return Ok(());
        }
        return tui::run(&app).map_err(|e| e.to_string());
    };
    match cmd {
        Cmd::Tui => return tui::run(&app).map_err(|e| e.to_string()),
        Cmd::Gui => return open_gui(),
        Cmd::Init { mut remote, github } => {
            if !github.is_empty() {
                let url = sys::output(
                    None,
                    &[
                        "gh",
                        "repo",
                        "create",
                        &github,
                        "--private",
                        "--description",
                        "omaphoenix snapshots",
                    ],
                )
                .map_err(|e| e.to_string())?;
                remote = format!("{}.git", url.trim());
            }
            app.init(&remote).map_err(|e| e.to_string())?;
            say(&format!("initialised {}", app.repo.dir.display()));
            if !remote.is_empty() {
                say(&format!("remote      {remote}"));
            }
            say("next: omaphoenix snapshot");
        }
        Cmd::Snapshot => match app.snapshot() {
            Ok(None) => say("no changes since the last snapshot"),
            Ok(Some(s)) => say(&format!("✓ snapshot {}  {}", s.id, s.time)),
            Err(e) => {
                notify("omaphoenix snapshot failed", &e.to_string());
                return Err(e.to_string());
            }
        },
        Cmd::List => {
            for s in app.repo.list().map_err(|e| e.to_string())? {
                say(&format!("{}  {}", s.id, s.time));
            }
        }
        Cmd::Restore {
            items,
            at,
            dry_run,
            only,
            all_hardware,
            no_restart,
            from,
        } => {
            if !app.repo.exists() {
                if from.is_empty() {
                    return Err("no local snapshot repo; pass --from <git url>".into());
                }
                Repo::clone_from(&from, &app.repo.dir).map_err(|e| e.to_string())?;
            }
            let opt = RestoreOptions {
                at,
                dry_run,
                no_restart,
                all_hardware,
                only,
                items,
            };
            let (snap, reports) = app.restore(&opt).map_err(|e| e.to_string())?;
            print_report(&snap, &reports, dry_run)?;
        }
        Cmd::Undo { dry_run } => {
            let rep = app.undo(dry_run).map_err(|e| e.to_string())?;
            let verb = if dry_run { "undo (dry run)" } else { "undo" };
            say(&format!(
                "{verb} of the restore of snapshot {}",
                rep.snapshot
            ));
            rep.reverted.iter().for_each(|r| say(&format!("  ✓ {r}")));
            rep.kept.iter().for_each(|k| say(&format!("  · {k}")));
            rep.failed.iter().for_each(|f| say(&format!("  ✗ {f}")));
            for (mgr, pkgs) in &rep.packages {
                say(&format!(
                    "\n{} package(s) were installed by the restore ({mgr}). To remove them:",
                    pkgs.len()
                ));
                if mgr == "pkg-repo" || mgr == "pkg-aur" {
                    say(&format!("  sudo pacman -Rns {}", pkgs.join(" ")));
                } else {
                    say(&format!("  {}   ({mgr})", pkgs.join(" ")));
                }
            }
            if !dry_run && !rep.reverted.is_empty() {
                say(&format!(
                    "\nAnything the restore created is in {}",
                    rep.trash.display()
                ));
            }
            if !rep.failed.is_empty() {
                return Err(format!(
                    "{} change(s) could not be reversed; run undo again after fixing them",
                    rep.failed.len()
                ));
            }
        }
        Cmd::Doctor {
            json,
            notify: notify_mode,
        } => {
            if notify_mode {
                doctor_notify(&app);
                return Ok(());
            }
            let findings = app.doctor();
            if json {
                say(&serde_json::to_string(&findings).map_err(|e| e.to_string())?);
                return Ok(());
            }
            let mut risks = 0;
            for f in &findings {
                let mark = match f.level.as_str() {
                    "risk" => "✗",
                    "warn" => "!",
                    _ => "✓",
                };
                say(&format!("{mark} {:<9} {}", f.area, f.msg));
                risks += usize::from(f.level == "risk");
            }
            if risks > 0 {
                return Err(format!("{risks} thing(s) would be lost in a reinstall"));
            }
        }
        Cmd::Reaudit => {
            let (snap, r) = app.reaudit().map_err(|e| e.to_string())?;
            say(&format!(
                "re-audit against snapshot {snap}: score {} → {}",
                r.before, r.after
            ));
            r.new_fails
                .iter()
                .for_each(|f| say(&format!("  ✗ new since the snapshot: {f}")));
            r.resolved
                .iter()
                .for_each(|f| say(&format!("  ✓ resolved: {f}")));
            if r.new_fails.is_empty() {
                say("  ✓ the restore brought no new risks");
            } else {
                return Err(format!(
                    "{} new risk(s) since the snapshot",
                    r.new_fails.len()
                ));
            }
        }
        Cmd::Secrets {
            command: SecretsCmd::Init,
        } => {
            let _ = writeln!(
                io::stderr(),
                "This passphrase unlocks your secrets on restore. It is never stored;\nlose it and the encrypted secrets are gone (nothing else is)."
            );
            let p = passphrase(true)?;
            app.init_secrets(&p).map_err(|e| e.to_string())?;
            say("✓ secrets set up; ~/.ssh is now included (private keys encrypted)");
        }
        Cmd::Timer { command } => match command {
            TimerCmd::On { interval } => {
                schedule::on(&interval)?;
                say(&schedule::status());
            }
            TimerCmd::Off => schedule::off()?,
            TimerCmd::Status => say(&schedule::status()),
        },
    }
    Ok(())
}

/// The login hook: one quiet notification about backup health, never an
/// error exit that Omarchy would log.
fn doctor_notify(app: &App<'_>) {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    let alert = app.backup_alert(now);
    if alert.is_empty() || !sys::has("notify-send") {
        return;
    }
    let mut lines: Vec<String> = alert.iter().take(3).map(|f| f.msg.clone()).collect();
    if alert.len() > 3 {
        lines.push(format!(
            "…and {} more: run omaphoenix doctor",
            alert.len() - 3
        ));
    }
    let urgency = if alert.iter().any(|f| f.level == "risk") {
        "critical"
    } else {
        "normal"
    };
    let _ = sys::output(
        None,
        &[
            "notify-send",
            "-u",
            urgency,
            "-a",
            "omaphoenix",
            "Your backups need a look",
            &lines.join("\n"),
        ],
    );
}

fn print_report(
    snap: &phx_core::store::Snapshot,
    reports: &[StageReport],
    dry: bool,
) -> Result<(), String> {
    say(&format!(
        "\n{} of snapshot {} ({})",
        if dry { "dry run" } else { "restore" },
        snap.id,
        snap.time
    ));
    let (mut failed, mut deferred) = (0, 0);
    for r in reports {
        let mark = if !r.ok() {
            failed += 1;
            "✗"
        } else if r.result.deferred.is_empty() {
            "✓"
        } else {
            "⏸"
        };
        deferred += r.result.deferred.len();
        let mut line = format!(
            "{mark} {:<9} applied {} · already present {}",
            r.category, r.result.applied, r.result.present
        );
        if !r.result.planned.is_empty() {
            let _ = write!(line, " · planned {}", r.result.planned.len());
        }
        say(&line);
        for (i, p) in r.result.planned.iter().enumerate() {
            if i == 15 {
                say(&format!("    … {} more", r.result.planned.len() - 15));
                break;
            }
            say(&format!("    + {p}"));
        }
        r.result
            .failed
            .iter()
            .for_each(|f| say(&format!("    ✗ {f}")));
        r.result
            .deferred
            .iter()
            .for_each(|d| say(&format!("    ⏸ {d}")));
        r.skipped
            .iter()
            .for_each(|s| say(&format!("    · skipped, other hardware: {s}")));
        if let Some(e) = &r.err {
            say(&format!("    ✗ {e}"));
        }
    }
    if deferred > 0 {
        say(&format!(
            "\n⏸ {deferred} item(s) were skipped for now because their download was too slow.\n  Everything else is restored. Run `omaphoenix restore` again later to finish them;\n  it continues where this run stopped."
        ));
    }
    if failed > 0 {
        return Err(format!("{failed} stage(s) failed"));
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
}
