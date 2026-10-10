//! TUI state and keys (DESIGN.md §4, §12): six sections on `1`–`6`.
//! Slow reads run on threads; anything that may prompt (snapshot, restore,
//! secrets) hands the terminal to the CLI, as the Go TUI did.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::mpsc::{self, Receiver};
use std::time::{Duration, Instant};

use phx_contract::selection::Choice;
use phx_core::doctor::Finding;
use phx_core::store::Snapshot;
use phx_core::{App, schedule};
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
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

/// Results of background work.
pub enum Msg {
    Doctor(Vec<Finding>),
    Choices(Result<Vec<(Choice, Vec<String>)>, String>),
    Audited(Result<PathBuf, String>),
    /// A line of a child command's output, for the output pane.
    Line(String),
    /// The child command finished; `true` on success.
    Done(bool),
}

pub use phx_core::migrate::{Migration, STEPS as MIGRATE_STEPS};

pub enum Suspend {
    /// Run `omaphoenix <args>` in the plain terminal.
    Cli(Vec<String>),
    /// Run an audit finding's fix command in the plain terminal.
    Fix(String),
    /// Open the full Security Audit app (`secaudit tui`), same history.
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

    // Timeline
    pub snaps: Vec<Snapshot>,
    pub snap_sel: usize,
    /// A question in the footer and what `y` does (restore, fix, undo).
    pub confirm: Option<(String, Suspend)>,
    /// `?`: the keys for this screen.
    pub help: bool,
    /// `/`: filter text for the current list, and whether keys type into it.
    pub search: String,
    pub searching: bool,
    /// `:` or `⌃k`: the command palette's query and highlighted row.
    pub palette: Option<(String, usize)>,
    pub output: Vec<String>,
    pub output_title: String,

    // Pulse / Settings
    pub timer: String,
    pub findings: Option<Vec<Finding>>,

    // Audit
    pub audit_store: Store,
    pub accepted: BTreeMap<String, Accepted>,
    pub audit: Option<Run>,
    pub audit_sel: usize,

    // Backup
    pub choices: Option<Vec<(Choice, Vec<String>)>>,
    pub cat_sel: usize,
    pub item_sel: usize,
    pub in_items: bool,
    pub dirty: bool,

