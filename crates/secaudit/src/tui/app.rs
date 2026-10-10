//! State and behaviour of `secaudit-tui`, ported method by method from the
//! Python `AuditApp` (and its screens): same keys, same enabled/disabled
//! rules (`check_action`), same messages. Drawing is in `view`.

use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

use ratatui::crossterm::event::{
    KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};
use secaudit_core::store::{Accepted, Item, Run, Store, now, sev_name, sev_order};

use super::scan::Scan;
use super::theme::Pal;
use super::times::local_time;

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

    /// `in_tab()`.
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

/// `ConfirmScreen`.
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
    /// `AISetupScreen`: one-time consent for AI explanations.
    AiSetup(super::ai_screen::AiSetup),
}

pub enum Screen {
    Main,
    Scan(Box<Scan>),
    /// `HistoryScreen` with its cursor row.
    History(usize),
    /// `LogScreen`: lines, scroll offset, title date.
    Log(Vec<String>, usize, String),
    /// `AIScreen`: Claude explains a finding or plans the scan.
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

    /// `show_run()`.
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

    /// `show_empty()`.
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

    /// Keeps the same finding selected after the view changes, as
    /// `refresh_table()` does; else the same position.
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

    /// The scan before `run`.
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

    // -- check_action ---------------------------------------------------

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

    // -- keys ---------------------------------------------------------------

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

    fn main_key(&mut self, key: KeyEvent) {
        // The search box takes printable keys while focused.
        if self.focus == Focus::Search {
            match key.code {
                KeyCode::Enter | KeyCode::Down | KeyCode::Tab => self.focus = Focus::Table,
                KeyCode::Esc => self.clear_search(),
                KeyCode::BackTab => self.focus = Focus::Tabs,
                KeyCode::Backspace => {
                    self.search.pop();
                    self.refresh(self.selected().map(|f| f.n));
                }
                KeyCode::Char(c) => {
                    self.search.push(c);
                    let keep = self.selected().map(|f| f.n);
                    self.refresh(keep);
                }
                _ => {}
            }
            return;
        }
        if let Focus::Welcome(i) = self.focus {
            match key.code {
                KeyCode::Tab | KeyCode::Right => {
                    self.focus = Focus::Welcome((i + 1) % 3);
                    return;
                }
                KeyCode::BackTab | KeyCode::Left => {
                    self.focus = Focus::Welcome((i + 2) % 3);
                    return;
                }
                KeyCode::Enter | KeyCode::Char(' ') => {
                    self.start_scan(["full", "quick", "deep"].get(i).copied().unwrap_or("full"));
                    return;
                }
                _ => {}
            }
        }
        if self.global_key(key) {
            return;
        }
        match self.focus {
            Focus::Table => self.table_key(key),
            Focus::Sections => self.sections_key(key),
            Focus::Tabs => match key.code {
                KeyCode::Left => self.cycle_tab(false),
                KeyCode::Right => self.cycle_tab(true),
                _ => {}
            },
            Focus::Detail => match key.code {
                KeyCode::Down => self.detail_scroll += 1,
                KeyCode::Up => self.detail_scroll = self.detail_scroll.saturating_sub(1),
                KeyCode::PageDown => self.detail_scroll += 8,
                KeyCode::PageUp => self.detail_scroll = self.detail_scroll.saturating_sub(8),
                KeyCode::Home => self.detail_scroll = 0,
                _ => {}
            },
            _ => {}
        }
    }

    /// App bindings (`BINDINGS`), active on the main screen.
    fn global_key(&mut self, key: KeyEvent) -> bool {
        let has_run = self.current.is_some();
        match key.code {
            KeyCode::Char('q') => self.quit = true,
            KeyCode::Char('?') => self.help = !self.help,
            KeyCode::Char('s') => self.modal = Some(Modal::Chooser(0)),
            KeyCode::Char('r') if has_run => {
                let mode = self
                    .run()
                    .map_or("full".to_owned(), |r| r.meta.mode.clone());
                self.start_scan(&mode);
            }
            KeyCode::Char('h') if !self.runs.is_empty() => self.screen = Screen::History(0),
            KeyCode::Char('/') if has_run => self.focus = Focus::Search,
            KeyCode::Char(c @ '1'..='5') if has_run => {
                let i = usize::from(c as u8 - b'1');
                if let Some(t) = Tab::ALL.get(i) {
                    self.set_tab(*t);
                }
            }
            KeyCode::Char('x') => self.run_fix(),
            KeyCode::Char(' ') if self.can_mark() => self.toggle_mark(),
            KeyCode::Char('*') if self.can_mark() => self.mark_all(),
            KeyCode::Char('a') if self.can_accept() => self.accept(),
            KeyCode::Char('i') if self.can_explain() => self.explain(),
            KeyCode::Char('I') if has_run => self.with_ai(super::ai_screen::Then::Plan),
            KeyCode::Char('c') if self.can_copy() => self.copy(),
            KeyCode::Char('o') if has_run => self.open_log(),
            KeyCode::Char('e') if has_run => self.export(),
            KeyCode::Esc if has_run => self.clear_search(),
            KeyCode::Tab => self.focus_next(true),
            KeyCode::BackTab => self.focus_next(false),
            _ => return false,
        }
        true
    }

