//! The snapshot timer, the repo and secrets setup.

use phx_core::App as Engine;
use phx_core::schedule;
use slint::ComponentHandle;

use crate::run::{cli, in_terminal, sq};
use crate::state::ui_engine;
use crate::timeline::refresh_snapshots;
use crate::ui::App;

pub(crate) fn refresh_settings(app: &App, engine: &Engine<'_>) {
    let status = schedule::status();
    app.set_timer_on(!status.starts_with("timer off"));
    app.set_timer_status(status.into());
    app.set_repo_dir(engine.repo.dir.to_string_lossy().as_ref().into());
    app.set_remote(engine.repo.remote().into());
    app.set_secrets_ready(phx_core::category::secret::secrets_ready(&engine.repo.dir));
}

pub(crate) fn wire(app: &App) {
    app.on_toggle_timer({
        let weak = app.as_weak();
        move || {
            let Some(app) = weak.upgrade() else { return };
            let r = if app.get_timer_on() {
                schedule::off()
            } else {
                schedule::on("30m")
            };
            if let Err(e) = r {
                app.set_toast(e.into());
            }
            refresh_settings(&app, &ui_engine());
        }
    });

    app.on_setup_secrets({
        let weak = app.as_weak();
        move || {
            let Some(app) = weak.upgrade() else { return };
            in_terminal(&app, &format!("{} secrets init", sq(&cli())), |app| {
                refresh_settings(app, &ui_engine());
            });
        }
    });

    app.on_set_up_repo({
        let weak = app.as_weak();
        move || {
            let Some(app) = weak.upgrade() else { return };
            in_terminal(&app, &format!("{} init", sq(&cli())), |app| {
                refresh_snapshots(app, &ui_engine());
                refresh_settings(app, &ui_engine());
            });
        }
    });
}