    // Migrate
    pub migration: Migration,
    pub migrate_sel: usize,
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
            palette: None,
            output: Vec::new(),
            output_title: String::new(),
            timer: schedule::status(),
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
            migration,
            migrate_sel: 0,
        }
    }

    pub fn notify(&mut self, msg: impl Into<String>) {
        self.toast = Some((msg.into(), Instant::now()));
    }

    /// Applies finished background work; true when the screen changed.
    pub fn tick(&mut self) -> bool {
        let mut changed = false;
        if self
            .toast
            .as_ref()
            .is_some_and(|(_, at)| at.elapsed() > Duration::from_secs(5))
        {
            self.toast = None;
            changed = true;
        }
        while let Ok(msg) = self.rx.try_recv() {
            changed = true;
            match msg {
                Msg::Doctor(f) => {
                    self.findings = Some(f);
                    self.jobs.remove("doctor");
                }
                Msg::Choices(Ok(c)) => {
                    self.choices = Some(c);
                    self.jobs.remove("choices");
                }
                Msg::Choices(Err(e)) => {
                    self.jobs.remove("choices");
                    self.notify(e);
                }
                Msg::Audited(r) => {
                    self.jobs.remove("audit");
                    match r {
                        Ok(dir) => {
                            self.audit = self
                                .audit_store
                                .runs()
                                .into_iter()
                                .find(|run| run.path == dir);
                            self.audit_sel = 0;
                            if !self.migration.audited {
                                self.migration.audited = true;
                                self.save_migration();
                            }
                            if let Some(run) = &self.audit {
                                let c = run.counts();
                                self.notify(format!(
                                    "Audit done: score {} ({}), {} fail, {} warn",
                                    c.score(),
                                    c.grade(),
                                    c.fail,
                                    c.warn
                                ));
                            }
                        }
                        Err(e) => self.notify(format!("Audit failed: {e}")),
                    }
                }
                Msg::Line(l) => self.output.push(l),
                Msg::Done(ok) => {
                    self.jobs.remove("command");
                    self.snaps = self.app.repo.list().unwrap_or_default();
                    if ok
                        && self.output_title.starts_with("Snapshot")
                        && let Some(s) = self.snaps.first()
                    {
                        self.migration.snapshot.clone_from(&s.id);
                        self.save_migration();
                    }
                }
            }
        }
        changed
    }

    fn save_migration(&self) {
        self.migration.save(&self.app.state_dir());
    }

    /// Runs `omaphoenix <args>` with output captured into the output pane.
    fn run_captured(&mut self, title: &str, args: &[&str]) {
        if self.jobs.contains("command") {
            self.notify("Still running the last command; wait for it to finish");
            return;
        }
        let Ok(exe) = std::env::current_exe() else {
            return;
        };
        let child = Command::new(exe)
            .args(args)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn();
        let mut child = match child {
            Ok(c) => c,
            Err(e) => {
                self.notify(format!("couldn't start: {e}"));
                return;
            }
        };
        self.output.clear();
        title.clone_into(&mut self.output_title);
        self.jobs.insert("command");
        let (out, err) = (child.stdout.take(), child.stderr.take());
        let tx = self.tx.clone();
        let tx_err = self.tx.clone();
        std::thread::spawn(move || {
            use std::io::BufRead;
            if let Some(e) = err {
                for l in std::io::BufReader::new(e).lines().map_while(Result::ok) {
                    let _ = tx_err.send(Msg::Line(l));
                }
            }
        });
        std::thread::spawn(move || {
            use std::io::BufRead;
            if let Some(o) = out {
                for l in std::io::BufReader::new(o).lines().map_while(Result::ok) {
                    let _ = tx.send(Msg::Line(l));
                }
            }
            let ok = child.wait().is_ok_and(|s| s.success());
            let _ = tx.send(Msg::Done(ok));
        });
    }

    fn load_doctor(&mut self) {
        if !self.jobs.insert("doctor") {
            return;
        }
        let tx = self.tx.clone();
        std::thread::spawn(move || {
            let findings = App::from_env(&quiet)
                .map(|a| a.doctor())
                .unwrap_or_default();
            let _ = tx.send(Msg::Doctor(findings));
        });
    }

    fn load_choices(&mut self) {
        if !self.jobs.insert("choices") {
            return;
        }
        let tx = self.tx.clone();
        std::thread::spawn(move || {
            let r = App::from_env(&quiet)
                .and_then(|a| a.choices())
                .map_err(|e| e.to_string());
            let _ = tx.send(Msg::Choices(r));
        });
    }

    /// A quick audit, in-process, saved where the Security Audit app keeps
    /// its history so both show the same scans.
    fn run_audit(&mut self) {
        if !self.jobs.insert("audit") {
            return;
        }
        let tx = self.tx.clone();
        let store = self.audit_store.clone();
        std::thread::spawn(move || {
            let r = audit_now(&store);
            let _ = tx.send(Msg::Audited(r));
        });
    }

    pub fn on_key(&mut self, key: KeyEvent) {
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
            self.quit = true;
            return;
        }
        if let Some((_, work)) = self.confirm.take() {
            if key.code == KeyCode::Char('y') {
                self.suspend = Some(work);
            }
            return;
        }
        if let Some((q, sel)) = self.palette.take() {
            self.palette_key(q, sel, key);
            return;
        }
        if self.searching {
            self.search_key(key);
            return;
        }
        if self.help {
            self.help = false;
            if matches!(key.code, KeyCode::Esc | KeyCode::Char('?')) {
                return;
            }
        }
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        let was_armed = std::mem::take(&mut self.quit_armed);
        match key.code {
            KeyCode::Char('?') => {
                self.help = true;
                return;
            }
            KeyCode::Char(':') => {
                self.palette = Some((String::new(), 0));
                return;
            }
            KeyCode::Char('k') if ctrl => {
                self.palette = Some((String::new(), 0));
                return;
            }
            KeyCode::Char('/') => {
                if matches!(self.section, Section::Timeline | Section::Audit) {
                    self.searching = true;
                } else {
                    self.notify("/ searches the Timeline and Audit lists");
                }
                return;
            }
            KeyCode::Char('u') => {
                self.ask_undo();
                return;
            }
            KeyCode::Esc if !self.search.is_empty() => {
                self.search.clear();
                return;
            }
            KeyCode::Char('q') => {
                if self.dirty && !was_armed {
                    self.quit_armed = true;
                    self.notify("Unsaved backup selection: w saves, q again quits without saving");
                } else {
                    self.quit = true;
                }
                return;
            }
            KeyCode::Char(c @ '1'..='6') => {
                let i = usize::from(c as u8 - b'1');
                if let Some(s) = Section::ALL.get(i) {
                    self.switch(*s);
                }
                return;
            }
            KeyCode::Tab | KeyCode::BackTab => {
                let i = Section::ALL
                    .iter()
                    .position(|s| *s == self.section)
                    .unwrap_or(0);
                let n = Section::ALL.len();
                let next = if key.code == KeyCode::Tab {
                    (i + 1) % n
                } else {
                    (i + n - 1) % n
                };
                if let Some(s) = Section::ALL.get(next) {
                    self.switch(*s);
                }
                return;
            }
            _ => {}
        }
        match self.section {
            Section::Pulse => self.pulse_key(key),
            Section::Timeline => self.timeline_key(key),
            Section::Audit => self.audit_key(key),
            Section::Backup => self.backup_key(key),
            Section::Migrate => self.migrate_key(key),
            Section::Settings => self.settings_key(key),
        }
    }

    /// Typing into the `/` filter.
    fn search_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Enter => self.searching = false,
            KeyCode::Esc => {
                self.searching = false;
                self.search.clear();
            }
            KeyCode::Backspace => {
                self.search.pop();
            }
            KeyCode::Char(c) => self.search.push(c),
            _ => {}
        }
        self.snap_sel = 0;
        self.audit_sel = 0;
    }

    fn switch(&mut self, s: Section) {
        if s != self.section {
            self.search.clear();
        }
        self.section = s;
        if s == Section::Backup && self.choices.is_none() {
            self.load_choices();
        }
        if s == Section::Pulse && self.findings.is_none() {
            self.load_doctor();
        }
    }

    pub fn start(&mut self) {
        if self.app.repo.exists() {
            self.load_doctor();
        }
    }

    /// The one primary action Pulse offers, chosen by state.
    pub fn primary(&self) -> (&'static str, String) {
        if !self.app.repo.exists() {
            return ("i", "Set up snapshots".into());
        }
        let risks = self.audit.as_ref().map_or(0, |r| r.counts().fail);
        if risks > 0 {
            return (
                "3",
                format!(
                    "Review {risks} audit risk{}",
                    if risks == 1 { "" } else { "s" }
                ),
            );
        }
        if self.migration.audited && self.migration.snapshot.is_empty() {
            return ("5", "Continue migration".into());
        }
        ("s", "Take snapshot".into())
    }

    fn pulse_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Enter | KeyCode::Char(' ') => match self.primary().0 {
                "3" => self.switch(Section::Audit),
                "5" => self.switch(Section::Migrate),
                "i" => self.suspend = Some(Suspend::Cli(vec!["init".into()])),
                _ => self.take_snapshot(),
            },
            KeyCode::Char('s') => self.take_snapshot(),
            KeyCode::Char('d' | 'r') => self.load_doctor(),
            _ => {}
        }
    }

    fn take_snapshot(&mut self) {
        if !self.app.repo.exists() {
            self.notify("Not set up yet: press enter on Pulse, or run omaphoenix init");
            return;
        }
        self.section = Section::Timeline;
        self.run_captured("Snapshot", &["snapshot"]);
    }

    /// Snapshots matching `/`.
    pub fn snap_view(&self) -> Vec<&Snapshot> {
        let q = self.search.to_lowercase();
        self.snaps
            .iter()
            .filter(|s| {
                q.is_empty()
                    || format!("{} {}", s.id, s.subject)
                        .to_lowercase()
                        .contains(&q)
            })
            .collect()
    }

    fn ask_undo(&mut self) {
        if !self.app.repo.exists() {
            self.notify("Nothing to undo: not set up yet");
            return;
        }
        self.confirm = Some((
            "Undo the last restore? Files come back; installed packages are listed, not removed. [y/N]".into(),
            Suspend::Cli(vec!["undo".into()]),
        ));
    }

    fn timeline_key(&mut self, key: KeyEvent) {
        let n = self.snap_view().len();
        match key.code {
            KeyCode::Down | KeyCode::Char('j') => {
                self.snap_sel = (self.snap_sel + 1).min(n.saturating_sub(1));
            }
            KeyCode::Up | KeyCode::Char('k') => self.snap_sel = self.snap_sel.saturating_sub(1),
            KeyCode::Char('s') => self.take_snapshot(),
            KeyCode::Enter | KeyCode::Char('p') => {
                if let Some(s) = self.snap_view().get(self.snap_sel).copied() {
                    let id = s.id.clone();
                    self.run_captured(
                        &format!("Preview of {id}"),
                        &["restore", "--dry-run", "--at", &id],
                    );
                }
            }
            KeyCode::Char('r') => {
                if let Some(id) = self.snap_view().get(self.snap_sel).map(|s| s.id.clone()) {
                    self.confirm = Some((
                        format!(
                            "Restore snapshot {id} onto this machine? Replaced files are kept as .pre-restore. [y/N]"
                        ),
                        Suspend::Cli(vec!["restore".into(), "--at".into(), id]),
                    ));
                }
            }
            KeyCode::Esc => self.output.clear(),
            _ => {}
        }
    }

    pub fn audit_items(&self) -> Vec<&secaudit_core::store::Item> {
        let Some(run) = &self.audit else {
            return Vec::new();
        };
        let q = self.search.to_lowercase();
        let mut items: Vec<_> = run
            .items
            .iter()
            .filter(|i| i.is_issue())
            .filter(|i| {
                q.is_empty()
                    || format!(
                        "{} {} {}",
                        i.finding.msg, i.finding.section, i.finding.detail
                    )
                    .to_lowercase()
                    .contains(&q)
            })
            .collect();
        items.sort_by_key(|i| {
            (
                i.accepted,
                secaudit_core::store::sev_order(i.finding.sev),
                i.n,
            )
        });
        items
    }

    fn audit_key(&mut self, key: KeyEvent) {
        let n = self.audit_items().len();
        match key.code {
            // DESIGN.md §7.1: s scans, R rescans; r means restore elsewhere.
            KeyCode::Char('s' | 'R') => self.run_audit(),
            KeyCode::Down | KeyCode::Char('j') => {
                self.audit_sel = (self.audit_sel + 1).min(n.saturating_sub(1));
            }
            KeyCode::Up | KeyCode::Char('k') => self.audit_sel = self.audit_sel.saturating_sub(1),
            KeyCode::Char('a') => self.toggle_accept(),
            KeyCode::Char('x') => {
                let fix = self
                    .audit_items()
                    .get(self.audit_sel)
                    .map(|i| i.finding.fix.clone())
                    .filter(|f| !f.is_empty());
                match fix {
                    // Fixes always confirm (DESIGN.md §7.1, AGENTS.md).
                    Some(fix) => {
                        self.confirm = Some((
                            format!("Run this fix in the terminal?  $ {fix}  [y/N]"),
                            Suspend::Fix(fix),
                        ));
                    }
                    None => self.notify("No automatic fix for this finding; see its advice"),
                }
            }
            KeyCode::Char('o') => {
                self.suspend = Some(Suspend::SecauditTui);
            }
            _ => {}
        }
    }

    fn toggle_accept(&mut self) {
        let Some(item) = self.audit_items().get(self.audit_sel).copied() else {
            return;
        };
        let key = item.accept_key();
        let (title, section, was) = (
            item.title().to_owned(),
            item.finding.section.clone(),
            item.accepted,
        );
        if was {
            self.accepted.remove(&key);
        } else {
            let at = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |d| d.as_secs());
            self.accepted.insert(
                key,
                Accepted {
                    section,
                    msg: title.clone(),
                    at,
                },
            );
        }
        if let Err(e) = self.audit_store.save_accepted(&self.accepted) {
            self.notify(format!("Couldn't save: {e}"));
            return;
        }
        if let Some(run) = &mut self.audit {
            run.recount(&self.accepted);
        }
        self.notify(if was {
            format!("Back to risks: {title}")
        } else {
            format!("Accepted, no longer scored: {title}")
        });
    }

    fn backup_key(&mut self, key: KeyEvent) {
        let Some(choices) = &mut self.choices else {
            if key.code == KeyCode::Char('r') {
                self.load_choices();
            }
            return;
        };
        if key.code == KeyCode::Char('w') {
            let plain: Vec<Choice> = choices.iter().map(|(c, _)| c.clone()).collect();
            match self.app.save_choices(&plain) {
                Ok(()) => {
                    self.dirty = false;
                    self.notify("✓ saved; the next snapshot captures exactly this");
                }
                Err(e) => self.notify(format!("save failed: {e}")),
            }
            return;
        }
        let n_cats = choices.len();
        let Some((cat, _)) = choices.get_mut(self.cat_sel) else {
            return;
        };
        let n_items = cat.items.len();
        match key.code {
            KeyCode::Up | KeyCode::Char('k') => {
                if self.in_items {
                    self.item_sel = self.item_sel.saturating_sub(1);
                } else {
                    self.cat_sel = self.cat_sel.saturating_sub(1);
                }
            }
            KeyCode::Down | KeyCode::Char('j') => {
                if self.in_items {
                    self.item_sel = (self.item_sel + 1).min(n_items.saturating_sub(1));
                } else {
                    self.cat_sel = (self.cat_sel + 1).min(n_cats.saturating_sub(1));
                }
            }
            KeyCode::Enter | KeyCode::Right | KeyCode::Char('l')
                if !self.in_items && n_items > 0 =>
            {
                self.in_items = true;
                self.item_sel = 0;
            }
            KeyCode::Esc | KeyCode::Left | KeyCode::Char('h') | KeyCode::Backspace => {
                self.in_items = false;
            }
            KeyCode::Char(' ') => {
                if self.in_items {
                    if let Some((_, on)) = cat.items.get_mut(self.item_sel) {
                        *on = !*on;
                    }
                } else {
                    let all = cat.items.iter().all(|(_, on)| *on);
                    cat.items.iter_mut().for_each(|(_, on)| *on = !all);
                }
                self.dirty = true;
            }
            KeyCode::Char('a') if self.in_items => {
                let all = cat.items.iter().all(|(_, on)| *on);
                cat.items.iter_mut().for_each(|(_, on)| *on = !all);
                self.dirty = true;
            }
            _ => {}
        }
    }

    /// Steps done so far, from the wizard's saved state.
    pub fn migrate_done(&self) -> [bool; 5] {
        self.migration.done()
    }

    fn migrate_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Down | KeyCode::Char('j') => {
                self.migrate_sel = (self.migrate_sel + 1).min(MIGRATE_STEPS.len() - 1);
            }
            KeyCode::Up | KeyCode::Char('k') => {
                self.migrate_sel = self.migrate_sel.saturating_sub(1);
            }
            KeyCode::Enter | KeyCode::Char(' ') => match self.migrate_sel {
                0 => {
                    self.section = Section::Audit;
                    self.run_audit();
                }
                4 => self.run_captured("Re-audit", &["reaudit"]),
                1 => {
                    self.migration.cleaned = true;
                    self.save_migration();
                    self.section = Section::Audit;
                }
                2 => self.take_snapshot(),
                _ => {
                    self.migration.restored_note_seen = true;
                    self.save_migration();
                }
            },
            KeyCode::Char('R') => {
                self.migration = Migration::default();
                self.save_migration();
                self.notify("Migration progress cleared");
            }
            _ => {}
        }
    }

    fn settings_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Char('t') => {
                let off = self.timer.starts_with("timer off");
                let r = if off {
                    schedule::on("30m")
                } else {
                    schedule::off()
                };
                if let Err(e) = r {
                    self.notify(e);
                }
                self.timer = schedule::status();
            }
            KeyCode::Char('e') => {
                self.suspend = Some(Suspend::Cli(vec!["secrets".into(), "init".into()]));
            }
            KeyCode::Char('i') if !self.app.repo.exists() => {
                self.suspend = Some(Suspend::Cli(vec!["init".into()]));
            }
            _ => {}
        }
    }
}