    fn focus_next(&mut self, forward: bool) {
        if self.current.is_none() {
            return;
        }
        let order = [
            Focus::Sections,
            Focus::Tabs,
            Focus::Search,
            Focus::Table,
            Focus::Detail,
        ];
        let i = order.iter().position(|f| *f == self.focus).unwrap_or(3);
        let n = order.len();
        self.focus = order
            .get(if forward {
                (i + 1) % n
            } else {
                (i + n - 1) % n
            })
            .copied()
            .unwrap_or(Focus::Table);
    }

    fn table_key(&mut self, key: KeyEvent) {
        let n = self.visible().len();
        let page = usize::from(self.size.1 / 3).max(1);
        let before = self.row;
        match key.code {
            KeyCode::Down | KeyCode::Char('j') => {
                self.row = (self.row + 1).min(n.saturating_sub(1));
            }
            KeyCode::Up | KeyCode::Char('k') => self.row = self.row.saturating_sub(1),
            KeyCode::PageDown => self.row = (self.row + page).min(n.saturating_sub(1)),
            KeyCode::PageUp => self.row = self.row.saturating_sub(page),
            KeyCode::Home => self.row = 0,
            KeyCode::End => self.row = n.saturating_sub(1),
            // `row_selected`: run the fix, else focus the detail.
            KeyCode::Enter => {
                if self.selected().is_some_and(|f| !f.finding.fix.is_empty()) {
                    self.run_fix();
                } else {
                    self.focus = Focus::Detail;
                }
            }
            _ => {}
        }
        if self.row != before {
            self.detail_scroll = 0;
        }
    }

    fn sections_key(&mut self, key: KeyEvent) {
        let n = self.sections().len() + 1;
        match key.code {
            KeyCode::Down => self.section_cursor = (self.section_cursor + 1).min(n - 1),
            KeyCode::Up => self.section_cursor = self.section_cursor.saturating_sub(1),
            KeyCode::Enter => {
                self.focus = Focus::Table;
                return;
            }
            _ => return,
        }
        // `section_highlighted`: the filter follows the highlight.
        let s = self
            .section_cursor
            .checked_sub(1)
            .and_then(|i| self.sections().get(i).cloned());
        if s != self.section {
            self.section = s;
            let keep = self.selected().map(|f| f.n);
            self.refresh(keep);
        }
    }

    fn set_tab(&mut self, t: Tab) {
        if t != self.tab {
            let keep = self.selected().map(|f| f.n);
            self.tab = t;
            self.refresh(keep);
        }
    }

    fn cycle_tab(&mut self, forward: bool) {
        let i = Tab::ALL.iter().position(|t| *t == self.tab).unwrap_or(0);
        let next = if forward { (i + 1) % 5 } else { (i + 4) % 5 };
        if let Some(t) = Tab::ALL.get(next) {
            self.set_tab(*t);
        }
    }

    /// `action_clear_search()`.
    fn clear_search(&mut self) {
        if !self.marked.is_empty() && self.focus != Focus::Search {
            self.marked.clear();
        } else if !self.search.is_empty() {
            self.search.clear();
            let keep = self.selected().map(|f| f.n);
            self.refresh(keep);
        }
        self.focus = Focus::Table;
    }

    /// `action_toggle_mark()`.
    fn toggle_mark(&mut self) {
        let Some(f) = self.selected() else { return };
        if f.finding.fix.is_empty() || f.accepted {
            self.notify(
                "Only findings with an automatic fix can be selected.",
                Severity::Warning,
                3,
            );
            return;
        }
        let n = f.n;
        if !self.marked.remove(&n) {
            self.marked.insert(n);
        }
        let len = self.visible().len();
        self.row = (self.row + 1).min(len.saturating_sub(1));
    }

    /// `action_mark_all()`.
    fn mark_all(&mut self) {
        let applied = self.run().map(Run::applied).unwrap_or_default();
        let fixable: BTreeSet<usize> = self
            .visible()
            .iter()
            .filter(|f| !f.finding.fix.is_empty() && !f.accepted && !applied.contains(&f.n))
            .map(|f| f.n)
            .collect();
        if fixable.is_empty() {
            self.notify(
                "Nothing in this view has an automatic fix.",
                Severity::Warning,
                3,
            );
            return;
        }
        if fixable.is_subset(&self.marked) {
            self.marked = &self.marked - &fixable;
        } else {
            self.marked.extend(fixable);
        }
    }

