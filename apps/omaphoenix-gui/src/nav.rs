//! Switching between the six sections.

use slint::{ComponentHandle, SharedString};

use crate::backup::load_choices;
use crate::state::ui_state;
use crate::ui::App;

pub(crate) fn wire(app: &App) {
    app.on_go({
        let (weak, state) = (app.as_weak(), ui_state());
        move |i| {
            let Some(app) = weak.upgrade() else { return };
            app.set_section(i);
            app.set_toast(SharedString::new());
            if i == 3 && state.borrow().choices.is_empty() {
                load_choices(&app);
            }
        }
    });
}
