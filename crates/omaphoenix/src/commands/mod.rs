use std::io::{self, IsTerminal};

use phx_core::App;

use crate::cli::{Cli, Cmd, SecretsCmd};
use crate::{out, prompt, tui};

mod doctor;
mod gui;
mod init;
mod reaudit;
mod restore;
mod secrets;
mod snapshot;
mod timer;
mod undo;

pub fn run(cli: Cli) -> Result<(), String> {
    if matches!(cli.command, Some(Cmd::Gui)) {
        return gui::open();
    }
    let ask = || prompt::passphrase(false);
    let mut app = App::from_env(&out::log).map_err(|e| e.to_string())?;
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
        Cmd::Tui => tui::run(&app).map_err(|e| e.to_string()),
        Cmd::Gui => gui::open(),
        Cmd::Init { remote, github } => init::run(&app, remote, &github),
        Cmd::Snapshot => snapshot::run(&app),
        Cmd::List => snapshot::list(&app),
        Cmd::Restore {
            items,
            at,
            dry_run,
            only,
            all_hardware,
            no_restart,
            from,
        } => restore::run(
            &app,
            items,
            at,
            dry_run,
            only,
            all_hardware,
            no_restart,
            &from,
        ),
        Cmd::Undo { dry_run } => undo::run(&app, dry_run),
        Cmd::Doctor { json, notify } => doctor::run(&app, json, notify),
        Cmd::Reaudit => reaudit::run(&app),
        Cmd::Secrets {
            command: SecretsCmd::Init,
        } => secrets::init(&app),
        Cmd::Timer { command } => timer::run(command),
    }
}