    fn fix_targets(&self) -> Vec<&Item> {
        let Some(run) = self.run() else {
            return Vec::new();
        };
        if !self.marked.is_empty() {
            return run
                .items
                .iter()
                .filter(|f| self.marked.contains(&f.n))
                .collect();
        }
        self.selected()
            .filter(|f| !f.finding.fix.is_empty() && !f.accepted)
            .into_iter()
            .collect()
    }

    /// `action_run_fix()`: the confirm dialog.
    fn run_fix(&mut self) {
        let targets = self.fix_targets();
        if targets.is_empty() {
            self.notify(
                "No automatic fix for this finding. See How to address.",
                Severity::Warning,
                5,
            );
            return;
        }
        let many = targets.len() > 1;
        let mut code = Vec::new();
        for (i, f) in targets.iter().enumerate() {
            if many {
                code.push(format!("{}. {}", i + 1, f.title()));
            }
            code.push(format!("$ {}", f.finding.fix));
            if many && i + 1 < targets.len() {
                code.push(String::new());
            }
        }
        let mut note = "Runs in this terminal. You'll see the output, then return here.".to_owned();
        if targets.iter().any(|f| f.finding.fix.contains("sudo")) {
            note.push_str(" sudo asks for your password once.");
        }
        let applied = self.run().map(Run::applied).unwrap_or_default();
        let again = targets.iter().filter(|f| applied.contains(&f.n)).count();
        if again > 0 {
            note = format!(
                "{again} of these {} already applied in this scan. {note}",
                if many { "were" } else { "was" }
            );
        }
        let title = if many {
            format!("Run {} fixes?", targets.len())
        } else {
            format!(
                "Run this fix?  {}",
                targets.first().map_or("", |f| f.title())
            )
        };
        let ok = if many {
            format!("Run {}", targets.len())
        } else {
            "Run".into()
        };
        let ns = targets.iter().map(|f| f.n).collect();
        self.modal = Some(Modal::Confirm(Confirm {
            title,
            code,
            note,
            ok,
            danger: false,
            on_ok: false,
            action: Action::RunFixes(ns),
        }));
    }

    /// Runs the fixes in the plain terminal (the loop has left the TUI).
    pub fn run_fixes(&mut self, ns: &[usize]) {
        use std::io::Write;
        let Some(run) = self.run().cloned() else {
            return;
        };
        let targets: Vec<&Item> = run.items.iter().filter(|f| ns.contains(&f.n)).collect();
        let many = targets.len() > 1;
        let mut out = std::io::stdout();
        let mut results = Vec::new();
        for (i, f) in targets.iter().enumerate() {
            let step = if many {
                format!("[{}/{}] ", i + 1, targets.len())
            } else {
                String::new()
            };
            let _ = writeln!(
                out,
                "\n  \x1b[1m{step}{}\x1b[0m\n  \x1b[2m$ {}\x1b[0m",
                f.title(),
                f.finding.fix
            );
            let rc = Command::new("bash")
                .arg("-c")
                .arg(&f.finding.fix)
                .status()
                .ok()
                .and_then(|s| s.code())
                .unwrap_or(1);
            let _ = writeln!(
                out,
                "{}",
                if rc == 0 {
                    "  \x1b[32m✓ done\x1b[0m".to_owned()
                } else {
                    format!("  \x1b[31m✗ exit status {rc}\x1b[0m")
                }
            );
            results.push((f.n, rc));
        }
        let ok_n = results.iter().filter(|(_, rc)| *rc == 0).count();
        let _ = write!(
            out,
            "\n  {ok_n} of {} succeeded.\n  Press Enter to return to Security Audit… ",
            results.len()
        );
        let _ = out.flush();
        let mut line = String::new();
        let _ = std::io::stdin().read_line(&mut line);
        for (n, rc) in &results {
            if *rc == 0 {
                let _ = run.mark_applied(*n);
                self.marked.remove(n);
            }
        }
        let failed = results.len() - ok_n;
        if failed > 0 {
            self.notify(
                &format!(
                    "{failed} fix{} failed; they stay selected.",
                    if failed == 1 { "" } else { "es" }
                ),
                Severity::Error,
                5,
            );
        } else {
            let title = format!(
                "{} fix{} applied",
                results.len(),
                if many { "es" } else { "" }
            );
            self.notify_for(
                Some(&title),
                "Press r to rescan and confirm the issues are gone.",
                Severity::Information,
                5,
            );
        }
    }

