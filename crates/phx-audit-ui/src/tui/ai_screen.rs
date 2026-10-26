//! `AIScreen` and `AISetupScreen` of the original: Claude's answer streams
//! in, saved answers come back free, follow-up questions continue the same
//! conversation. The requests themselves are in [`crate::ai`].

use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver};

use ratatui::text::Span;
use serde_json::{Value, json};

use crate::ai::{self, Event, Request};

/// What the screen shows, top to bottom.
pub enum Part {
    Question(String),
    Answer(String),
}

/// The status line under the answer.
pub enum Status {
    Thinking,
    Writing,
    Saved,
    Note(String, Option<String>),
    Error(String, bool),
    Empty,
}

pub struct AiView {
    pub title: Vec<Span<'static>>,
    prompt: String,
    pub subject: String,
    /// The conversation: role and content (text, or the API's blocks).
    messages: Vec<Value>,
    pub last_answer: String,
    pub busy: bool,
    pub backend: &'static str,
    session: Option<String>,
    request: Option<Request>,
    rx: Option<Receiver<Event>>,
    pub parts: Vec<Part>,
    /// Text of the answer streaming in.
    streaming: String,
    pub status: Status,
    pub input: String,
    /// Focus: the follow-up input (true) or the answer (Tab).
    pub input_focus: bool,
    pub scroll: usize,
    /// Follow the end while the answer streams (unless scrolled away).
    pub follow: bool,
    state_dir: PathBuf,
}

impl AiView {
    /// Opens on `prompt`: a saved answer if there is one, else asks.
    pub fn open(
        state_dir: &Path,
        title: Vec<Span<'static>>,
        prompt: String,
        subject: &str,
    ) -> Self {
        let mut v = Self {
            title,
            prompt,
            subject: subject.to_owned(),
            messages: Vec::new(),
            last_answer: String::new(),
            busy: false,
            backend: ai::backend(),
            session: None,
            request: None,
            rx: None,
            parts: Vec::new(),
            streaming: String::new(),
            status: Status::Empty,
            input: String::new(),
            input_focus: true,
            scroll: 0,
            follow: true,
            state_dir: state_dir.to_path_buf(),
        };
        if let Ok(text) = std::fs::read_to_string(ai::cache_path(&v.state_dir, &v.prompt)) {
            v.messages = vec![
                json!({"role": "user", "content": v.prompt}),
                json!({"role": "assistant", "content": text}),
            ];
            v.last_answer.clone_from(&text);
            v.parts.push(Part::Answer(text));
            v.status = Status::Saved;
            v.follow = false;
        } else {
            let p = v.prompt.clone();
            v.ask(&p, false);
        }
        v
    }

    /// `via` on the top bar.
    pub fn via(&self) -> String {
        if self.backend == "claude-code" {
            "Claude Code".into()
        } else {
            ai::model()
        }
    }

    /// The answer text streaming in, for drawing.
    pub fn streaming(&self) -> Option<&str> {
        if self.busy {
            Some(&self.streaming)
        } else {
            None
        }
    }

    fn ask(&mut self, question: &str, show_question: bool) {
        self.busy = true;
        if show_question {
            self.parts.push(Part::Question(question.to_owned()));
        }
        let redacted = ai::redact(question);
        self.messages
            .push(json!({"role": "user", "content": redacted}));
        self.streaming.clear();
        self.status = Status::Thinking;
        self.follow = true;
        let (tx, rx) = mpsc::channel();
        self.rx = Some(rx);
        self.request = Some(if self.backend == "claude-code" {
            let mut prompt = redacted;
            if self.session.is_none() && self.messages.len() > 1 {
                // The first answer came from the cache, so this conversation
                // has no session yet: give it the context.
                let prev: Vec<String> = self
                    .messages
                    .iter()
                    .take(self.messages.len() - 1)
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
                &self.state_dir.join("claude-code"),
                &prompt,
                self.session.as_deref(),
                tx,
            )
        } else {
            ai::ask_api(&self.messages, tx)
        });
    }

