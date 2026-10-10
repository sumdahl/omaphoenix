//! State and behaviour of `secaudit-tui`, a method-by-method port of the Python `AuditApp`.

mod actions;
mod ai;
mod history;
mod main_keys;
mod modals;
mod mouse;
mod util;

use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use secaudit_core::store::{Accepted, Item, Run, Store, sev_name, sev_order};

use super::scan::Scan;
use super::theme::Pal;

/// Columns at which the detail pane moves beside the table.
pub const WIDE: u16 = 150;

/// Below this the trend box hides and the sidebar shrinks.
pub const NARROW: u16 = 110;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tab {
    Issues,
    Accepted,
    Notes,
    Passed,
    All,
}

impl Tab {
    pub const ALL: [Self; 5] = [
        Self::Issues,
        Self::Accepted,
        Self::Notes,
        Self::Passed,
        Self::All,
    ];

    pub fn holds(self, f: &Item) -> bool {
        let sev = sev_name(f.finding.sev);
        match self {
            Self::Issues => f.is_issue() && !f.accepted,
            Self::Accepted => f.accepted,
            Self::Notes => sev == "info" || sev == "skip",
            Self::Passed => sev == "pass",
            Self::All => true,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Focus {
    Sections,
    Tabs,
    Search,
    Table,
    Detail,
    /// The welcome screen's buttons: Full, Quick, Deep.
    Welcome(usize),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Severity {
    Information,
    Warning,
    Error,
}

pub struct Toast {
    pub title: Option<String>,
    pub body: String,
    pub severity: Severity,
    pub until: Instant,
}

/// What a confirm dialog does on yes.
pub enum Action {
    RunFixes(Vec<usize>),
    DeleteRun(PathBuf),
}

pub struct Confirm {
    pub title: String,
    pub code: Vec<String>,
    pub note: String,
    pub ok: String,
    pub danger: bool,
    /// The focused button: false = Cancel (default), true = ok.
    pub on_ok: bool,
    pub action: Action,
}

pub enum Modal {
    Confirm(Confirm),
    /// `ScanChooser`, with the highlighted option.
    Chooser(usize),
    /// The command palette: query and highlighted row.
    Palette(String, usize),
    AiSetup(super::ai_screen::AiSetup),
}

pub enum Screen {
    Main,
    Scan(Box<Scan>),
    /// `HistoryScreen` with its cursor row.
    History(usize),
    /// `LogScreen`: lines, scroll offset, title date.
    Log(Vec<String>, usize, String),
    Ai(Box<super::ai_screen::AiView>),
}

pub enum Suspend {
    Sudo(String),
    Fixes(Vec<usize>),
}

/// Clickable regions recorded while drawing.
#[derive(Default, Clone)]
pub struct Hits {
    pub tabs: Vec<(u16, u16, u16)>,
    pub table: Option<(u16, u16, u16, usize)>,
    pub sections: Option<(u16, u16, u16, usize)>,
    pub search: Option<(u16, u16, u16)>,
    pub buttons: Vec<(u16, u16, u16, &'static str)>,
    pub options: Vec<(u16, u16, usize)>,
    pub history: Option<(u16, u16, usize)>,
}

pub struct App {
    pub pal: Pal,
    pub store: Store,
    pub engine: PathBuf,
    pub host: String,
    pub user: String,
    pub runs: Vec<Run>,
    pub current: Option<PathBuf>,
    pub accepted: BTreeMap<String, Accepted>,
    pub tab: Tab,
    pub section: Option<String>,
    pub section_cursor: usize,
    pub search: String,
    pub marked: BTreeSet<usize>,
    pub row: usize,
    pub row_offset: std::cell::Cell<usize>,
    pub sec_offset: std::cell::Cell<usize>,
    pub hist_offset: std::cell::Cell<usize>,
    pub detail_scroll: usize,
    pub focus: Focus,
    pub screen: Screen,
    pub modal: Option<Modal>,
    pub help: bool,
    pub toasts: Vec<Toast>,
    pub suspend: Option<Suspend>,
    pub quit: bool,
    pub size: (u16, u16),
    pub hits: RefCell<Hits>,
}

/// The engine: `$SECAUDIT_ENGINE`, else `secaudit` next to this binary.
fn engine_path() -> PathBuf {
    std::env::var_os("SECAUDIT_ENGINE").map_or_else(
        || {
            std::env::current_exe()
                .ok()
                .and_then(|e| e.parent().map(|d| d.join("secaudit")))
                .unwrap_or_else(|| PathBuf::from("secaudit"))
        },
        PathBuf::from,
    )
}

fn hostname() -> String {
    std::fs::read_to_string("/proc/sys/kernel/hostname")
        .unwrap_or_default()
        .trim()
        .to_owned()
}

impl App {
    pub fn new(store: Store) -> Self {
        let runs = store.runs();
        let mut app = Self {
            pal: Pal::load(),
            accepted: store.accepted(),
            store,
            engine: engine_path(),
            host: hostname(),
            user: std::env::var("USER").unwrap_or_default(),
            runs,
            current: None,
            tab: Tab::Issues,
            section: None,
            section_cursor: 0,
            search: String::new(),
            marked: BTreeSet::new(),
            row: 0,
            row_offset: std::cell::Cell::new(0),
            sec_offset: std::cell::Cell::new(0),
            hist_offset: std::cell::Cell::new(0),
            detail_scroll: 0,
            focus: Focus::Table,
            screen: Screen::Main,
            modal: None,
            help: false,
            toasts: Vec::new(),
            suspend: None,
            quit: false,
            size: (80, 24),
            hits: RefCell::new(Hits::default()),
        };
        if !app.engine.is_file() {
            let msg = format!("secaudit engine not found at {}", app.engine.display());
            app.notify_for(None, &msg, Severity::Error, 30);
        }
        match app.runs.first().map(|r| r.path.clone()) {
            Some(p) => app.show_run(&p),
            None => app.focus = Focus::Welcome(0),
        }
        app
    }
    pub fn run(&self) -> Option<&Run> {
        let cur = self.current.as_ref()?;
        self.runs.iter().find(|r| &r.path == cur)
    }

    pub fn notify(&mut self, body: &str, severity: Severity, secs: u64) {
        self.notify_for(None, body, severity, secs);
    }

    pub fn notify_for(&mut self, title: Option<&str>, body: &str, severity: Severity, secs: u64) {
        self.toasts.push(Toast {
            title: title.map(str::to_owned),
            body: body.to_owned(),
            severity,
            until: Instant::now() + Duration::from_secs(secs),
        });
    }

    pub fn show_run(&mut self, path: &Path) {
        if self.current.as_deref() != Some(path) {
            self.marked.clear();
        }
        self.current = Some(path.to_path_buf());
        let sections = self.sections();
        if self.section.as_ref().is_some_and(|s| !sections.contains(s)) {
            self.section = None;
        }
        self.section_cursor = self
            .section
            .as_ref()
            .and_then(|s| sections.iter().position(|x| x == s))
            .map_or(0, |i| i + 1);
        self.focus = Focus::Table;
        self.clamp_row();
    }

    fn show_empty(&mut self) {
        self.current = None;
        self.focus = Focus::Welcome(0);
    }

    /// Sections present in the run, in engine order, then any others.
    pub fn sections(&self) -> Vec<String> {
        let Some(run) = self.run() else {
            return Vec::new();
        };
        let present: BTreeSet<&str> = run
            .items
            .iter()
            .map(|i| i.finding.section.as_str())
            .collect();
        let order: Vec<&str> = secaudit_core::checks::SECTIONS
            .iter()
            .map(|s| s.name)
            .collect();
        let mut out: Vec<String> = order
            .iter()
            .filter(|s| present.contains(**s))
            .map(|s| (*s).to_owned())
            .collect();
        out.extend(
            present
                .iter()
                .filter(|s| !order.contains(s))
                .map(|s| (*s).to_owned()),
        );
        out
    }

    /// `visible()`: tab, section and search applied, worst first.
    pub fn visible(&self) -> Vec<&Item> {
        let Some(run) = self.run() else {
            return Vec::new();
        };
        let q = self.search.trim().to_lowercase();
        let mut rows: Vec<&Item> = run
            .items
            .iter()
            .filter(|f| self.tab.holds(f))
            .filter(|f| {
                self.section
                    .as_ref()
                    .is_none_or(|s| &f.finding.section == s)
            })
            .filter(|f| {
                let fd = &f.finding;
                q.is_empty()
                    || format!(
                        "{}\n{}\n{}\n{}\n{}",
                        fd.msg, fd.section, fd.detail, fd.fix, fd.hint
                    )
                    .to_lowercase()
                    .contains(&q)
            })
            .collect();
        rows.sort_by_key(|f| (sev_order(f.finding.sev), f.n));
        rows
    }

    pub fn selected(&self) -> Option<&Item> {
        self.visible().get(self.row).copied()
    }

    fn clamp_row(&mut self) {
        let n = self.visible().len();
        self.row = self.row.min(n.saturating_sub(1));
        self.detail_scroll = 0;
    }

    /// Keeps the same finding selected after the view changes (`refresh_table()`), else the same position.
    fn refresh(&mut self, keep: Option<usize>) {
        if let Some(n) = keep
            && let Some(i) = self.visible().iter().position(|f| f.n == n)
        {
            self.row = i;
        }
        self.clamp_row();
    }

    fn previous_of(&self, path: &Path) -> Option<&Run> {
        let i = self.runs.iter().position(|r| r.path == path)?;
        self.runs.get(i + 1)
    }

    pub fn previous_of_run(&self, run: &Run) -> Option<&Run> {
        self.previous_of(&run.path)
    }

    pub fn tick(&mut self) -> bool {
        let before = self.toasts.len();
        self.toasts.retain(|t| t.until > Instant::now());
        let mut changed = before != self.toasts.len();
        if let Screen::Ai(v) = &mut self.screen {
            changed |= v.poll();
        }
        if let Screen::Scan(scan) = &mut self.screen {
            changed |= scan.poll(&self.store);
            changed |= scan.running();
            if scan.finished {
                let dir = scan.dir.clone();
                self.screen = Screen::Main;
                self.scan_done(Some(&dir));
                changed = true;
            }
        }
        changed
    }

    pub fn can_run_fix(&self) -> bool {
        !self.marked.is_empty()
            || self
                .selected()
                .is_some_and(|f| !f.finding.fix.is_empty() && !f.accepted)
    }

    pub fn can_accept(&self) -> bool {
        self.selected().is_some_and(Item::is_issue)
    }

    pub fn can_mark(&self) -> bool {
        self.current.is_some() && self.tab != Tab::Accepted
    }

    /// `check_action("explain")`: a finding that isn't a pass.
    pub fn can_explain(&self) -> bool {
        self.selected()
            .is_some_and(|f| sev_name(f.finding.sev) != "pass")
    }

    pub fn can_copy(&self) -> bool {
        self.selected()
            .is_some_and(|f| !f.finding.fix.is_empty() || !f.finding.hint.is_empty())
    }

    pub fn on_key(&mut self, key: KeyEvent) {
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        // Textual: ctrl+c copies, and says how to quit.
        if ctrl && key.code == KeyCode::Char('c') {
            self.notify("Press ctrl+q to quit the app", Severity::Information, 5);
            return;
        }
        if let Some(modal) = self.modal.take() {
            self.modal_key(modal, key);
            return;
        }
        if ctrl && key.code == KeyCode::Char('p') {
            self.modal = Some(Modal::Palette(String::new(), 0));
            return;
        }
        if ctrl && key.code == KeyCode::Char('q') {
            if let Screen::Scan(scan) = &mut self.screen {
                scan.cancel();
            }
            self.quit = true;
            return;
        }
        match std::mem::replace(&mut self.screen, Screen::Main) {
            Screen::Main => self.main_key(key),
            Screen::Scan(scan) => self.scan_key(scan, key),
            Screen::History(sel) => self.history_key(sel, key),
            Screen::Log(lines, scroll, title) => self.log_key(lines, scroll, title, key),
            Screen::Ai(v) => self.ai_key(v, key),
        }
    }
}

pub const CHOOSER: [(&str, &str, &str, &str); 3] = [
    (
        "full",
        "Full scan",
        "f",
        "sudo · every check · about 20 seconds",
    ),
    (
        "quick",
        "Quick scan",
        "q",
        "no sudo · skips root-only checks · about 10 seconds",
    ),
    (
        "deep",
        "Deep scan",
        "d",
        "sudo · adds package integrity and whole-disk SUID · several minutes",
    ),
];
