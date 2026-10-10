//! The snapshot timer, the repo and secrets setup.

use phx_core::App as Engine;
use phx_core::schedule;
use slint::ComponentHandle;

use crate::activity;
use crate::run::{cli, in_terminal, sq};
use crate::state::ui_engine;
use crate::timeline::refresh_snapshots;
use crate::ui::App;

pub(crate) fn refresh_settings(app: &App, engine: &Engine<'_>) {
    let interval = schedule::interval();
    app.set_timer_on(interval.is_some());
    app.set_interval(interval.unwrap_or_default().into());
    app.set_timer_status(schedule::describe(interval).into());
    app.set_repo_dir(engine.repo.dir.to_string_lossy().as_ref().into());
    app.set_remote(engine.repo.remote().into());
    app.set_secrets_ready(phx_core::category::secret::secrets_ready(&engine.repo.dir));
}

pub(crate) fn wire(app: &App) {
    app.on_set_interval({
        let weak = app.as_weak();
        move |choice| {
            let Some(app) = weak.upgrade() else { return };
            let interval = (!choice.is_empty()).then_some(choice.as_str());
            match schedule::set(interval) {
                Ok(()) => app.set_toast(schedule::describe(interval).into()),
                Err(e) => app.set_toast(e.into()),
            }
            refresh_settings(&app, &ui_engine());
            activity::refresh(&app);
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