    /// `action_accept()`.
    fn accept(&mut self) {
        let Some(f) = self.selected() else { return };
        let (n, was, key, title, section) = (
            f.n,
            f.accepted,
            f.accept_key(),
            f.title().to_owned(),
            f.finding.section.clone(),
        );
        let msg = if was {
            self.accepted.remove(&key);
            format!("Moved back to Issues: {title}")
        } else {
            self.accepted.insert(
                key,
                Accepted {
                    section,
                    msg: title.clone(),
                    at: now(),
                },
            );
            self.marked.remove(&n);
            format!("Accepted, no longer scored: {title}")
        };
        let _ = self.store.save_accepted(&self.accepted);
        for r in &mut self.runs {
            r.recount(&self.accepted);
        }
        self.refresh(Some(n));
        self.notify(
            &format!("{msg}\nPress a again to undo."),
            Severity::Information,
            4,
        );
    }

    /// `action_copy()`.
    fn copy(&mut self) {
        let Some(f) = self.selected() else { return };
        let (text, is_fix) = if f.finding.fix.is_empty() {
            (f.finding.hint.clone(), false)
        } else {
            (f.finding.fix.clone(), true)
        };
        if text.is_empty() {
            return;
        }
        copy_to_clipboard(&text);
        self.notify(
            if is_fix {
                "Fix command copied"
            } else {
                "Advice copied"
            },
            Severity::Information,
            5,
        );
    }

    fn open_log(&mut self) {
        let Some(run) = self.run() else { return };
        let text = std::fs::read_to_string(run.path.join("output.txt"))
            .unwrap_or_else(|_| "(no output saved for this scan)".into());
        // Textual's Log splits each line again, which drops empty ones.
        let lines = strip_ansi(&text)
            .lines()
            .filter(|l| !l.is_empty())
            .map(str::to_owned)
            .collect();
        let title = local_time(run.meta.started, "%a %-d %b %H:%M");
        self.screen = Screen::Log(lines, 0, title);
    }

    /// `action_export()`.
    fn export(&mut self) {
        let Some(run) = self.run() else { return };
        let started = local_time(run.meta.started, "%Y-%m-%d %H:%M");
        match run.export_markdown(&started) {
            Ok(path) => {
                let s = path.display().to_string();
                copy_to_clipboard(&s);
                let home = std::env::var("HOME").unwrap_or_default();
                let shown = if home.is_empty() {
                    s
                } else {
                    s.replacen(&home, "~", 1)
                };
                self.notify_for(
                    Some("Report saved"),
                    &format!("{shown}\nPath copied to clipboard."),
                    Severity::Information,
                    8,
                );
            }
            Err(e) => self.notify(&e.to_string(), Severity::Error, 8),
        }
    }

    // -- scans ----------------------------------------------------------------

    /// `start_scan()`: full and deep scans need sudo first.
    pub fn start_scan(&mut self, mode: &str) {
        if !self.engine.is_file() {
            let msg = format!("secaudit engine not found at {}", self.engine.display());
            self.notify(&msg, Severity::Error, 5);
            return;
        }
        if mode == "quick" || super::scan::sudo_cached() {
            self.launch(mode);
        } else {
            self.suspend = Some(Suspend::Sudo(mode.to_owned()));
        }
    }

    pub fn launch(&mut self, mode: &str) {
        match Scan::start(&self.store, &self.engine, mode, &self.host, &self.user) {
            Ok(scan) => self.screen = Screen::Scan(Box::new(scan)),
            Err(e) => self.notify(&e.to_string(), Severity::Error, 5),
        }
    }

    fn scan_key(&mut self, mut scan: Box<Scan>, key: KeyEvent) {
        match key.code {
            // `action_cancel`: stop the engine, dismiss with nothing.
            KeyCode::Esc => {
                scan.cancel();
                self.scan_done(None);
                return;
            }
            KeyCode::Char('?') => self.help = !self.help,
            KeyCode::Char('q') => {
                scan.cancel();
                self.quit = true;
            }
            _ => {}
        }
        self.screen = Screen::Scan(scan);
    }

    /// `scan_done()`.
    fn scan_done(&mut self, path: Option<&Path>) {
        let Some(path) = path else {
            self.notify("Scan cancelled", Severity::Warning, 5);
            return;
        };
        self.runs = self.store.runs();
        if !self.runs.iter().any(|r| r.path == path) {
            return;
        }
        self.section = None;
        self.show_run(path);
        let Some(run) = self.run() else { return };
        let c = run.counts();
        let body = format!(
            "Score {} ({}) · {} fail · {} warn",
            c.score(),
            c.grade(),
            c.fail,
            c.warn
        );
        let sev = if c.fail > 0 {
            Severity::Error
        } else {
            Severity::Information
        };
        let _ = Command::new("notify-send")
            .args([
                "-a",
                "secaudit",
                "-i",
                "security-high-symbolic",
                "Security audit finished",
                &body,
            ])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn();
        self.notify_for(Some("Scan finished"), &body, sev, 5);
    }

    // -- history, log -----------------------------------------------------------

