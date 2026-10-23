//! Search boxes: Timeline, Audit and Backup filter their lists with `phx_core::fuzzy`.

use slint::ComponentHandle;

use crate::audit::refresh_audit;
use crate::backup::refresh_backup;
use crate::state::{ui_engine, ui_motion, ui_state, ui_timer};
use crate::timeline::refresh_snapshots;
use crate::ui::App;

pub(crate) fn wire(app: &App) {
    app.on_search({
        let weak = app.as_weak();
        move |section, text| {
            let Some(app) = weak.upgrade() else { return };
            let state = ui_state();
            let text = text.to_string();
            match section {
                1 => {
                    state.borrow_mut().query.timeline = text;
                    refresh_snapshots(&app, &ui_engine());
                }
                2 => {
                    state.borrow_mut().query.audit = text;
                    app.set_finding_sel(0);
                    refresh_audit(&app, &state, &ui_motion(), &ui_timer());
                }
                3 => {
                    state.borrow_mut().query.backup = text;
                    refresh_backup(&app, &state);
                }
                _ => {}
            }
        }
    });
}
