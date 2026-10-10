use std::io::Write as _;
use std::process::{Command, Stdio};
use std::{env, fs};

use secaudit::ai;
use secaudit_core::store::sev_name;
use slint::{ComponentHandle, SharedString};

use super::Then;
use super::session::{ask, setup_enable, show_ai, with_ai};
use crate::state::{selected, state, toast};
use crate::ui::App;

pub(crate) fn wire(app: &App) {
    let weak = app.as_weak();
    app.on_explain(move || {
        let Some(app) = weak.upgrade() else { return };
        if let Some((_, item)) = selected(&app).filter(|(_, i)| sev_name(i.finding.sev) != "pass") {
            with_ai(&app, Then::Explain(item.n));
        }
    });
    let weak = app.as_weak();
    app.on_plan(move || {
        if let Some(app) = weak.upgrade() {
            with_ai(&app, Then::Plan);
        }
    });
    let weak = app.as_weak();
    app.on_ai_send(move |q| {
        let Some(app) = weak.upgrade() else { return };
        let st = state();
        let mut s = st.borrow_mut();
        let dir = s.store.dir.clone();
        if let Some(sess) = &mut s.ai
            && !sess.busy
            && !q.trim().is_empty()
        {
            ask(sess, q.trim(), true, &dir);
        }
        drop(s);
        show_ai(&app);
    });
    let weak = app.as_weak();
    app.on_ai_regenerate(move || {
        let Some(app) = weak.upgrade() else { return };
        let st = state();
        let mut s = st.borrow_mut();
        let dir = s.store.dir.clone();
        if let Some(sess) = &mut s.ai
            && !sess.busy
        {
            let _ = fs::remove_file(ai::cache_path(&dir, &sess.prompt));
            sess.messages.clear();
            sess.session = None;
            sess.parts.clear();
            let p = sess.prompt.clone();
            ask(sess, &p, false, &dir);
        }
        drop(s);
        show_ai(&app);
    });
    let weak = app.as_weak();
    app.on_ai_copy(move || {
        let Some(app) = weak.upgrade() else { return };
        let text = state()
            .borrow()
            .ai
            .as_ref()
            .map(|a| a.last_answer.clone())
            .unwrap_or_default();
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
            &app,
            if ok {
                "Answer copied"
            } else {
                "wl-copy isn't available to copy with"
            },
        );
    });
    let weak = app.as_weak();
    app.on_ai_close(move || {
        let Some(app) = weak.upgrade() else { return };
        if let Some(mut sess) = state().borrow_mut().ai.take()
            && let Some(r) = &mut sess.request
        {
            r.cancel();
        }
        show_ai(&app);
    });
    wire_setup(app);
}

fn wire_setup(app: &App) {
    let weak = app.as_weak();
    app.on_setup_enable(move |key| {
        if let Some(app) = weak.upgrade() {
            setup_enable(&app, &key);
        }
    });
    let weak = app.as_weak();
    app.on_setup_switch(move |mode| {
        let Some(app) = weak.upgrade() else { return };
        let m: &'static str = if mode == "api" { "api" } else { "claude-code" };
        if let Some(setup) = &mut state().borrow_mut().setup {
            setup.1 = m;
        }
        app.set_setup_error(SharedString::new());
        show_ai(&app);
    });
    let weak = app.as_weak();
    app.on_setup_cancel(move || {
        let Some(app) = weak.upgrade() else { return };
        state().borrow_mut().setup = None;
        show_ai(&app);
    });
    app.set_ai_model(ai::model().into());
    let home = env::var("HOME").unwrap_or_default();
    app.set_setup_key_path(
        ai::key_file()
            .to_string_lossy()
            .replacen(&home, "~", 1)
            .into(),
    );
}
