//! TUI state and keys (DESIGN.md §4, §12); anything that may prompt hands the terminal to the CLI.

mod audit;
mod backup;
mod backup_search;

pub use backup_search::Hit;
mod jobs;
mod keys;
mod migrate;
mod palette;
mod pulse;
mod settings;
mod timeline;

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver};
use std::time::Instant;

use phx_contract::selection::Choice;
use phx_core::doctor::Finding;
use phx_core::store::Snapshot;
use phx_core::{App, schedule};
use secaudit_core::store::{Accepted, Run, Store};

use super::theme::Theme;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Section {
    Pulse,
    Timeline,
    Audit,
    Backup,
    Migrate,
    Settings,
}

impl Section {
    pub const ALL: [Self; 6] = [
        Self::Pulse,
        Self::Timeline,
        Self::Audit,
        Self::Backup,
        Self::Migrate,
        Self::Settings,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::Pulse => "Pulse",
            Self::Timeline => "Timeline",
            Self::Audit => "Audit",
            Self::Backup => "Backup",
            Self::Migrate => "Migrate",
            Self::Settings => "Settings",
        }
    }
}

pub enum Msg {
    Doctor(Vec<Finding>),
    Choices(Result<Vec<(Choice, Vec<String>)>, String>),
    Audited(Result<PathBuf, String>),
    /// The audit started a section: (done, total, section).
    AuditProgress(usize, usize, String),
    Line(String),
    Done(bool),
}

pub use phx_core::migrate::{Migration, STEPS as MIGRATE_STEPS};

pub enum Suspend {
    Cli(Vec<String>),
    Fix(String),
    SecauditTui,
}

#[allow(
    clippy::struct_excessive_bools,
    reason = "independent UI flags, not a state machine"
)]
pub struct State<'a> {
    pub app: &'a App<'a>,
    pub theme: Theme,
    pub section: Section,
    pub toast: Option<(String, Instant)>,
    pub quit: bool,
    pub quit_armed: bool,
    pub suspend: Option<Suspend>,
    tx: mpsc::Sender<Msg>,
    rx: Receiver<Msg>,
    /// Background jobs running, by kind: `doctor`, `choices`, `audit`, `command`.
    pub jobs: std::collections::BTreeSet<&'static str>,

    pub snaps: Vec<Snapshot>,
    pub snap_sel: usize,
    /// A question in the footer and what `y` does (restore, fix, undo).
    pub confirm: Option<(String, Suspend)>,
    pub help: bool,
    /// `/`: filter text for the current list, and whether keys type into it.
    pub search: String,
    pub searching: bool,
    /// Highlighted row in Backup's search results.
    pub search_sel: usize,
    /// `:` or `⌃k`: the command palette's query and highlighted row.
    pub palette: Option<(String, usize)>,
    pub output: Vec<String>,
    pub output_title: String,

    pub timer: String,
    /// Automatic snapshot interval; `None` is manual only.
    pub interval: Option<&'static str>,
    pub findings: Option<Vec<Finding>>,

    pub audit_store: Store,
    pub accepted: BTreeMap<String, Accepted>,
    pub audit: Option<Run>,
    pub audit_sel: usize,

    pub choices: Option<Vec<(Choice, Vec<String>)>>,
    pub cat_sel: usize,
    pub item_sel: usize,
    pub in_items: bool,
    pub dirty: bool,
    /// Detected items the user hasn't reviewed yet, as `category:id`.
    pub unreviewed: BTreeSet<String>,
    /// Before a snapshot with unreviewed items: `y` snapshots, `b` opens Backup.
    pub ask_snapshot: Option<String>,
    /// The timer's snapshot is running, checked every couple of seconds.
    pub timer_running: bool,
    last_poll: Instant,

    pub migration: Migration,
    pub migrate_sel: usize,
    pub audit_progress: Option<(usize, usize, String)>,
}

fn quiet(_: &str, _: &str) {}

impl<'a> State<'a> {
    pub fn new(app: &'a App<'a>) -> Self {
        Self::with_store(app, Store::from_env())
    }

    /// The state over a given audit history; tests use a throwaway one.
    pub fn with_store(app: &'a App<'a>, audit_store: Store) -> Self {
        let (tx, rx) = mpsc::channel();
        let accepted = audit_store.accepted();
        let audit = audit_store.runs().into_iter().next();
        let migration = Migration::load(&app.state_dir());
        Self {
            app,
            theme: Theme::load(),
            section: Section::Pulse,
            toast: None,
            quit: false,
            quit_armed: false,
            suspend: None,
            tx,
            rx,
            jobs: std::collections::BTreeSet::new(),
            snaps: app.repo.list().unwrap_or_default(),
            snap_sel: 0,
            confirm: None,
            help: false,
            search: String::new(),
            searching: false,
            search_sel: 0,
            palette: None,
            audit_progress: None,
            output: Vec::new(),
            output_title: String::new(),
            timer: schedule::status(),
            interval: schedule::interval(),
            findings: None,
            audit_store,
            accepted,
            audit,
            audit_sel: 0,
            choices: None,
            cat_sel: 0,
            item_sel: 0,
            in_items: false,
            dirty: false,
            unreviewed: BTreeSet::new(),
            ask_snapshot: None,
            timer_running: false,
            last_poll: Instant::now(),
            migration,
            migrate_sel: 0,
        }
    }

    pub fn notify(&mut self, msg: impl Into<String>) {
        self.toast = Some((msg.into(), Instant::now()));
    }

    pub(super) fn save_migration(&self) {
        self.migration.save(&self.app.state_dir());
    }

    pub fn start(&mut self) {
        if !self.app.repo.exists() {
            return;
        }
        self.load_doctor();
        self.load_choices();
        if !self.app.has_chosen() {
            self.section = Section::Backup;
        }
    }

    /// Opens Backup so the user can decide before anything is captured.
    pub(super) fn open_backup(&mut self, why: &str) {
        self.section = Section::Backup;
        self.in_items = false;
        if self.choices.is_none() {
            self.load_choices();
        }
        self.notify(why);
    }
}
