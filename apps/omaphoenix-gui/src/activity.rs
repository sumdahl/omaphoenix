//! The status line's words when nothing is running, and watching for the automatic snapshot.

use std::time::Duration;

use phx_core::schedule;
use slint::{ComponentHandle, Timer, TimerMode};

use crate::state::{ui_engine, ui_state};
use crate::ui::App;
use crate::{pulse, timeline};

/// Says what is happening, or that nothing is and what needs the user.
pub(crate) fn refresh(app: &App) {
    let engine = ui_engine();
    let state = ui_state();
    let s = state.borrow();
    let chosen = engine.has_chosen();
    let text = if s.timer_running {
        "The automatic snapshot is running in the background…".to_owned()
    } else if s.jobs.contains("choices") {
        "Looking at what's on this machine…".to_owned()
    } else if s.jobs.contains("doctor") {
        "Checking what a reinstall would lose…".to_owned()
    } else if !engine.repo.exists() {
        "Not set up: nothing is backed up yet.".to_owned()
    } else {
        let mut parts = vec!["✓ Idle".to_owned()];
        if !chosen {
            parts.push("● choose what to back up (Backup)".to_owned());
        } else if !s.unreviewed.is_empty() {
            parts.push(format!(
                "● {} new item(s) to review (Backup)",
                s.unreviewed.len()
            ));
        }
        parts.push(app.get_last_snapshot().to_string());
        let interval = app.get_interval();
        parts.push(if interval.is_empty() {
            "manual snapshots".to_owned()
        } else {
            format!(
                "automatic, {}",
                phx_core::schedule::label(Some(interval.as_str()))
            )
        });
        parts.join(" · ")
    };
    app.set_activity(text.into());
    app.set_needs_choice(engine.repo.exists() && !chosen);
    app.set_new_count(i32::try_from(s.unreviewed.len()).unwrap_or(i32::MAX));
}

/// Polls the timer's unit; keep the returned `Timer` alive.
pub(crate) fn watch(app: &App) -> Timer {
    let timer = Timer::default();
    let weak = app.as_weak();
    timer.start(TimerMode::Repeated, Duration::from_secs(2), move || {
        let Some(app) = weak.upgrade() else { return };
        let running = schedule::running();
        let was = std::mem::replace(&mut ui_state().borrow_mut().timer_running, running);
        if was == running {
            return;
        }
        if !running {
            timeline::refresh_snapshots(&app, &ui_engine());
            pulse::load_doctor(&app);
        }
        refresh(&app);
    });
    timer
}
