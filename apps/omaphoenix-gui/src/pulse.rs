//! The headline and the one primary action.

use phx_core::App as Engine;
use slint::ComponentHandle;

use crate::run::background;
use crate::state::{Shared, quiet, ui_state};
use crate::timeline::take_snapshot;
use crate::ui::App;

/// Derived from state, never claiming protection that isn't there (DESIGN.md §1–2).
pub(crate) fn refresh_pulse(app: &App, state: &Shared) {
    let s = state.borrow();
    let risks = app.get_doctor_risks();
    let (headline, sub) = if !app.get_set_up() {
        (
            "Nothing is protected yet.",
            "Set up snapshots, and a reinstall stops being a loss.",
        )
    } else if risks > 0 {
        (
            "Some of your setup would be lost in a reinstall.",
            "See what's at risk below, and take a snapshot.",
        )
    } else if risks == 0 {
        (
            "You're protected.",
            "A reinstall would lose nothing that matters.",
        )
    } else {
        ("Checking your protection…", "")
    };
    app.set_headline(headline.into());
    app.set_subline(sub.into());
    let fails = s.audit.as_ref().map_or(0, |r| r.counts().fail);
    let label = if !app.get_set_up() {
        "Set up snapshots".to_owned()
    } else if fails > 0 {
        format!(
            "Review {fails} audit risk{}",
            if fails == 1 { "" } else { "s" }
        )
    } else if s.migration.audited && s.migration.snapshot.is_empty() {
        "Continue migration".to_owned()
    } else {
        "Take snapshot".to_owned()
    };
    app.set_primary_label(label.into());
}

pub(crate) fn load_doctor(app: &App) {
    background(
        app,
        || {
            Engine::from_env(&quiet)
                .map(|e| e.doctor())
                .unwrap_or_default()
        },
        |app, findings| {
            let risks = findings.iter().filter(|f| f.level == "risk").count();
            app.set_doctor_risks(i32::try_from(risks).unwrap_or(0));
            refresh_pulse(app, &ui_state());
        },
    );
}

pub(crate) fn wire(app: &App) {
    app.on_primary({
        let weak = app.as_weak();
        move || {
            let Some(app) = weak.upgrade() else { return };
            let label = app.get_primary_label();
            if label.starts_with("Review") {
                app.invoke_go(2);
            } else if label.starts_with("Continue") {
                app.invoke_go(4);
            } else if label.starts_with("Set up") {
                app.invoke_set_up_repo();
            } else {
                take_snapshot(&app);
            }
        }
    });
}