/// Command palette entries: every action by name (DESIGN.md §7.1).
pub const ACTIONS: [&str; 16] = [
    "Take snapshot",
    "Scan (quick audit)",
    "Preview selected snapshot",
    "Restore selected snapshot",
    "Undo last restore",
    "Save backup selection",
    "Turn automatic snapshots on/off",
    "Set up encrypted secrets",
    "Open the Security Audit app",
    "Go to Pulse",
    "Go to Timeline",
    "Go to Audit",
    "Go to Backup",
    "Go to Migrate",
    "Go to Settings",
    "Keys",
];

impl State<'_> {
    /// Actions matching the palette query (subsequence, any case).
    pub fn actions(q: &str) -> Vec<&'static str> {
        let q = q.to_lowercase();
        ACTIONS
            .iter()
            .copied()
            .filter(|a| {
                let mut it = a.to_lowercase().chars().collect::<Vec<_>>().into_iter();
                q.chars().all(|c| it.any(|x| x == c))
            })
            .collect()
    }

    fn palette_key(&mut self, mut q: String, mut sel: usize, key: KeyEvent) {
        let n = Self::actions(&q).len();
        match key.code {
            KeyCode::Esc => return,
            KeyCode::Down => sel = (sel + 1).min(n.saturating_sub(1)),
            KeyCode::Up => sel = sel.saturating_sub(1),
            KeyCode::Backspace => {
                q.pop();
                sel = 0;
            }
            KeyCode::Enter => {
                if let Some(a) = Self::actions(&q).get(sel).copied() {
                    self.act(a);
                }
                return;
            }
            KeyCode::Char(c) => {
                q.push(c);
                sel = 0;
            }
            _ => {}
        }
        self.palette = Some((q, sel));
    }

    /// Does a palette action through the same paths as its key.
    fn act(&mut self, a: &str) {
        let press = |st: &mut Self, s: Section, c: KeyCode| {
            st.switch(s);
            st.on_key(KeyEvent::new(c, KeyModifiers::NONE));
        };
        match a {
            "Take snapshot" => self.take_snapshot(),
            "Scan (quick audit)" => press(self, Section::Audit, KeyCode::Char('s')),
            "Preview selected snapshot" => press(self, Section::Timeline, KeyCode::Enter),
            "Restore selected snapshot" => press(self, Section::Timeline, KeyCode::Char('r')),
            "Undo last restore" => self.ask_undo(),
            "Save backup selection" => press(self, Section::Backup, KeyCode::Char('w')),
            "Turn automatic snapshots on/off" => press(self, Section::Settings, KeyCode::Char('t')),
            "Set up encrypted secrets" => press(self, Section::Settings, KeyCode::Char('e')),
            "Open the Security Audit app" => self.suspend = Some(Suspend::SecauditTui),
            "Keys" => self.help = true,
            other => {
                if let Some(s) = Section::ALL
                    .iter()
                    .find(|s| other == format!("Go to {}", s.label()))
                {
                    self.switch(*s);
                }
            }
        }
    }
}

/// Runs the quick audit and stores it like a Security Audit scan.
fn audit_now(store: &Store) -> Result<PathBuf, String> {
    let ctx = secaudit_core::audit::Ctx::from_env();
    let host = std::fs::read_to_string("/proc/sys/kernel/hostname").unwrap_or_default();
    let kernel = std::fs::read_to_string("/proc/sys/kernel/osrelease").unwrap_or_default();
    store
        .record_quick_scan(
            &ctx,
            host.trim(),
            kernel.trim(),
            &phx_core::category::stamp(),
        )
        .map_err(|e| e.to_string())
}

impl State<'_> {
    /// What's running, for the footer.
    pub fn busy_label(&self) -> Option<String> {
        if self.jobs.is_empty() {
            return None;
        }
        let names: Vec<&str> = self
            .jobs
            .iter()
            .map(|j| match *j {
                "doctor" => "checking backups",
                "choices" => "detecting items",
                "audit" => "auditing",
                _ => "running",
            })
            .collect();
        Some(format!("⋯ {}", names.join(", ")))
    }
}
