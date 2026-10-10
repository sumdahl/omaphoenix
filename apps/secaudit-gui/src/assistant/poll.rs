use std::fs;
use std::time::Duration;

use secaudit::ai::{self, Event};
use serde_json::json;
use slint::{ComponentHandle, Timer, TimerMode};

use super::session::show_ai;
use crate::state::state;
use crate::ui::App;

pub(crate) fn poll_ai(app: &App) {
    let st = state();
    let mut s = st.borrow_mut();
    let dir = s.store.dir.clone();
    let Some(sess) = &mut s.ai else { return };
    let Some(rx) = &sess.rx else { return };
    let events: Vec<Event> = rx.try_iter().collect();
    if events.is_empty() {
        return;
    }
    for e in events {
        match e {
            Event::Started => sess.status = "Writing…".into(),
            Event::Text(t) => sess.streaming.push_str(&t),
            Event::Done {
                note,
                warning,
                turn,
                session,
            } => {
                let text = std::mem::take(&mut sess.streaming);
                sess.messages
                    .push(turn.unwrap_or_else(|| json!({"role": "assistant", "content": text})));
                if session.is_some() {
                    sess.session = session;
                }
                if sess.messages.len() == 2 && !text.is_empty() {
                    let path = ai::cache_path(&dir, &sess.prompt);
                    if let Some(d) = path.parent() {
                        let _ = fs::create_dir_all(d);
                    }
                    let _ = fs::write(path, &text);
                }
                sess.last_answer.clone_from(&text);
                sess.parts.push((false, text));
                sess.status = format!("{note}{}", warning.unwrap_or_default());
                sess.busy = false;
                sess.rx = None;
                sess.request = None;
            }
            Event::Error { message, retry } => {
                if sess.messages.last().is_some_and(|m| m["role"] == "user") {
                    sess.messages.pop();
                }
                let partial = std::mem::take(&mut sess.streaming);
                if !partial.is_empty() {
                    sess.parts.push((false, partial));
                }
                sess.status = format!(
                    "✗ {message}{}",
                    if retry {
                        "  ·  Ask again to retry"
                    } else {
                        ""
                    }
                );
                sess.error = true;
                sess.busy = false;
                sess.rx = None;
                sess.request = None;
            }
        }
    }
    drop(s);
    show_ai(app);
}

/// Polls Claude; keep it alive for the app's lifetime.
pub(crate) fn poll_timer(app: &App) -> Timer {
    let ai_ticker = Timer::default();
    let weak = app.as_weak();
    ai_ticker.start(TimerMode::Repeated, Duration::from_millis(100), move || {
        if let Some(app) = weak.upgrade() {
            poll_ai(&app);
        }
    });
    ai_ticker
}
