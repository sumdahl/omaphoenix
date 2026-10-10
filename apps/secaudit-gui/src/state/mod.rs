use std::cell::RefCell;
use std::collections::BTreeMap;
use std::process::Command;
use std::rc::Rc;
use std::time::{Duration, Instant};

use secaudit_core::store::{Accepted, Run, Store};
use slint::{ComponentHandle, ModelRc, SharedString, Timer, TimerMode, VecModel};

use crate::assistant::{AiSess, Then};
use crate::ui::App;

mod view;

pub(crate) use view::{refresh, selected, wire};

pub(crate) struct State {
    pub(crate) store: Store,
    pub(crate) runs: Vec<Run>,
    pub(crate) current: usize,
    pub(crate) accepted: BTreeMap<String, Accepted>,
    /// Line numbers (`Item::n`) behind the rows on screen.
    pub(crate) shown: Vec<usize>,
    pub(crate) toast_until: Option<Instant>,
    /// History row armed for deletion, and until when.
    pub(crate) delete_armed: Option<(usize, Instant)>,
    pub(crate) ai: Option<AiSess>,
    /// What the consent dialog leads to, and its backend choice.
    pub(crate) setup: Option<(Then, &'static str)>,
}

impl State {
    pub(crate) fn run(&self) -> Option<&Run> {
        self.runs.get(self.current)
    }

    pub(crate) fn reload(&mut self) {
        self.accepted = self.store.accepted();
        self.runs = self.store.runs();
        for r in &mut self.runs {
            r.recount(&self.accepted);
        }
        self.current = self.current.min(self.runs.len().saturating_sub(1));
    }
}

pub(crate) type Shared = Rc<RefCell<State>>;

thread_local! {
    pub(crate) static STATE: RefCell<Option<Shared>> = const { RefCell::new(None) };
}

pub(crate) fn state() -> Shared {
    STATE.with(|s| s.borrow().clone()).unwrap_or_else(|| {
        Rc::new(RefCell::new(State {
            store: Store::from_env(),
            runs: Vec::new(),
            current: 0,
            accepted: BTreeMap::new(),
            shown: Vec::new(),
            toast_until: None,
            delete_armed: None,
            ai: None,
            setup: None,
        }))
    })
}

pub(crate) fn model<T: Clone + 'static>(v: Vec<T>) -> ModelRc<T> {
    ModelRc::new(Rc::new(VecModel::from(v)))
}

pub(crate) fn int(n: usize) -> i32 {
    i32::try_from(n).unwrap_or(i32::MAX)
}

/// `date -d @epoch +fmt`: local time without a time-zone crate.
pub(crate) fn local(epoch: u64, fmt: &str) -> String {
    Command::new("date")
        .args(["-d", &format!("@{epoch}"), &format!("+{fmt}")])
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_owned())
        .unwrap_or_default()
}

pub(crate) fn toast(app: &App, msg: impl Into<SharedString>) {
    app.set_toast(msg.into());
    state().borrow_mut().toast_until = Some(Instant::now() + Duration::from_secs(5));
}

pub(crate) fn toast_timer(app: &App) -> Timer {
    let ticker = Timer::default();
    let weak = app.as_weak();
    ticker.start(TimerMode::Repeated, Duration::from_millis(500), move || {
        let Some(app) = weak.upgrade() else { return };
        let expired = state()
            .borrow()
            .toast_until
            .is_some_and(|t| Instant::now() >= t);
        if expired {
            state().borrow_mut().toast_until = None;
            app.set_toast(SharedString::new());
        }
    });
    ticker
}
