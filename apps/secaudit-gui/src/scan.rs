use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Duration;
use std::{env, fs, thread};

use phx_audit::audit::Ctx;
use phx_audit::store::{Meta, Run, now};
use slint::{ComponentHandle, SharedString, Weak};

use crate::state::{local, refresh, state, toast};
use crate::terminal::{in_terminal, sq};
use crate::ui::App;

/// The engine: `secaudit` next to this binary, else on PATH.
pub(crate) fn engine() -> String {
    env::current_exe()
        .ok()
        .and_then(|e| e.parent().map(|d| d.join("secaudit")))
        .filter(|p| p.exists())
        .map_or_else(
            || "secaudit".to_owned(),
            |p| p.to_string_lossy().into_owned(),
        )
}

pub(crate) fn host() -> String {
    fs::read_to_string("/proc/sys/kernel/hostname")
        .unwrap_or_default()
        .trim()
        .to_owned()
}

pub(crate) fn stamp() -> String {
    local(now(), "%Y%m%d-%H%M%S")
}

pub(crate) fn scan(app: &App, mode: &str) {
    let mode = if mode == "again" {
        state()
            .borrow()
            .run()
            .map_or_else(|| "quick".to_owned(), |r| r.meta.mode.clone())
    } else {
        mode.to_owned()
    };
    if !app.get_busy().is_empty() {
        return;
    }
    let store = state().borrow().store.clone();
    let kernel = fs::read_to_string("/proc/sys/kernel/osrelease")
        .unwrap_or_default()
        .trim()
        .to_owned();
    if mode == "quick" {
        app.set_busy("Scanning…".into());
        app.set_progress(0.0);
        let weak = app.as_weak();
        thread::spawn(move || {
            let progress_to = weak.clone();
            let result = store.record_quick_scan_with(
                &Ctx::from_env(),
                &host(),
                &kernel,
                &stamp(),
                |done, total, section| {
                    let (w, label) = (
                        progress_to.clone(),
                        format!("Scanning: {section}  ({}/{total})", done + 1),
                    );
                    #[allow(clippy::cast_precision_loss, reason = "a dozen sections")]
                    let frac = done as f32 / total as f32;
                    let _ = slint::invoke_from_event_loop(move || {
                        if let Some(app) = w.upgrade() {
                            app.set_busy(label.into());
                            app.set_progress(frac);
                        }
                    });
                },
            );
            let _ = slint::invoke_from_event_loop(move || {
                let Some(app) = weak.upgrade() else { return };
                app.set_busy(SharedString::new());
                app.set_progress(-1.0);
                match result {
                    Ok(_) => scan_done(&app),
                    Err(e) => toast(&app, format!("Scan failed: {e}")),
                }
            });
        });
        return;
    }
    // Full and deep: sudo asks in a terminal, the engine writes the run.
    let mut meta = Meta {
        mode: mode.clone(),
        started: now(),
        host: host(),
        user: env::var("USER").unwrap_or_default(),
        kernel,
        finished: None,
    };
    let dir = match store.begin(&stamp(), &meta) {
        Ok(d) => d,
        Err(e) => return toast(app, format!("Couldn't start the scan: {e}")),
    };
    let deep = if mode == "deep" { " --deep" } else { "" };
    let script = format!(
        "echo; echo '  Security Audit · {mode} scan'; echo '  Reads root-only files; never changes anything. sudo is only used to look.'; echo; sudo {}{deep} --json {} --user {} 2>&1 | tee {}",
        sq(&engine()),
        sq(&dir.join("findings.jsonl").to_string_lossy()),
        sq(&env::var("USER").unwrap_or_default()),
        sq(&dir.join("output.txt").to_string_lossy()),
    );
    app.set_busy(format!("{mode} scan: waiting for sudo in the terminal…").into());
    app.set_progress(-1.0);
    follow_output(app.as_weak(), dir.join("output.txt"), mode.clone());
    in_terminal(app, &script, move |app| {
        app.set_busy(SharedString::new());
        app.set_progress(-1.0);
        if fs::metadata(dir.join("findings.jsonl")).map_or(0, |m| m.len()) == 0 {
            let _ = fs::remove_dir_all(&dir);
            return toast(app, "Scan cancelled or failed; nothing was saved");
        }
        let _ = store.finish(&dir, &mut meta);
        scan_done(app);
    });
}

/// Progress comes from the `== Section ==` headers the terminal scan tees to `output.txt`.
pub(crate) fn follow_output(weak: Weak<App>, output: PathBuf, mode: String) {
    let total = phx_audit::checks::SECTIONS.len();
    thread::spawn(move || {
        let mut seen = 0;
        loop {
            thread::sleep(Duration::from_millis(500));
            let text = fs::read_to_string(&output).unwrap_or_default();
            let sections: Vec<&str> = text
                .lines()
                .filter_map(|l| l.strip_prefix("== ").and_then(|l| l.strip_suffix(" ==")))
                .filter(|s| *s != "Summary")
                .collect();
            let finished = text.lines().any(|l| l == "== Summary ==");
            if sections.len() != seen || finished {
                seen = sections.len();
                let current = sections
                    .last()
                    .map_or_else(String::new, |s| (*s).to_owned());
                let (w, mode) = (weak.clone(), mode.clone());
                #[allow(clippy::cast_precision_loss, reason = "a dozen sections")]
                let frac = seen.saturating_sub(1) as f32 / total as f32;
                let label = format!("{mode} scan: {current}  ({seen}/{total})");
                let alive = slint::invoke_from_event_loop(move || {
                    if let Some(app) = w.upgrade()
                        && !app.get_busy().is_empty()
                    {
                        app.set_busy(label.into());
                        app.set_progress(frac);
                    }
                });
                if alive.is_err() || finished {
                    return;
                }
            }
            // Stop when the scan's folder is gone (cancelled) or the app went idle.
            if !output.parent().is_some_and(Path::exists) {
                return;
            }
        }
    });
}

pub(crate) fn scan_done(app: &App) {
    let st = state();
    st.borrow_mut().reload();
    st.borrow_mut().current = 0;
    app.set_tab(0);
    app.set_sel(0);
    refresh(app);
    // The counts are copied out so no borrow is held while toast() writes.
    let counts = st.borrow().run().map(Run::counts);
    if let Some(c) = counts {
        let body = format!(
            "Score {} ({}) · {} fail · {} warn",
            c.score(),
            c.grade(),
            c.fail,
            c.warn
        );
        let _ = Command::new("notify-send")
            .args([
                "-a",
                "secaudit",
                "-i",
                "security-high-symbolic",
                "Security audit finished",
                &body,
            ])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn();
        toast(app, format!("Scan finished: {body}"));
    }
}

pub(crate) fn wire(app: &App) {
    let weak = app.as_weak();
    app.on_scan(move |mode| {
        if let Some(app) = weak.upgrade() {
            scan(&app, &mode);
        }
    });
}
