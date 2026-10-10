use std::sync::mpsc::Receiver;

use secaudit::ai::{Event, Request};
use serde_json::Value;

mod markdown;
mod poll;
mod session;
mod wire;

pub(crate) use poll::poll_timer;
pub(crate) use wire::wire;

#[derive(Clone, Copy)]
pub(crate) enum Then {
    /// Finding `n` of the current run.
    Explain(usize),
    Plan,
}

pub(crate) struct AiSess {
    prompt: String,
    title: String,
    backend: &'static str,
    messages: Vec<Value>,
    session: Option<String>,
    request: Option<Request>,
    rx: Option<Receiver<Event>>,
    /// (is a question, text)
    parts: Vec<(bool, String)>,
    streaming: String,
    last_answer: String,
    status: String,
    error: bool,
    busy: bool,
}