    fn history_key(&mut self, mut sel: usize, key: KeyEvent) {
        let n = self.runs.len();
        match key.code {
            KeyCode::Esc => {
                self.history_closed(None);
                return;
            }
            KeyCode::Down => sel = (sel + 1).min(n.saturating_sub(1)),
            KeyCode::Up => sel = sel.saturating_sub(1),
            KeyCode::PageDown => sel = (sel + 10).min(n.saturating_sub(1)),
            KeyCode::PageUp => sel = sel.saturating_sub(10),
            KeyCode::Home => sel = 0,
            KeyCode::End => sel = n.saturating_sub(1),
            KeyCode::Enter => {
                let p = self.runs.get(sel).map(|r| r.path.clone());
                self.history_closed(p);
                return;
            }
            KeyCode::Delete | KeyCode::Char('d') => {
                if let Some(r) = self.runs.get(sel) {
                    let when = local_time(r.meta.started, "%a %-d %b %H:%M");
                    self.modal = Some(Modal::Confirm(Confirm {
                        title: format!("Delete the {} scan from {when}?", r.meta.mode),
                        code: Vec::new(),
                        note: String::new(),
                        ok: "Delete".into(),
                        danger: true,
                        on_ok: false,
                        action: Action::DeleteRun(r.path.clone()),
                    }));
                }
            }
            KeyCode::Char('?') => self.help = !self.help,
            KeyCode::Char('q') => self.quit = true,
            _ => {}
        }
        self.screen = Screen::History(sel);
    }

    /// `action_history.opened()`.
    fn history_closed(&mut self, path: Option<PathBuf>) {
        self.screen = Screen::Main;
        self.runs = self.store.runs();
        if let Some(p) = path
            && self.runs.iter().any(|r| r.path == p)
        {
            self.section = None;
            self.show_run(&p);
            return;
        }
        if self.runs.is_empty() {
            self.show_empty();
        } else if let Some(cur) = self.current.clone() {
            let p = if cur.exists() {
                cur
            } else {
                self.runs
                    .first()
                    .map(|r| r.path.clone())
                    .unwrap_or_default()
            };
            self.show_run(&p);
        }
    }

    fn log_key(&mut self, lines: Vec<String>, mut scroll: usize, title: String, key: KeyEvent) {
        let page = usize::from(self.size.1.saturating_sub(3)).max(1);
        let max = lines.len().saturating_sub(page);
        match key.code {
            KeyCode::Esc | KeyCode::Char('q') => return,
            KeyCode::Down => scroll += 1,
            KeyCode::Up => scroll = scroll.saturating_sub(1),
            KeyCode::PageDown | KeyCode::Char(' ') => scroll += page,
            KeyCode::PageUp => scroll = scroll.saturating_sub(page),
            KeyCode::Home => scroll = 0,
            KeyCode::End => scroll = max,
            KeyCode::Char('?') => self.help = !self.help,
            _ => {}
        }
        self.screen = Screen::Log(lines, scroll.min(max), title);
    }

    // -- modals ---------------------------------------------------------------

    // -- AI (the original's action_explain / action_ai_plan) -----------------

    fn explain(&mut self) {
        if let Some(n) = self
            .selected()
            .filter(|f| sev_name(f.finding.sev) != "pass")
            .map(|f| f.n)
        {
            self.with_ai(super::ai_screen::Then::Explain(n));
        }
    }

    /// `with_ai()`: runs `then` once AI is set up, after the one-time dialog if needed.
    fn with_ai(&mut self, then: super::ai_screen::Then) {
        if crate::ai::ready() {
            self.open_ai(then);
        } else {
            self.modal = Some(Modal::AiSetup(super::ai_screen::AiSetup::new(then)));
        }
    }

    fn open_ai(&mut self, then: super::ai_screen::Then) {
        use ratatui::text::Span;
        let Some(run) = self.run() else { return };
        let p = &self.pal;
        let bold = |s: ratatui::style::Style| s.add_modifier(ratatui::style::Modifier::BOLD);
        let (title, prompt, subject) = match then {
            super::ai_screen::Then::Explain(n) => {
                let Some(f) = run.items.iter().find(|i| i.n == n) else {
                    return;
                };
                let mut b = super::widgets::badge(p, sev_name(f.finding.sev));
                b.style = bold(b.style);
                let title = vec![
                    b,
                    Span::styled("  ", bold(p.fg_(p.fg))),
                    Span::styled(f.title().to_owned(), bold(p.fg_(p.fg))),
                    Span::styled(format!("   {}", f.finding.section), bold(p.fg_(p.muted))),
                ];
                (title, crate::ai::explain_prompt(f, run), "explain")
            }
            super::ai_screen::Then::Plan => {
                let c = run.counts();
                let n = c.fail + c.warn;
                let title = vec![
                    Span::styled("Action plan  ", bold(p.fg_(p.fg))),
                    Span::styled(
                        format!(
                            "{n} open issue{} · score {} {}",
                            if n == 1 { "" } else { "s" },
                            c.score(),
                            c.grade()
                        ),
                        bold(p.fg_(p.muted)),
                    ),
                ];
                (title, crate::ai::plan_prompt(run), "action plan")
            }
        };
        let view = super::ai_screen::AiView::open(&self.store.dir, title, prompt, subject);
        self.screen = Screen::Ai(Box::new(view));
    }

