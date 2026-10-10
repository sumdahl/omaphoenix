//! Running work off the UI thread: the CLI, terminals and threads.

use std::{env, thread};

use phx_core::sys;
use slint::ComponentHandle;

use crate::ui::App;

pub(crate) fn cli() -> String {
    env::current_exe()
        .ok()
        .and_then(|e| e.parent().map(|d| d.join("omaphoenix")))
        .filter(|p| p.exists())
        .map_or_else(
            || "omaphoenix".to_owned(),
            |p| p.to_string_lossy().into_owned(),
        )
}

/// A terminal window, because sudo and passphrases need one.
pub(crate) fn in_terminal(app: &App, script: &str, done: impl FnOnce(&App) + Send + 'static) {
    if !sys::has("xdg-terminal-exec") {
        app.set_toast(format!("Run this in a terminal: {script}").into());
        return;
    }
    let full = format!("{script}; echo; read -rp 'Press Enter to close ' _");
    let child = sys::command("xdg-terminal-exec")
        .args(["bash", "-c", &full])
        .spawn();
    let weak = app.as_weak();
    match child {
        Ok(mut c) => {
            thread::spawn(move || {
                let _ = c.wait();
                let _ = slint::invoke_from_event_loop(move || {
                    if let Some(app) = weak.upgrade() {
                        done(&app);
                    }
                });
            });
        }
        Err(e) => app.set_toast(format!("Couldn't open a terminal: {e}").into()),
    }
}

pub(crate) fn sq(s: &str) -> String {
    format!("'{}'", s.replace('\'', r"'\''"))
}

pub(crate) fn background<T: Send + 'static>(
    app: &App,
    work: impl FnOnce() -> T + Send + 'static,
    then: impl FnOnce(&App, T) + Send + 'static,
) {
    let weak = app.as_weak();
    thread::spawn(move || {
        let out = work();
        let _ = slint::invoke_from_event_loop(move || {
            if let Some(app) = weak.upgrade() {
                then(&app, out);
            }
        });
    });
}

pub(crate) fn ago(iso: &str) -> String {
    let then = sys::output(None, &["date", "-d", iso, "+%s"])
        .ok()
        .and_then(|s| s.parse::<u64>().ok());
    let now = secaudit_core::store::now();
    match then.map(|t| now.saturating_sub(t)) {
        None => iso.to_owned(),
        Some(0..60) => "just now".into(),
        Some(s @ 60..3600) => format!("{} min ago", s / 60),
        Some(s @ 3600..172_800) => format!("{} h ago", s / 3600),
        Some(s) => format!("{} days ago", s / 86_400),
    }
}

pub(crate) fn set_busy(app: &App, what: &str) {
    app.set_busy(what.into());
}
