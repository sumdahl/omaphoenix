//! Running `omaphoenix snapshot` and streaming its steps into the progress strip.

use std::thread;
use std::time::Duration;

use phx_core::sys;
use slint::{ComponentHandle, Weak};

use super::refresh_snapshots;
use crate::motion::{ROW, kick};
use crate::run::{cli, set_busy};
use crate::spring::GLIDE;
use crate::state::{ui_engine, ui_motion, ui_timer};
use crate::ui::App;

/// In the order `omaphoenix snapshot` reports them.
const SNAP_STEPS: &[(&str, &str)] = &[
    ("audit", "Auditing this machine first (quick, read-only)…"),
    ("pkg-repo", "Listing packages…"),
    ("pkg-aur", "Listing AUR packages…"),
    ("plugin", "Saving Omarchy plugins…"),
    ("theme", "Saving themes…"),
    ("config", "Saving config files…"),
    ("local", "Saving local data…"),
    ("bin", "Saving your scripts…"),
    ("pkg-other", "Listing other package managers…"),
    ("secret", "Encrypting secrets…"),
    ("etc", "Saving /etc files…"),
    ("service", "Listing services…"),
];

fn snapshot_step(line: &str) -> Option<(&'static str, f32)> {
    #[allow(clippy::cast_precision_loss, reason = "a dozen steps")]
    let total = (SNAP_STEPS.len() + 1) as f32;
    let t = line.trim_start();
    if t.contains("auditing this machine first") {
        return SNAP_STEPS.first().map(|s| (s.1, 0.0));
    }
    if t.contains("saving the snapshot") {
        return Some(("Saving and pushing to GitHub…", (total - 0.5) / total));
    }
    let mut words = t.split_whitespace();
    let (name, second) = (words.next()?, words.next()?);
    let done = SNAP_STEPS.iter().position(|(n, _)| *n == name)?;
    // "audit score …" or "<category> N captured (…)" ends that step.
    if !(name == "audit" && second == "score" || t.contains(" captured (")) {
        return None;
    }
    let next = SNAP_STEPS
        .get(done + 1)
        .map_or("Saving and pushing to GitHub…", |s| s.1);
    #[allow(clippy::cast_precision_loss, reason = "a dozen steps")]
    Some((next, (done + 1) as f32 / total))
}

pub(super) fn run_snapshot(weak: Weak<App>) {
    use std::io::{BufRead, BufReader, Read};
    thread::spawn(move || {
        let spawned = sys::command(&cli())
            // The app shows the result; no desktop notification on top.
            .env("OMAPHOENIX_NO_NOTIFY", "1")
            .arg("snapshot")
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn();
        let mut child = match spawned {
            Ok(c) => c,
            Err(e) => {
                let msg = format!("Snapshot failed: {e}");
                let _ = slint::invoke_from_event_loop(move || {
                    if let Some(app) = weak.upgrade() {
                        set_busy(&app, "");
                        app.set_toast(msg.into());
                    }
                });
                return;
            }
        };
        let mut last = String::new();
        if let Some(out) = child.stdout.take() {
            for line in BufReader::new(out).lines().map_while(Result::ok) {
                if let Some((label, frac)) = snapshot_step(&line) {
                    let w = weak.clone();
                    let _ = slint::invoke_from_event_loop(move || {
                        if let Some(app) = w.upgrade() {
                            set_busy(&app, label);
                            app.set_progress(frac);
                        }
                    });
                }
                if !line.trim().is_empty() {
                    last = line;
                }
            }
        }
        let mut err = String::new();
        if let Some(mut e) = child.stderr.take() {
            let _ = e.read_to_string(&mut err);
        }
        let ok = child.wait().is_ok_and(|s| s.success());
        let locked = err.contains("already running");
        let _ = slint::invoke_from_event_loop(move || {
            let Some(app) = weak.upgrade() else { return };
            if locked {
                // Usually the automatic run holds the lock: show its result, not an error.
                set_busy(
                    &app,
                    "An automatic snapshot is running; showing it when it finishes…",
                );
                app.set_progress(-1.0);
                wait_for_other_run(app.as_weak());
                return;
            }
            snapshot_finished(
                &app,
                if ok {
                    Ok(last.trim().to_owned())
                } else {
                    Err(err.trim().lines().last().unwrap_or("it stopped").to_owned())
                },
            );
        });
    });
}

fn wait_for_other_run(weak: Weak<App>) {
    thread::spawn(move || {
        loop {
            thread::sleep(Duration::from_secs(2));
            if ui_engine().repo.lock().is_ok() {
                break;
            }
        }
        let _ = slint::invoke_from_event_loop(move || {
            if let Some(app) = weak.upgrade() {
                snapshot_finished(&app, Ok("The automatic snapshot finished".into()));
            }
        });
    });
}

fn snapshot_finished(app: &App, r: Result<String, String>) {
    use slint::Model as _;
    let (motion, timer) = (ui_motion(), ui_timer());
    set_busy(app, "");
    app.set_progress(-1.0);
    let before = app.get_snaps().row_count();
    refresh_snapshots(app, &ui_engine());
    match r {
        Ok(line) => app.set_toast(line.into()),
        Err(e) => app.set_toast(format!("Snapshot failed: {e}").into()),
    }
    // FLIP: the offset must change in the same frame as the insert.
    if app.get_snaps().row_count() > before {
        let mut m = motion.borrow_mut();
        m.drop.set(-ROW);
        app.set_drop(-ROW);
        let reduced = m.reduced;
        m.drop.to(0.0, GLIDE, reduced);
        drop(m);
        kick(app, &motion, &timer);
    }
}

#[cfg(test)]
mod tests {
    /// Real `omaphoenix snapshot` output lines.
    #[test]
    fn snapshot_output_moves_the_progress_strip_forward() {
        let lines = [
            "▸ auditing this machine first (quick, read-only)",
            "  audit     score 90 (A): 0 item(s) left out, 31 flagged",
            "  pkg-repo  221 captured (221 found)",
            "  pkg-aur   14 captured (14 found)",
            "  plugin io.github.sumdahl.geet: bundled 4 unpushed",
            "  plugin    41 captured (41 found)",
            "  config .config/Code skipped: browser/app profile (caches and sessions, not config)",
            "  ! skip /home/x/.config/ngrok/ngrok.yml: contains something shaped like a secret",
            "  service   44 captured (44 found)",
            "▸ saving the snapshot",
            "✓ snapshot d61abc7  2026-10-10T12:00:07+05:45",
        ];
        let steps: Vec<(&str, f32)> = lines
            .iter()
            .filter_map(|l| super::snapshot_step(l))
            .collect();
        let labels: Vec<&str> = steps.iter().map(|s| s.0).collect();
        assert_eq!(
            labels,
            [
                "Auditing this machine first (quick, read-only)…",
                "Listing packages…",
                "Listing AUR packages…",
                "Saving Omarchy plugins…",
                "Saving themes…",
                "Saving and pushing to GitHub…",
                "Saving and pushing to GitHub…",
            ],
            "detail lines (bundled, skipped, warnings) don't move it"
        );
        assert!(
            steps.windows(2).all(|w| w[0].1 <= w[1].1),
            "progress only goes forward: {steps:?}"
        );
        assert!(
            steps.last().is_some_and(|s| s.1 > 0.9 && s.1 < 1.0),
            "nearly done while pushing"
        );
    }
}