    fn setup_button(&mut self, mut setup: super::ai_screen::AiSetup, id: &str) {
        match id {
            "ai-yes" => {
                if setup.enable() {
                    self.open_ai(setup.then);
                } else {
                    self.modal = Some(Modal::AiSetup(setup));
                }
            }
            "ai-use-api" => {
                setup.switch("api");
                self.modal = Some(Modal::AiSetup(setup));
            }
            "ai-use-cc" => {
                setup.switch("claude-code");
                self.modal = Some(Modal::AiSetup(setup));
            }
            _ => {}
        }
    }

    fn ai_key(&mut self, mut v: Box<super::ai_screen::AiView>, key: KeyEvent) {
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        match key.code {
            KeyCode::Esc => {
                v.cancel();
                return;
            }
            KeyCode::Char('r') if ctrl => v.regenerate(),
            KeyCode::Char('o') if ctrl && !v.last_answer.is_empty() => {
                copy_to_clipboard(&v.last_answer);
                self.notify("Answer copied", Severity::Information, 5);
            }
            KeyCode::Tab | KeyCode::BackTab => v.input_focus = !v.input_focus,
            KeyCode::Enter if v.input_focus => v.submit(),
            KeyCode::Backspace if v.input_focus && !v.busy => {
                v.input.pop();
            }
            KeyCode::Char(c) if v.input_focus && !v.busy && !ctrl => v.input.push(c),
            // With the answer focused, the app's keys work.
            KeyCode::Char('q') if !v.input_focus => {
                v.cancel();
                self.quit = true;
            }
            KeyCode::Char('?') if !v.input_focus => self.help = !self.help,
            KeyCode::Up => {
                v.follow = false;
                v.scroll = v.scroll.saturating_sub(1);
            }
            KeyCode::Down => v.scroll += 1,
            KeyCode::PageUp => {
                v.follow = false;
                v.scroll = v.scroll.saturating_sub(usize::from(self.size.1 / 2));
            }
            KeyCode::PageDown => v.scroll += usize::from(self.size.1 / 2),
            KeyCode::Home => {
                v.follow = false;
                v.scroll = 0;
            }
            KeyCode::End => v.follow = true,
            _ => {}
        }
        self.screen = Screen::Ai(v);
    }

    fn modal_key(&mut self, modal: Modal, key: KeyEvent) {
        match modal {
            Modal::AiSetup(mut setup) => {
                let on_key_field = setup.asks_key() && setup.focus == 0;
                match key.code {
                    KeyCode::Esc => {}
                    KeyCode::Tab | KeyCode::Right if !on_key_field || key.code == KeyCode::Tab => {
                        setup.focus = (setup.focus + 1) % setup.slots();
                        self.modal = Some(Modal::AiSetup(setup));
                    }
                    KeyCode::BackTab | KeyCode::Left
                        if !on_key_field || key.code == KeyCode::BackTab =>
                    {
                        setup.focus = (setup.focus + setup.slots() - 1) % setup.slots();
                        self.modal = Some(Modal::AiSetup(setup));
                    }
                    KeyCode::Enter if on_key_field => self.setup_button(setup, "ai-yes"),
                    KeyCode::Enter | KeyCode::Char(' ') if !on_key_field => {
                        let id = setup.focused_button().unwrap_or("ai-no");
                        self.setup_button(setup, id);
                    }
                    KeyCode::Backspace if on_key_field => {
                        setup.key.pop();
                        self.modal = Some(Modal::AiSetup(setup));
                    }
                    KeyCode::Char(c) if on_key_field => {
                        setup.key.push(c);
                        self.modal = Some(Modal::AiSetup(setup));
                    }
                    _ => self.modal = Some(Modal::AiSetup(setup)),
                }
            }
            Modal::Confirm(mut c) => match key.code {
                KeyCode::Char('y') => self.confirmed(c),
                KeyCode::Char('n') | KeyCode::Esc => {}
                KeyCode::Tab | KeyCode::BackTab | KeyCode::Left | KeyCode::Right => {
                    c.on_ok = !c.on_ok;
                    self.modal = Some(Modal::Confirm(c));
                }
                KeyCode::Enter | KeyCode::Char(' ') => {
                    if c.on_ok {
                        self.confirmed(c);
                    }
                }
                _ => self.modal = Some(Modal::Confirm(c)),
            },
            Modal::Chooser(i) => match key.code {
                KeyCode::Esc => {}
                KeyCode::Char('f') => self.start_scan("full"),
                KeyCode::Char('q') => self.start_scan("quick"),
                KeyCode::Char('d') => self.start_scan("deep"),
                KeyCode::Down => self.modal = Some(Modal::Chooser((i + 1).min(2))),
                KeyCode::Up => self.modal = Some(Modal::Chooser(i.saturating_sub(1))),
                KeyCode::Enter => self.start_scan(CHOOSER.get(i).map_or("full", |m| m.0)),
                _ => self.modal = Some(Modal::Chooser(i)),
            },
            Modal::Palette(mut q, mut i) => {
                let n = self.commands(&q).len();
                match key.code {
                    KeyCode::Esc => return,
                    KeyCode::Down => i = (i + 1).min(n.saturating_sub(1)),
                    KeyCode::Up => i = i.saturating_sub(1),
                    KeyCode::Backspace => {
                        q.pop();
                        i = 0;
                    }
                    KeyCode::Enter => {
                        if let Some(cmd) = self.commands(&q).get(i).map(|c| c.0) {
                            self.run_command(cmd);
                        }
                        return;
                    }
                    KeyCode::Char(c) => {
                        q.push(c);
                        i = 0;
                    }
                    _ => {}
                }
                self.modal = Some(Modal::Palette(q, i));
            }
        }
    }

