use std::fs;
use std::path::Path;
use std::sync::mpsc;

use secaudit::ai;
use serde_json::json;
use slint::SharedString;

use super::markdown::md_blocks;
use super::{AiSess, Then};
use crate::state::{model, state};
use crate::ui::{AiBlock, App};

pub(crate) fn show_ai(app: &App) {
    let st = state();
    let s = st.borrow();
    app.set_setup_open(s.setup.is_some());
    if let Some((_, mode)) = s.setup {
        app.set_setup_mode(mode.into());
        app.set_setup_asks_key(mode == "api" && !ai::env_credentials() && !ai::has_stored_key());
        app.set_setup_alt(ai::claude_bin().is_some());
    }
    let Some(sess) = &s.ai else {
        app.set_ai_open(false);
        return;
    };
    app.set_ai_open(true);
    app.set_ai_title(sess.title.as_str().into());
    app.set_ai_via(if sess.backend == "claude-code" {
        "Claude Code".into()
    } else {
        ai::model().into()
    });
    let mut blocks = Vec::new();
    for (q, text) in &sess.parts {
        if *q {
            blocks.push(AiBlock {
                kind: "q".into(),
                text: text.as_str().into(),
            });
        } else {
            blocks.extend(md_blocks(text));
        }
    }
    if sess.busy {
        blocks.extend(md_blocks(&sess.streaming));
    }
    app.set_ai_blocks(model(blocks));
    app.set_ai_status(sess.status.as_str().into());
    app.set_ai_error(sess.error);
    app.set_ai_busy(sess.busy);
}

pub(crate) fn ask(sess: &mut AiSess, question: &str, show: bool, state_dir: &Path) {
    sess.busy = true;
    sess.error = false;
    if show {
        sess.parts.push((true, question.to_owned()));
    }
    let redacted = ai::redact(question);
    sess.messages
        .push(json!({"role": "user", "content": redacted}));
    sess.streaming.clear();
    sess.status = "Claude is thinking…".into();
    let (tx, rx) = mpsc::channel();
    sess.rx = Some(rx);
    sess.request = Some(if sess.backend == "claude-code" {
        let mut prompt = redacted;
        if sess.session.is_none() && sess.messages.len() > 1 {
            let prev: Vec<String> = sess.messages[..sess.messages.len() - 1]
                .iter()
                .map(|m| {
                    let role = m["role"].as_str().unwrap_or_default();
                    format!(
                        "<{role}>\n{}\n</{role}>",
                        m["content"].as_str().unwrap_or_default()
                    )
                })
                .collect();
            prompt = format!(
                "Earlier in this conversation:\n{}\n\nFollow-up question: {prompt}",
                prev.join("\n\n")
            );
        }
        ai::ask_claude_code(
            &state_dir.join("claude-code"),
            &prompt,
            sess.session.as_deref(),
            tx,
        )
    } else {
        ai::ask_api(&sess.messages, tx)
    });
}

pub(crate) fn open_ai(app: &App, then: Then) {
    let st = state();
    let mut s = st.borrow_mut();
    let Some(run) = s.run() else { return };
    let (title, prompt) = match then {
        Then::Explain(n) => {
            let Some(f) = run.items.iter().find(|i| i.n == n) else {
                return;
            };
            (
                format!("{}   {}", f.title(), f.finding.section),
                ai::explain_prompt(f, run),
            )
        }
        Then::Plan => {
            let c = run.counts();
            let n = c.fail + c.warn;
            (
                format!(
                    "Action plan  {n} open issue{} · score {} {}",
                    if n == 1 { "" } else { "s" },
                    c.score(),
                    c.grade()
                ),
                ai::plan_prompt(run),
            )
        }
    };
    let dir = s.store.dir.clone();
    let mut sess = AiSess {
        prompt,
        title,
        backend: ai::backend(),
        messages: Vec::new(),
        session: None,
        request: None,
        rx: None,
        parts: Vec::new(),
        streaming: String::new(),
        last_answer: String::new(),
        status: String::new(),
        error: false,
        busy: false,
    };
    if let Ok(text) = fs::read_to_string(ai::cache_path(&dir, &sess.prompt)) {
        sess.messages = vec![
            json!({"role": "user", "content": sess.prompt}),
            json!({"role": "assistant", "content": text}),
        ];
        sess.last_answer.clone_from(&text);
        sess.parts.push((false, text));
        sess.status = "saved answer  ·  Ask again to regenerate".into();
    } else {
        let p = sess.prompt.clone();
        ask(&mut sess, &p, false, &dir);
    }
    s.ai = Some(sess);
    drop(s);
    show_ai(app);
}

pub(crate) fn with_ai(app: &App, then: Then) {
    if ai::ready() {
        open_ai(app, then);
    } else {
        let mode = if ai::claude_bin().is_some() {
            "claude-code"
        } else {
            "api"
        };
        state().borrow_mut().setup = Some((then, mode));
        app.set_setup_error(SharedString::new());
        show_ai(app);
    }
}

pub(crate) fn setup_enable(app: &App, key: &str) {
    let Some((then, mode)) = state().borrow().setup else {
        return;
    };
    if mode == "api" && !ai::env_credentials() && !ai::has_stored_key() {
        if !key.trim().starts_with("sk-ant-") {
            app.set_setup_error(
                "That doesn't look like an Anthropic API key (they start with sk-ant-).".into(),
            );
            return;
        }
        if let Err(e) = ai::store_key(key) {
            app.set_setup_error(e.to_string().into());
            return;
        }
    }
    if let Err(e) = ai::save_consent(mode) {
        app.set_setup_error(e.to_string().into());
        return;
    }
    state().borrow_mut().setup = None;
    open_ai(app, then);
}
