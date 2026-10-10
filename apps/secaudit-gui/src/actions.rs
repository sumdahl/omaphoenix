use std::collections::BTreeMap;
use std::io::Write as _;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};
use std::{env, fs};

use secaudit_core::store::{Accepted, Run, now};
use slint::ComponentHandle;

use crate::state::{local, refresh, selected, state, toast};
use crate::terminal::{in_terminal, sq};
use crate::ui::App;

pub(crate) fn toggle_accept(app: &App) {
    let Some((_, item)) = selected(app) else {
        return;
    };
    if !item.is_issue() {
        return;
    }
    let st = state();
    let mut s = st.borrow_mut();
    let key = item.accept_key();
    let msg = if item.accepted {
        s.accepted.remove(&key);
        format!(
            "Moved back to Issues: {}. Press a again to undo.",
            item.title()
        )
    } else {
        s.accepted.insert(
            key,
            Accepted {
                section: item.finding.section.clone(),
                msg: item.title().to_owned(),
                at: now(),
            },
        );
        format!(
            "Accepted, no longer scored: {}. Press a again to undo.",
            item.title()
        )
    };
    if let Err(e) = s.store.save_accepted(&s.accepted) {
        drop(s);
        return toast(app, format!("Couldn't save: {e}"));
    }
    let acc = s.accepted.clone();
    for r in &mut s.runs {
        r.recount(&acc);
    }
    drop(s);
    refresh(app);
    toast(app, msg);
}

pub(crate) fn copy(app: &App) {
    let Some((_, item)) = selected(app) else {
        return;
    };
    let (text, what) = if item.finding.fix.is_empty() {
        (item.finding.hint, "Advice")
    } else {
        (item.finding.fix, "Fix command")
    };
    if text.is_empty() {
        return;
    }
    let ok = Command::new("wl-copy")
        .stdin(Stdio::piped())
        .spawn()
        .ok()
        .and_then(|mut c| {
            c.stdin.take()?.write_all(text.as_bytes()).ok()?;
            c.wait().ok()
        })
        .is_some_and(|s| s.success());
    toast(
        app,
        if ok {
            format!("{what} copied")
        } else {
            "wl-copy isn't available to copy with".into()
        },
    );
}

/// The terminal always asks before running the fix (AGENTS.md).
pub(crate) fn run_fix(app: &App) {
    let Some((run_dir, item)) = selected(app) else {
        return;
    };
    if item.finding.fix.is_empty() || item.accepted {
        return toast(
            app,
            "No automatic fix for this finding. See How to address.",
        );
    }
    let status = env::temp_dir().join(format!(
        "secaudit-gui-fix-{}-{}",
        std::process::id(),
        item.n
    ));
    let _ = fs::remove_file(&status);
    let script = format!(
        "echo; echo '  '{title}; echo; echo '  $ '{fix}; echo; read -rp '  Run this fix now? [y/N] ' a; if [[ $a == [yY]* ]]; then echo; bash -c {fix}; echo $? > {st}; fi",
        title = sq(item.title()),
        fix = sq(&item.finding.fix),
        st = sq(&status.to_string_lossy()),
    );
    let n = item.n;
    in_terminal(app, &script, move |app| {
        let code = fs::read_to_string(&status)
            .ok()
            .map(|s| s.trim().to_owned());
        let _ = fs::remove_file(&status);
        match code.as_deref() {
            Some("0") => {
                if let Ok(run) = Run::load(&run_dir, &BTreeMap::new()) {
                    let _ = run.mark_applied(n);
                }
                state().borrow_mut().reload();
                refresh(app);
                toast(app, "Fix applied. Rescan (R) to confirm the issue is gone.");
            }
            Some(c) => toast(app, format!("The fix exited with status {c}")),
            None => toast(app, "Fix not run"),
        }
    });
}

pub(crate) fn export(app: &App) {
    let st = state();
    let s = st.borrow();
    let Some(run) = s.run() else { return };
    match run.export_markdown(&local(run.meta.started, "%Y-%m-%d %H:%M")) {
        Ok(p) => {
            let home = env::var("HOME").unwrap_or_default();
            let shown = p.to_string_lossy().replacen(&home, "~", 1);
            drop(s);
            toast(app, format!("Report saved: {shown}"));
        }
        Err(e) => {
            drop(s);
            toast(app, format!("Couldn't save the report: {e}"));
        }
    }
}

/// Two clicks within five seconds delete (DESIGN.md: never discard without asking).
pub(crate) fn delete_run(app: &App, i: usize) {
    let st = state();
    let armed = st
        .borrow()
        .delete_armed
        .is_some_and(|(j, until)| j == i && Instant::now() < until);
    if !armed {
        st.borrow_mut().delete_armed = Some((i, Instant::now() + Duration::from_secs(5)));
        let label = st
            .borrow()
            .runs
            .get(i)
            .map(|r| local(r.meta.started, "%a %-d %b %H:%M"))
            .unwrap_or_default();
        return toast(
            app,
            format!("Press Delete again to remove the scan from {label}"),
        );
    }
    let path = st.borrow().runs.get(i).map(|r| r.path.clone());
    if let Some(p) = path {
        let _ = fs::remove_dir_all(p);
    }
    st.borrow_mut().delete_armed = None;
    st.borrow_mut().reload();
    refresh(app);
    toast(app, "Scan deleted");
}

pub(crate) fn wire(app: &App) {
    let weak = app.as_weak();
    let on = move |f: fn(&App)| {
        let weak = weak.clone();
        move || {
            if let Some(app) = weak.upgrade() {
                f(&app);
            }
        }
    };
    app.on_toggle_accept({
        let f = on(toggle_accept);
        move |_| f()
    });
    app.on_copy({
        let f = on(copy);
        move |_| f()
    });
    app.on_run_fix({
        let f = on(run_fix);
        move |_| f()
    });
    app.on_export(on(export));
    let weak = app.as_weak();
    app.on_delete_run(move |i| {
        if let Some(app) = weak.upgrade() {
            delete_run(&app, usize::try_from(i).unwrap_or(0));
        }
    });
}