    fn confirmed(&mut self, c: Confirm) {
        match c.action {
            Action::RunFixes(ns) => self.suspend = Some(Suspend::Fixes(ns)),
            Action::DeleteRun(path) => {
                let _ = std::fs::remove_dir_all(&path);
                self.runs = self.store.runs();
                self.notify("Scan deleted", Severity::Information, 5);
                if let Screen::History(sel) = self.screen {
                    self.screen = Screen::History(sel.min(self.runs.len().saturating_sub(1)));
                }
            }
        }
    }

    /// The palette's commands matching `query` (subsequence, any case),
    /// sorted by name: (name, help).
    pub fn commands(&self, query: &str) -> Vec<(&'static str, &'static str)> {
        let mut all = vec![
            (
                "Deep scan",
                "Full scan plus package integrity and whole-disk SUID",
            ),
            ("Full scan", "Run every check with sudo"),
            (
                "Keys",
                if self.help {
                    "Hide the keys and widget help panel"
                } else {
                    "Show help for the focused widget and a summary of available keys"
                },
            ),
            ("Quick scan", "User-level checks without sudo"),
            ("Quit", "Quit the application as soon as possible"),
        ];
        if !self.runs.is_empty() {
            all.push(("Scan history", "Past scans, trend and changes"));
        }
        if self.current.is_some() {
            all.push(("Export Markdown report", "Checklist of issues with fixes"));
            all.push(("Full output", "Raw text output of this scan"));
            all.push((
                "AI action plan",
                "Claude prioritizes every open issue in this scan",
            ));
        }
        if self.can_explain() {
            all.push((
                "Explain with AI",
                "Claude explains the selected finding and how to fix it",
            ));
        }
        all.sort_by_key(|c| c.0);
        let q = query.to_lowercase();
        all.into_iter()
            .filter(|(name, _)| {
                let mut it = name.to_lowercase().chars().collect::<Vec<_>>().into_iter();
                q.chars().all(|qc| it.any(|c| c == qc))
            })
            .collect()
    }

    fn run_command(&mut self, cmd: &str) {
        match cmd {
            "Deep scan" => self.start_scan("deep"),
            "Full scan" => self.start_scan("full"),
            "Quick scan" => self.start_scan("quick"),
            "Keys" => self.help = !self.help,
            "Quit" => self.quit = true,
            "Scan history" => self.screen = Screen::History(0),
            "Export Markdown report" => self.export(),
            "Full output" => self.open_log(),
            "AI action plan" => self.with_ai(super::ai_screen::Then::Plan),
            "Explain with AI" => self.explain(),
            _ => {}
        }
    }

    // -- mouse --------------------------------------------------------------

