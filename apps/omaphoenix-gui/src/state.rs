//! UI-thread state shared by every section.

use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;
use std::rc::Rc;

use phx_audit::store::{Accepted, Run};
use phx_contract::selection::Choice;
use phx_core::App as Engine;
use phx_core::migrate::Migration;
use slint::{ModelRc, Timer, VecModel};

use crate::motion::{Motion, SHEET_H};
use crate::spring::{GLIDE, SHEET, Spring};

pub(crate) fn quiet(_: &str, _: &str) {}

#[derive(Default)]
pub(crate) struct Model {
    pub(crate) choices: Vec<(Choice, Vec<String>)>,
    pub(crate) audit: Option<Run>,
    pub(crate) accepted: BTreeMap<String, Accepted>,
    pub(crate) migration: Migration,
    pub(crate) preview: String,
    /// Detected items the user hasn't reviewed, as `category:id`.
    pub(crate) unreviewed: BTreeSet<String>,
    /// Background loads in flight, for the status line: `choices`, `doctor`.
    pub(crate) jobs: BTreeSet<&'static str>,
    pub(crate) timer_running: bool,
    /// Search text per section: Timeline, Audit, Backup.
    pub(crate) query: Queries,
    /// Backup rows on screen as (category, item) while searching.
    pub(crate) shown: Vec<(usize, usize)>,
}

#[derive(Default)]
pub(crate) struct Queries {
    pub(crate) timeline: String,
    pub(crate) audit: String,
    pub(crate) backup: String,
}

pub(crate) type Shared = Rc<RefCell<Model>>;

// `invoke_from_event_loop` closures must be `Send`, so they fetch these on arrival instead of capturing `Rc`s.
thread_local! {
    static STATE: Shared = Rc::new(RefCell::new(Model::default()));
    static MOTION: Rc<RefCell<Motion>> = Rc::new(RefCell::new(Motion {
        ring: Spring::new(0.0, GLIDE),
        drop: Spring::new(0.0, GLIDE),
        sheet: Spring::new(SHEET_H, SHEET),
        sheet_open: false,
        reduced: false,
        last: None,
        drag: Vec::new(),
        drag_start: SHEET_H,
    }));
    static TIMER: Rc<Timer> = Rc::new(Timer::default());
}

/// Built per use: sharing one across threads would need its log callback to be `Sync`.
pub(crate) fn ui_engine() -> Engine<'static> {
    Engine::from_env(&quiet).unwrap_or_else(|_| Engine {
        home: PathBuf::new(),
        repo: phx_core::store::Repo {
            dir: PathBuf::new(),
        },
        log: &quiet,
        passphrase: None,
    })
}

pub(crate) fn ui_state() -> Shared {
    STATE.with(Rc::clone)
}

pub(crate) fn ui_motion() -> Rc<RefCell<Motion>> {
    MOTION.with(Rc::clone)
}

pub(crate) fn ui_timer() -> Rc<Timer> {
    TIMER.with(Rc::clone)
}

pub(crate) fn model<T: Clone + 'static>(v: Vec<T>) -> ModelRc<T> {
    ModelRc::new(VecModel::from(v))
}
