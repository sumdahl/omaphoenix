//! The security audit: score, findings, accepting them and running fixes.

use std::cell::RefCell;
use std::fs;
use std::rc::Rc;

use phx_audit::store::{Accepted, Store};
use slint::{ComponentHandle, SharedString, Timer};

use crate::migrate::refresh_migrate;
use crate::motion::{Motion, kick};
use crate::pulse::refresh_pulse;
use crate::run::{background, in_terminal, set_busy, sq};
use crate::spring::GLIDE;
use crate::state::{Shared, model, ui_engine, ui_motion, ui_state, ui_timer};
use crate::ui::{App, AuditRow};

/// Issues on screen: matching the Audit search, accepted last, worst first.
fn shown_issues<'a>(
    run: &'a phx_audit::store::Run,
    query: &str,
) -> Vec<&'a phx_audit::store::Item> {
    let issues: Vec<&phx_audit::store::Item> = run.items.iter().filter(|i| i.is_issue()).collect();
    let mut items: Vec<&phx_audit::store::Item> = phx_core::fuzzy::filter(query, &issues, |i| {
        format!(
            "{} {} {}",
            i.finding.msg, i.finding.section, i.finding.detail
        )
    })
    .into_iter()
    .filter_map(|i| issues.get(i).copied())
    .collect();
    items.sort_by_key(|i| (i.accepted, phx_audit::store::sev_order(i.finding.sev), i.n));
    items
}

pub(crate) fn refresh_audit(
    app: &App,
    state: &Shared,
    motion: &Rc<RefCell<Motion>>,
    timer: &Rc<Timer>,
) {
    let s = state.borrow();
    let Some(run) = &s.audit else {
        app.set_audited(false);
        return;
    };
    let c = run.counts();
    app.set_audited(true);
    app.set_score(i32::try_from(c.score()).unwrap_or(0));
    app.set_fails(i32::try_from(c.fail).unwrap_or(0));
    app.set_warns(i32::try_from(c.warn).unwrap_or(0));
    app.set_accepted(i32::try_from(c.accepted).unwrap_or(0));
    let items = shown_issues(run, &s.query.audit);
    app.set_findings(model(
        items
            .iter()
            .map(|i| AuditRow {
                sev: phx_audit::store::sev_name(i.finding.sev).into(),
                title: i.title().into(),
                section: i.finding.section.as_str().into(),
                detail: i.finding.detail.as_str().into(),
                fix: i.finding.fix.as_str().into(),
                hint: i.finding.hint.as_str().into(),
                accepted: i.accepted,
            })
            .collect(),
    ));
    #[allow(clippy::cast_precision_loss, reason = "a score of 0-100")]
    let target = c.score() as f32;
    let mut m = motion.borrow_mut();
    let reduced = m.reduced;
    m.ring.to(target, GLIDE, reduced);
    drop(m);
    kick(app, motion, timer);
    drop(s);
    refresh_pulse(app, state);
}

pub(crate) fn wire(app: &App) {
    app.on_scan({
        let weak = app.as_weak();
        move || {
            let Some(app) = weak.upgrade() else { return };
            if !app.get_busy().is_empty() {
                return;
            }
            app.set_toast(SharedString::new());
            app.set_progress(0.0);
            set_busy(&app, "Auditing (quick, no sudo)…");
            let progress_to = app.as_weak();
            background(
                &app,
                move || {
                    let ctx = phx_audit::audit::Ctx::from_env();
                    let host = fs::read_to_string("/proc/sys/kernel/hostname").unwrap_or_default();
                    let kernel =
                        fs::read_to_string("/proc/sys/kernel/osrelease").unwrap_or_default();
                    let store = Store::from_env();
                    let dir = store.record_quick_scan_with(
                        &ctx,
                        host.trim(),
                        kernel.trim(),
                        &phx_core::category::stamp(),
                        |done, total, section| {
                            let (w, label) = (
                                progress_to.clone(),
                                format!("Auditing: {section}  ({}/{total})", done + 1),
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
                    dir.map(|d| store.runs().into_iter().find(|r| r.path == d))
                        .map_err(|e| e.to_string())
                },
                |app, r| {
                    let (state, motion, timer) = (ui_state(), ui_motion(), ui_timer());
                    set_busy(app, "");
                    app.set_progress(-1.0);
                    match r {
                        Ok(run) => {
                            {
                                let mut s = state.borrow_mut();
                                s.audit = run;
                                if !s.migration.audited {
                                    s.migration.audited = true;
                                    s.migration.save(&ui_engine().state_dir());
                                }
                            }
                            app.set_finding_sel(0);
                            refresh_audit(app, &state, &motion, &timer);
                            refresh_migrate(app, &state, &ui_engine());
                            app.set_toast(format!("Audit done: score {}", app.get_score()).into());
                        }
                        Err(e) => app.set_toast(format!("Audit failed: {e}").into()),
                    }
                },
            );
        }
    });

    wire_findings(app);
}

fn wire_findings(app: &App) {
    app.on_toggle_accept({
        let (weak, state, motion, timer) = (app.as_weak(), ui_state(), ui_motion(), ui_timer());
        move |i| {
            let Some(app) = weak.upgrade() else { return };
            let Ok(i) = usize::try_from(i) else { return };
            {
                let mut s = state.borrow_mut();
                let Some(run) = &s.audit else { return };
                let items = shown_issues(run, &s.query.audit);
                let Some(item) = items.get(i) else { return };
                let (key, was, title, section) = (
                    item.accept_key(),
                    item.accepted,
                    item.title().to_owned(),
                    item.finding.section.clone(),
                );
                if was {
                    s.accepted.remove(&key);
                } else {
                    s.accepted.insert(
                        key,
                        Accepted {
                            section,
                            msg: title,
                            at: phx_audit::store::now(),
                        },
                    );
                }
                let accepted = s.accepted.clone();
                if let Err(e) = Store::from_env().save_accepted(&accepted) {
                    app.set_toast(format!("Couldn't save: {e}").into());
                    return;
                }
                if let Some(run) = &mut s.audit {
                    run.recount(&accepted);
                }
            }
            refresh_audit(&app, &state, &motion, &timer);
        }
    });

    app.on_run_fix({
        let weak = app.as_weak();
        move |i| {
            let Some(app) = weak.upgrade() else { return };
            let fix = usize::try_from(i)
                .ok()
                .and_then(|i| slint::Model::row_data(&app.get_findings(), i))
                .map(|f| f.fix.to_string());
            if let Some(fix) = fix.filter(|f| !f.is_empty()) {
                // Fixes always confirm (AGENTS.md): it runs only on y.
                let script = format!(
                    "echo; echo '  $ '{q}; echo; read -rp '  Run this fix now? [y/N] ' a; [[ $a == [yY]* ]] && bash -c {q}",
                    q = sq(&fix)
                );
                in_terminal(&app, &script, |app| {
                    app.set_toast("Scan again to confirm the fix worked".into());
                });
            }
        }
    });
}
