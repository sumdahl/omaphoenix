//! State and behaviour of `omaphoenix audit tui`, a method-by-method port of the Python `AuditApp`.

mod ai;
mod fixes;
mod history;
mod main_keys;
mod modals;
mod mouse;
mod output;
mod scans;
mod shown_run;
mod types;
mod util;

use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;
use std::time::{Duration, Instant};

use phx_audit::store::{Accepted, Item, Run, Store, sev_name};
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use super::theme::Pal;
pub use types::{
    Action, CHOOSER, Confirm, Focus, Hits, Modal, NARROW, Screen, Severity, Suspend, Tab, Toast,
    WIDE,
};

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

fn engine_path() -> PathBuf {
    crate::engine::exe()
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
            let msg = format!(
                "the audit engine (omaphoenix) was not found at {}",
                app.engine.display()
            );
            app.notify_for(None, &msg, Severity::Error, 30);
        }
        match app.runs.first().map(|r| r.path.clone()) {
            Some(p) => app.show_run(&p),
            None => app.focus = Focus::Welcome(0),
        }
        app
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