    pub fn on_mouse(&mut self, m: MouseEvent) {
        let (x, y) = (m.column, m.row);
        if !matches!(
            m.kind,
            MouseEventKind::Down(MouseButton::Left)
                | MouseEventKind::ScrollDown
                | MouseEventKind::ScrollUp
        ) {
            return;
        }
        let hits = self.hits.borrow().clone();
        if let MouseEventKind::ScrollDown | MouseEventKind::ScrollUp = m.kind {
            let down = matches!(m.kind, MouseEventKind::ScrollDown);
            let code = if down { KeyCode::Down } else { KeyCode::Up };
            self.on_key(KeyEvent::new(code, KeyModifiers::NONE));
            return;
        }
        for (bx, by, bw, id) in &hits.buttons {
            if y == *by && x >= *bx && x < bx + bw {
                self.click_button(id);
                return;
            }
        }
        if let Some(Modal::Chooser(_)) = self.modal {
            for (oy, oh, i) in &hits.options {
                if y >= *oy && y < oy + oh {
                    self.modal = None;
                    self.start_scan(CHOOSER.get(*i).map_or("full", |c| c.0));
                    return;
                }
            }
            return;
        }
        if self.modal.is_some() {
            return;
        }
        if let Some((hy, hh, first)) = hits.history
            && y >= hy
            && y < hy + hh
            && let Screen::History(_) = self.screen
        {
            self.screen = Screen::History(
                (first + usize::from(y - hy)).min(self.runs.len().saturating_sub(1)),
            );
            return;
        }
        for (i, (tx0, tx1, ty)) in hits.tabs.iter().enumerate() {
            if y == *ty && x >= *tx0 && x < *tx1 {
                if let Some(t) = Tab::ALL.get(i) {
                    self.set_tab(*t);
                }
                self.focus = Focus::Tabs;
                return;
            }
        }
        if let Some((sx, sy, sw)) = hits.search
            && y >= sy
            && y < sy + 3
            && x >= sx
            && x < sx + sw
        {
            self.focus = Focus::Search;
            return;
        }
        if let Some((tx, ty, th, first)) = hits.table
            && y >= ty
            && y < ty + th
            && x >= tx
        {
            let n = self.visible().len();
            self.row = (first + usize::from(y - ty)).min(n.saturating_sub(1));
            self.detail_scroll = 0;
            self.focus = Focus::Table;
            return;
        }
        if let Some((sx, sy, sh, first)) = hits.sections
            && y >= sy
            && y < sy + sh
            && x >= sx
        {
            self.focus = Focus::Sections;
            self.section_cursor = (first + usize::from(y - sy)).min(self.sections().len());
            self.sections_key(KeyEvent::new(KeyCode::Null, KeyModifiers::NONE));
            let s = self
                .section_cursor
                .checked_sub(1)
                .and_then(|i| self.sections().get(i).cloned());
            if s != self.section {
                self.section = s;
                self.clamp_row();
            }
        }
    }

    fn click_button(&mut self, id: &str) {
        match id {
            "btn-full" => self.start_scan("full"),
            "btn-quick" => self.start_scan("quick"),
            "btn-deep" => self.start_scan("deep"),
            "btn-runfix" => self.run_fix(),
            "btn-copy" => self.copy(),
            "btn-accept" => self.accept(),
            "btn-explain" => self.explain(),
            "ai-no" | "ai-yes" | "ai-use-api" | "ai-use-cc" => {
                if let Some(Modal::AiSetup(setup)) = self.modal.take() {
                    self.setup_button(setup, id);
                }
            }
            "yes" | "no" => {
                if let Some(Modal::Confirm(c)) = self.modal.take()
                    && id == "yes"
                {
                    self.confirmed(c);
                }
            }
            _ => {}
        }
    }
}

/// `ScanChooser.MODES`.
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

/// `copy_to_clipboard()`: wl-copy, else OSC 52.
fn copy_to_clipboard(text: &str) {
    use std::io::Write;
    let piped = Command::new("wl-copy")
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .ok()
        .and_then(|mut c| {
            c.stdin.take()?.write_all(text.as_bytes()).ok()?;
            c.wait().ok()
        })
        .is_some_and(|s| s.success());
    if !piped {
        let b64 = base64(text.as_bytes());
        let _ = write!(std::io::stdout(), "\x1b]52;c;{b64}\x07");
    }
}

fn base64(data: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::new();
    for chunk in data.chunks(3) {
        let b = [
            chunk.first().copied().unwrap_or(0),
            chunk.get(1).copied().unwrap_or(0),
            chunk.get(2).copied().unwrap_or(0),
        ];
        let n = (u32::from(b[0]) << 16) | (u32::from(b[1]) << 8) | u32::from(b[2]);
        for i in 0..4 {
            if i <= chunk.len() {
                out.push(char::from(
                    *T.get(((n >> (18 - 6 * i)) & 63) as usize).unwrap_or(&b'='),
                ));
            } else {
                out.push('=');
            }
        }
    }
    out
}

/// Removes ANSI colour codes, as the log screen does.
pub fn strip_ansi(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        if c == '\x1b' {
            for c in chars.by_ref() {
                if c.is_ascii_alphabetic() {
                    break;
                }
            }
        } else {
            out.push(c);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    #[test]
    fn base64_matches_rfc() {
        assert_eq!(super::base64(b"hi"), "aGk=");
        assert_eq!(super::base64(b"abc"), "YWJj");
    }
}
