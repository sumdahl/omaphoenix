//! Spring-driven motion (DESIGN.md §6).

use std::cell::RefCell;
use std::rc::Rc;
use std::time::{Duration, Instant};

use phx_core::sys;
use slint::{ComponentHandle, Timer, TimerMode};

use crate::spring::Spring;
use crate::ui::App;

pub(crate) const ROW: f32 = 44.0;
pub(crate) const SHEET_H: f32 = 460.0;

pub(crate) fn reduced_motion() -> bool {
    sys::output(None, &["hyprctl", "getoption", "animations:enabled", "-j"])
        .is_ok_and(|o| o.contains("\"int\": 0") || o.contains("\"int\":0"))
}

pub(crate) struct Motion {
    pub(crate) ring: Spring,
    pub(crate) drop: Spring,
    pub(crate) sheet: Spring,
    pub(crate) sheet_open: bool,
    pub(crate) reduced: bool,
    pub(crate) last: Option<Instant>,
    pub(crate) drag: Vec<(Instant, f32)>,
    pub(crate) drag_start: f32,
}

/// Runs the frame driver only until every spring settles: no idle wakeups.
pub(crate) fn kick(app: &App, motion: &Rc<RefCell<Motion>>, timer: &Rc<Timer>) {
    if timer.running() {
        return;
    }
    motion.borrow_mut().last = None;
    let (weak, m, t) = (app.as_weak(), motion.clone(), Rc::downgrade(timer));
    timer.start(TimerMode::Repeated, Duration::from_millis(4), move || {
        let Some(app) = weak.upgrade() else { return };
        let mut m = m.borrow_mut();
        let now = Instant::now();
        let dt = m
            .last
            .map_or(1.0 / 60.0, |l| (now - l).as_secs_f32().min(1.0 / 20.0));
        m.last = Some(now);
        m.ring.step(dt);
        m.drop.step(dt);
        m.sheet.step(dt);
        app.set_ring_value(m.ring.value);
        app.set_drop(m.drop.value);
        app.set_sheet_y(m.sheet.value);
        if !m.sheet_open && m.sheet.value >= SHEET_H - 0.5 {
            m.sheet.set(SHEET_H);
            app.set_sheet_y(SHEET_H);
            app.set_sheet_visible(false);
        }
        if !(m.ring.moving() || m.drop.moving() || m.sheet.moving())
            && let Some(t) = t.upgrade()
        {
            t.stop();
        }
    });
}
