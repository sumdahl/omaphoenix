//! Snapshots: the list, the snapshot run, and the restore preview sheet.

mod runner;
mod sheet;

use phx_core::App as Engine;
use slint::{ComponentHandle, SharedString};

use crate::run::{ago, set_busy};
use crate::state::{model, ui_engine, ui_state};
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

/// Snapshots once the user has decided what to back up; asks about anything new first.
pub(crate) fn take_snapshot(app: &App) {
    if !app.get_busy().is_empty() {
        return;
    }
    if !app.get_set_up() {
        app.set_toast("Not set up yet: Settings → Set up snapshots".into());
        return;
    }
    if !ui_engine().has_chosen() {
        app.invoke_go(3);
        app.set_toast("First choose what to back up: untick what you don't want, then Save".into());
        return;
    }
    let (detecting, new) = {
        let state = ui_state();
        let s = state.borrow();
        (s.jobs.contains("choices"), s.unreviewed.len())
    };
    if detecting {
        app.set_toast("Checking this machine for new items first; try again in a moment".into());
        return;
    }
    if new > 0 {
        app.set_ask_new(i32::try_from(new).unwrap_or(i32::MAX));
        return;
    }
    start_snapshot(app);
}

fn start_snapshot(app: &App) {
    if ui_state().borrow().timer_running {
        app.set_toast(
            "The automatic snapshot is running right now; it will finish on its own".into(),
        );
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

    app.on_snapshot_anyway({
        let weak = app.as_weak();
        move || {
            let Some(app) = weak.upgrade() else { return };
            app.set_ask_new(0);
            start_snapshot(&app);
        }
    });
    app.on_review_new({
        let weak = app.as_weak();
        move || {
            let Some(app) = weak.upgrade() else { return };
            app.set_ask_new(0);
            app.invoke_go(3);
            app.set_toast("New items are marked ● new; tick or untick them, then Save".into());
        }
    });
    app.on_cancel_ask({
        let weak = app.as_weak();
        move || {
            if let Some(app) = weak.upgrade() {
                app.set_ask_new(0);
            }
        }
    });

    sheet::wire(app);
}
