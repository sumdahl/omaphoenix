//! The restore preview sheet: open, drag, release, close and apply.

use std::time::Instant;

use slint::ComponentHandle;

use super::refresh_snapshots;
use crate::motion::{SHEET_H, kick};
use crate::run::{background, cli, in_terminal, sq};
use crate::spring::SHEET;
use crate::state::{model, ui_engine, ui_motion, ui_state, ui_timer};
use crate::ui::{App, PreviewRow};

fn close_preview(app: &App) {
    let (motion, timer) = (ui_motion(), ui_timer());
    {
        let mut m = motion.borrow_mut();
        m.sheet_open = false;
        let reduced = m.reduced;
        m.sheet.to(SHEET_H, SHEET, reduced);
    }
    kick(app, &motion, &timer);
}

pub(super) fn wire(app: &App) {
    app.on_close_preview({
        let weak = app.as_weak();
        move || {
            let Some(app) = weak.upgrade() else { return };
            close_preview(&app);
        }
    });

    app.on_apply_restore({
        let (weak, state) = (app.as_weak(), ui_state());
        move || {
            let Some(app) = weak.upgrade() else { return };
            let id = state.borrow().preview.clone();
            close_preview(&app);
            in_terminal(
                &app,
                &format!("{} restore --at {}", sq(&cli()), sq(&id)),
                |app| {
                    refresh_snapshots(app, &ui_engine());
                    app.set_toast(
                        "Restore finished in the terminal; replaced files are kept as .pre-restore"
                            .into(),
                    );
                },
            );
        }
    });

    wire_sheet(app);
}

fn wire_sheet(app: &App) {
    let (state, motion, timer) = (ui_state(), ui_motion(), ui_timer());

    app.on_open_preview({
        let (weak, motion, timer, state) =
            (app.as_weak(), motion.clone(), timer.clone(), state.clone());
        move |id| {
            let Some(app) = weak.upgrade() else { return };
            state.borrow_mut().preview = id.to_string();
            app.set_preview_id(id.clone());
            app.set_rows_loaded(false);
            app.set_rows(model(Vec::new()));
            app.set_sheet_visible(true);
            {
                let mut m = motion.borrow_mut();
                m.sheet_open = true;
                let reduced = m.reduced;
                m.sheet.to(0.0, SHEET, reduced);
            }
            kick(&app, &motion, &timer);
            let at = id.to_string();
            background(
                &app,
                move || {
                    let opt = phx_core::RestoreOptions {
                        at,
                        dry_run: true,
                        ..Default::default()
                    };
                    ui_engine()
                        .restore(&opt)
                        .map(|(_, reports)| reports)
                        .map_err(|e| e.to_string())
                },
                |app, r| {
                    app.set_rows_loaded(true);
                    match r {
                        Ok(reports) => app.set_rows(model(
                            reports
                                .iter()
                                .filter(|r| !r.result.planned.is_empty())
                                .map(|r| PreviewRow {
                                    category: r.category.into(),
                                    count: i32::try_from(r.result.planned.len()).unwrap_or(0),
                                    detail: r
                                        .result
                                        .planned
                                        .first()
                                        .map(String::as_str)
                                        .unwrap_or_default()
                                        .into(),
                                })
                                .collect(),
                        )),
                        Err(e) => app.set_toast(e.into()),
                    }
                },
            );
        }
    });

    app.on_drag_sheet({
        let (weak, motion) = (app.as_weak(), motion.clone());
        move |dy| {
            let Some(app) = weak.upgrade() else { return };
            let mut m = motion.borrow_mut();
            if m.drag.is_empty() {
                m.drag_start = m.sheet.value;
            }
            let y = (m.drag_start + dy).max(0.0);
            m.sheet.set(y);
            m.drag.push((Instant::now(), y));
            app.set_sheet_y(y);
        }
    });

    app.on_release_sheet({
        let weak = app.as_weak();
        move || {
            let Some(app) = weak.upgrade() else { return };
            let mut m = motion.borrow_mut();
            let v = match (m.drag.first(), m.drag.last()) {
                (Some(a), Some(b)) if b.0 > a.0 => (b.1 - a.1) / (b.0 - a.0).as_secs_f32(),
                _ => 0.0,
            };
            m.drag.clear();
            let closing = m.sheet.value > SHEET_H * 0.35 || v > 900.0;
            if closing {
                drop(m);
                close_preview(&app);
                return;
            }
            let reduced = m.reduced;
            m.sheet.fling(v, 0.0, SHEET, reduced);
            drop(m);
            kick(&app, &motion, &timer);
        }
    });
}