    /// Takes what arrived; true when the screen changed.
    pub fn poll(&mut self) -> bool {
        let Some(rx) = &self.rx else { return false };
        let mut events = Vec::new();
        while let Ok(e) = rx.try_recv() {
            events.push(e);
        }
        let changed = !events.is_empty();
        for e in events {
            match e {
                Event::Started => self.status = Status::Writing,
                Event::Text(t) => self.streaming.push_str(&t),
                Event::Done {
                    note,
                    warning,
                    turn,
                    session,
                } => {
                    let text = std::mem::take(&mut self.streaming);
                    self.messages.push(
                        turn.unwrap_or_else(|| json!({"role": "assistant", "content": text})),
                    );
                    if session.is_some() {
                        self.session = session;
                    }
                    self.parts.push(Part::Answer(text.clone()));
                    self.last_answer.clone_from(&text);
                    if self.messages.len() == 2 && !text.is_empty() {
                        let path = ai::cache_path(&self.state_dir, &self.prompt);
                        if let Some(dir) = path.parent() {
                            let _ = std::fs::create_dir_all(dir);
                        }
                        let _ = std::fs::write(path, &text);
                    }
                    self.status = Status::Note(note, warning);
                    self.finish();
                }
                Event::Error { message, retry } => {
                    // Drop the unanswered question so a retry starts from a valid history.
                    if self.messages.last().is_some_and(|m| m["role"] == "user") {
                        self.messages.pop();
                    }
                    let partial = std::mem::take(&mut self.streaming);
                    if !partial.is_empty() {
                        self.parts.push(Part::Answer(partial));
                    }
                    self.status = Status::Error(message, retry);
                    self.finish();
                }
            }
        }
        changed
    }

    fn finish(&mut self) {
        self.busy = false;
        self.rx = None;
        self.request = None;
    }

    /// Enter in the input: a follow-up question.
    pub fn submit(&mut self) {
        let q = self.input.trim().to_owned();
        if q.is_empty() || self.busy {
            return;
        }
        self.input.clear();
        self.ask(&q, true);
    }

    /// `ctrl+r`: forget the saved answer and ask again.
    pub fn regenerate(&mut self) {
        if self.busy {
            return;
        }
        let _ = std::fs::remove_file(ai::cache_path(&self.state_dir, &self.prompt));
        self.messages.clear();
        self.session = None;
        self.parts.clear();
        let p = self.prompt.clone();
        self.ask(&p, false);
    }

    /// Leaving the screen stops the request.
    pub fn cancel(&mut self) {
        if let Some(r) = &mut self.request {
            r.cancel();
        }
        self.finish();
    }
}

/// What the setup dialog leads to once enabled.
#[derive(Clone, Copy)]
pub enum Then {
    /// Explain finding `n` of the current run.
    Explain(usize),
    Plan,
}

/// `AISetupScreen`: one-time consent; Claude Code when installed, else an API key.
pub struct AiSetup {
    pub mode: &'static str,
    pub key: String,
    pub error: String,
    /// Focus: 0 is the key field when shown, then Cancel, the alternative, the primary.
    pub focus: usize,
    pub then: Then,
}

impl AiSetup {
    pub fn new(then: Then) -> Self {
        let mode = if ai::claude_bin().is_some() {
            "claude-code"
        } else {
            "api"
        };
        let mut s = Self {
            mode,
            key: String::new(),
            error: String::new(),
            focus: 0,
            then,
        };
        s.focus = s.first_focus();
        s
    }

    /// The key field is shown for the API without saved or env credentials.
    pub fn asks_key(&self) -> bool {
        self.mode == "api" && !ai::env_credentials() && !ai::has_stored_key()
    }

    /// The buttons, left to right: (id, label, primary).
    pub fn buttons(&self) -> Vec<(&'static str, &'static str, bool)> {
        let mut b = vec![("ai-no", "Cancel", false)];
        if self.mode == "claude-code" {
            b.push(("ai-use-api", "Use an API key instead", false));
            b.push(("ai-yes", "Use Claude Code", true));
        } else {
            if ai::claude_bin().is_some() {
                b.push(("ai-use-cc", "Use Claude Code instead", false));
            }
            b.push(("ai-yes", "Enable", true));
        }
        b
    }

    fn first_focus(&self) -> usize {
        // The key field, else the primary button, as the original focuses.
        if self.asks_key() {
            0
        } else {
            self.buttons().len() - 1
        }
    }

    /// Focusable count: the key field (if any) and the buttons.
    pub fn slots(&self) -> usize {
        usize::from(self.asks_key()) + self.buttons().len()
    }

    /// The focused button's id, if a button has focus.
    pub fn focused_button(&self) -> Option<&'static str> {
        let i = self.focus.checked_sub(usize::from(self.asks_key()))?;
        self.buttons().get(i).map(|b| b.0)
    }

    pub fn switch(&mut self, mode: &'static str) {
        self.mode = mode;
        self.error.clear();
        self.focus = self.first_focus();
    }

    /// `enable()`: true when consent is saved and the action can go ahead.
    pub fn enable(&mut self) -> bool {
        if self.asks_key() {
            let key = self.key.trim();
            if !key.starts_with("sk-ant-") {
                self.error =
                    "That doesn't look like an Anthropic API key (they start with sk-ant-).".into();
                return false;
            }
            if let Err(e) = ai::store_key(key) {
                self.error = e.to_string();
                return false;
            }
        }
        match ai::save_consent(self.mode) {
            Ok(()) => true,
            Err(e) => {
                self.error = e.to_string();
                false
            }
        }
    }
}
