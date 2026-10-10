//! The guided migration to a fresh install.

use phx_core::App as Engine;
use phx_core::migrate;
use phx_core::sys;
use slint::ComponentHandle;

use crate::pulse::refresh_pulse;
use crate::run::{background, cli, set_busy};
use crate::state::{Shared, model, ui_engine, ui_state};
use crate::timeline::take_snapshot;
use crate::ui::{App, StepRow};

pub(crate) fn refresh_migrate(app: &App, state: &Shared, engine: &Engine<'_>) {
    let s = state.borrow();
    let done = s.migration.done();
    let remote = engine.repo.remote();
    app.set_steps(model(
        migrate::STEPS
            .iter()
            .enumerate()
            .map(|(i, t)| StepRow {
                title: (*t).into(),
                done: done.get(i).copied().unwrap_or(false),
                explain: migrate::explain(i, &remote).into(),
            })
            .collect(),
    ));
}

pub(crate) fn wire(app: &App) {
    app.on_migrate_step({
        let (weak, state) = (app.as_weak(), ui_state());
        move |i| {
            let Some(app) = weak.upgrade() else { return };
            match i {
                0 => {
                    app.invoke_go(2);
                    app.invoke_scan();
                }
                4 => {
                    set_busy(&app, "Re-auditing against the restored snapshot…");
                    background(
                        &app,
                        || {
                            sys::command(&cli())
                                .env("OMAPHOENIX_NO_NOTIFY", "1")
                                .arg("reaudit")
                                .output()
                                .map(|o| {
                                    String::from_utf8_lossy(&if o.stdout.is_empty() {
                                        o.stderr
                                    } else {
                                        o.stdout
                                    })
                                    .lines()
                                    .take(4)
                                    .collect::<Vec<_>>()
                                    .join(" · ")
                                })
                        },
                        |app, r| {
                            set_busy(app, "");
                            app.set_toast(r.unwrap_or_else(|e| e.to_string()).into());
                        },
                    );
                }
                1 => {
                    let mut s = state.borrow_mut();
                    s.migration.cleaned = true;
                    s.migration.save(&ui_engine().state_dir());
                    drop(s);
                    app.invoke_go(2);
                }
                2 => {
                    take_snapshot(&app);
                    if let Some(id) = ui_engine()
                        .repo
                        .list()
                        .ok()
                        .and_then(|l| l.first().map(|s| s.id.clone()))
                    {
                        let mut s = state.borrow_mut();
                        s.migration.snapshot = id;
                        s.migration.save(&ui_engine().state_dir());
                    }
                }
                _ => {
                    let mut s = state.borrow_mut();
                    s.migration.restored_note_seen = true;
                    s.migration.save(&ui_engine().state_dir());
                }
            }
            refresh_migrate(&app, &state, &ui_engine());
            refresh_pulse(&app, &state);
        }
    });
}
