//! Snapshots: the list, the snapshot run, and the restore preview sheet.

mod runner;
mod sheet;

use phx_core::App as Engine;
use slint::{ComponentHandle, SharedString};

use crate::run::{ago, set_busy};
use crate::state::model;
use crate::ui::{App, Snap};
use runner::run_snapshot;

pub(crate) fn refresh_snapshots(app: &App, engine: &Engine<'_>) {
    let snaps = engine.repo.list().unwrap_or_default();
    app.set_set_up(engine.repo.exists());
    app.set_last_snapshot(
        snaps
            .first()
            .map_or_else(
                || "No snapshot yet".to_owned(),
                |s| format!("Last snapshot {} ({})", ago(&s.time), s.id),
            )
            .into(),
    );
    app.set_snaps(model(
        snaps
            .iter()
            .map(|s| Snap {
                id: s.id.as_str().into(),
                when: ago(&s.time).into(),
                note: s.subject.as_str().into(),
            })
            .collect(),
    ));
}

pub(crate) fn take_snapshot(app: &App) {
    if !app.get_busy().is_empty() {
        return;
    }
    if !app.get_set_up() {
        app.set_toast("Not set up yet: Settings → Set up snapshots".into());
        return;
    }
    app.set_toast(SharedString::new());
    app.set_progress(0.0);
    set_busy(app, "Starting the snapshot…");
    run_snapshot(app.as_weak());
}

pub(crate) fn wire(app: &App) {
    app.on_take_snapshot({
        let weak = app.as_weak();
        move || {
            let Some(app) = weak.upgrade() else { return };
            take_snapshot(&app);
        }
    });

    sheet::wire(app);
}
