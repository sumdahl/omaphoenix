use std::process::Command;
use std::thread;

use slint::ComponentHandle;

use crate::state::toast;
use crate::ui::App;

pub(crate) fn sq(s: &str) -> String {
    format!("'{}'", s.replace('\'', r"'\''"))
}

/// Runs a shell script in a terminal window, then `done` on the UI thread.
pub(crate) fn in_terminal(app: &App, script: &str, done: impl FnOnce(&App) + Send + 'static) {
    let full = format!("{script}; echo; read -rp 'Press Enter to close ' _");
    let child = Command::new("xdg-terminal-exec")
        .args(["--title=Security Audit", "bash", "-c", &full])
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
        Err(_) => toast(
            app,
            format!("No terminal to run this in; run it yourself: {script}"),
        ),
    }
}
