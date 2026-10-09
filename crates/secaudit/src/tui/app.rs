//! TUI state and key handling. Drawing is in `view`.

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;
use std::process::Command;
use std::time::{Duration, Instant};

use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use secaudit_core::Severity;
use secaudit_core::store::{Accepted, Item, Run, Store, now, sev_order};

use super::scan::Scan;
use super::theme::Theme;

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

    pub fn label(self) -> &'static str {
        match self {
            Self::Issues => "Issues",
            Self::Accepted => "Accepted",
            Self::Notes => "Notes",
            Self::Passed => "Passed",
            Self::All => "All",
        }
    }

    pub fn holds(self, item: &Item) -> bool {
        match self {
            Self::Issues => item.is_issue() && !item.accepted,
            Self::Accepted => item.accepted,
            Self::Notes => matches!(item.finding.sev, Severity::Info | Severity::Skip),
            Self::Passed => item.finding.sev == Severity::Pass,
            Self::All => true,
        }
    }
}

/// What a confirm screen does on yes.
pub enum Action {
    RunFixes(Vec<usize>),
    DeleteRun(PathBuf),
}

pub struct Confirm {
    pub title: String,
    pub code: Vec<String>,
    pub note: String,
    pub yes: String,
    pub action: Action,
}

pub enum Screen {
    Main,
    Chooser,
    Scan(Box<Scan>),
    Confirm(Confirm),
    History { sel: usize },
    Log { lines: Vec<String>, scroll: usize },
    Help,
}

/// Work that needs the plain terminal; the loop leaves the TUI screen for it.
pub enum Suspend {
    Sudo(String),
    Fixes(Vec<usize>),
}

pub struct App {
    pub store: Store,
    pub theme: Theme,
    pub runs: Vec<Run>,
    pub current: Option<usize>,
    pub accepted: BTreeMap<String, Accepted>,
    pub tab: Tab,
    pub section: Option<String>,
    pub sections_focus: bool,
    pub section_sel: usize,
    pub sel: usize,
    pub marked: BTreeSet<usize>,
    pub search: String,
    pub searching: bool,
    pub screen: Screen,
    pub toast: Option<(String, Instant)>,
    pub suspend: Option<Suspend>,
    pub quit: bool,
}

impl App {
    pub fn new() -> Self {
        let store = Store::from_env();
        store.prune_incomplete(600);
        let runs = store.runs();
        let current = (!runs.is_empty()).then_some(0);
        Self {
            accepted: store.accepted(),
            store,
            theme: Theme::load(),
            runs,
            current,
            tab: Tab::Issues,
            section: None,
            sections_focus: false,
            section_sel: 0,
            sel: 0,
            marked: BTreeSet::new(),
            search: String::new(),
            searching: false,
            screen: Screen::Main,
            toast: None,
            suspend: None,
            quit: false,
        }
    }

    pub fn run(&self) -> Option<&Run> {
        self.current.and_then(|i| self.runs.get(i))
    }

    pub fn notify(&mut self, msg: impl Into<String>) {
        self.toast = Some((msg.into(), Instant::now()));
    }

    /// Clears an old toast and follows a running scan; true if something changed.
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
        if let Screen::Scan(scan) = &mut self.screen {
            changed |= scan.poll(&self.store);
            if scan.finished {
                let dir = scan.dir.clone();
                self.scan_done(&dir);
                return true;
            }
        }
        changed
    }

    /// Sections of the current run, in report order.
    pub fn sections(&self) -> Vec<String> {
        let mut out: Vec<String> = Vec::new();
        for i in self.run().map(|r| r.items.as_slice()).unwrap_or_default() {
            if !out.contains(&i.finding.section) {
                out.push(i.finding.section.clone());
            }
        }
        out
    }

    /// Findings shown: tab, section filter and search applied, worst first.
    pub fn visible(&self) -> Vec<&Item> {
        let Some(run) = self.run() else {
            return Vec::new();
        };
        let q = self.search.to_lowercase();
        let mut items: Vec<&Item> = run
            .items
            .iter()
            .filter(|i| self.tab.holds(i))
            .filter(|i| {
                self.section
                    .as_ref()
                    .is_none_or(|s| &i.finding.section == s)
            })
            .filter(|i| {
                q.is_empty()
                    || [&i.finding.msg, &i.finding.detail, &i.finding.section]
                        .iter()
                        .any(|t| t.to_lowercase().contains(&q))
            })
            .collect();
        items.sort_by_key(|i| (sev_order(i.finding.sev), i.n));
        items
    }

    pub fn selected(&self) -> Option<&Item> {
        self.visible().get(self.sel).copied()
    }

    fn clamp(&mut self) {
        let n = self.visible().len();
        self.sel = self.sel.min(n.saturating_sub(1));
    }

    pub fn tab_count(&self, tab: Tab) -> usize {
        self.run()
            .map_or(0, |r| r.items.iter().filter(|i| tab.holds(i)).count())
    }

    pub fn on_key(&mut self, key: KeyEvent) {
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
            if let Screen::Scan(scan) = &mut self.screen {
                scan.cancel();
            }
            self.quit = true;
            return;
        }
        match std::mem::replace(&mut self.screen, Screen::Main) {
            Screen::Main => self.main_key(key),
            Screen::Chooser => self.chooser_key(key),
            Screen::Scan(scan) => self.scan_key(scan, key),
            Screen::Confirm(c) => self.confirm_key(c, key),
            Screen::History { sel } => self.history_key(sel, key),
            Screen::Log { lines, scroll } => self.log_key(lines, scroll, key),
            Screen::Help => {}
        }
    }

    fn main_key(&mut self, key: KeyEvent) {
        if self.searching {
            match key.code {
                KeyCode::Esc => {
                    self.searching = false;
                    self.search.clear();
                }
                KeyCode::Enter => self.searching = false,
                KeyCode::Backspace => {
                    self.search.pop();
                }
                KeyCode::Char(c) => self.search.push(c),
                _ => {}
            }
            self.sel = 0;
            return;
        }
        match key.code {
            KeyCode::Char('q') => self.quit = true,
            KeyCode::Char('?') => self.screen = Screen::Help,
            KeyCode::Char('s') => self.screen = Screen::Chooser,
            KeyCode::Char('r') => {
                let mode = self
                    .run()
                    .map_or("full".to_owned(), |r| r.meta.mode.clone());
                self.start_scan(&mode);
            }
            KeyCode::Char('h') => {
                self.screen = Screen::History {
                    sel: self.current.unwrap_or(0),
                }
            }
            KeyCode::Char('/') => self.searching = true,
            KeyCode::Char(c @ '1'..='5') => {
                let i = usize::from(c as u8 - b'1');
                if let Some(t) = Tab::ALL.get(i) {
                    self.tab = *t;
                    self.sel = 0;
                }
            }
            KeyCode::Tab => self.sections_focus = !self.sections_focus,
            KeyCode::Char('[') => self.cycle_section(false),
            KeyCode::Char(']') => self.cycle_section(true),
            KeyCode::Down | KeyCode::Char('j') => self.move_by(1),
            KeyCode::Up | KeyCode::Char('k') => self.move_by(-1),
            KeyCode::PageDown => self.move_by(10),
            KeyCode::PageUp => self.move_by(-10),
            KeyCode::Home | KeyCode::Char('g') => self.sel = 0,
            KeyCode::End | KeyCode::Char('G') => self.sel = self.visible().len().saturating_sub(1),
            KeyCode::Enter if self.sections_focus => {
                let names = self.sections();
                self.section = self
                    .section_sel
                    .checked_sub(1)
                    .and_then(|i| names.get(i).cloned());
                self.sections_focus = false;
                self.sel = 0;
            }
            KeyCode::Char(' ') => self.toggle_mark(),
            KeyCode::Char('*') => self.mark_all(),
            KeyCode::Char('x') => self.ask_fix(),
            KeyCode::Char('a') => self.toggle_accept(),
            KeyCode::Char('c') => self.copy_fix(),
            KeyCode::Char('o') => self.open_log(),
            KeyCode::Char('e') => self.export(),
            KeyCode::Esc => {
                self.marked.clear();
                self.search.clear();
                self.section = None;
            }
            _ => {}
        }
    }

    fn move_by(&mut self, d: isize) {
        if self.sections_focus {
            let n = self.sections().len() + 1;
            self.section_sel = self
                .section_sel
                .saturating_add_signed(d)
                .min(n.saturating_sub(1));
        } else {
            let n = self.visible().len();
            self.sel = self.sel.saturating_add_signed(d).min(n.saturating_sub(1));
        }
    }

    fn cycle_section(&mut self, forward: bool) {
        let names = self.sections();
        let pos = self
            .section
            .as_ref()
            .and_then(|s| names.iter().position(|n| n == s))
            .map_or(0, |p| p + 1);
        let n = names.len() + 1;
        let next = if forward {
            (pos + 1) % n
        } else {
            (pos + n - 1) % n
        };
        self.section = next.checked_sub(1).and_then(|i| names.get(i).cloned());
        self.section_sel = next;
        self.sel = 0;
    }

    fn toggle_mark(&mut self) {
        let Some(item) = self.selected() else { return };
        if item.finding.fix.is_empty() || item.accepted {
            self.notify("No automatic fix for this finding");
            return;
        }
        let n = item.n;
        if !self.marked.remove(&n) {
            self.marked.insert(n);
        }
        self.move_by(1);
    }

    fn mark_all(&mut self) {
        let fixable: Vec<usize> = self
            .visible()
            .iter()
            .filter(|i| i.is_issue() && !i.accepted && !i.finding.fix.is_empty())
            .map(|i| i.n)
            .collect();
        let count = fixable.len();
        self.marked.extend(fixable);
        self.notify(format!("{count} fixable issues selected"));
    }

    fn fix_targets(&self) -> Vec<&Item> {
        let Some(run) = self.run() else {
            return Vec::new();
        };
        if !self.marked.is_empty() {
            return run
                .items
                .iter()
                .filter(|i| self.marked.contains(&i.n))
                .collect();
        }
        self.selected()
            .filter(|i| !i.finding.fix.is_empty() && !i.accepted)
            .into_iter()
            .collect()
    }

    fn ask_fix(&mut self) {
        let targets = self.fix_targets();
        if targets.is_empty() {
            self.notify("No automatic fix for this finding. See How to address.");
            return;
        }
        let many = targets.len() > 1;
        let mut code = Vec::new();
        for (i, t) in targets.iter().enumerate() {
            if many {
                code.push(format!("{}. {}", i + 1, t.title()));
            }
            code.push(format!("$ {}", t.finding.fix));
            if many {
                code.push(String::new());
            }
        }
        let mut note = "Runs in this terminal. You'll see the output, then return here.".to_owned();
        if targets.iter().any(|t| t.finding.fix.contains("sudo")) {
            note.push_str(" sudo asks for your password once.");
        }
        let applied = self.run().map(Run::applied).unwrap_or_default();
        let again = targets.iter().filter(|t| applied.contains(&t.n)).count();
        if again > 0 {
            note = format!(
                "{again} of these {} already applied in this scan. {note}",
                if many { "were" } else { "was" }
            );
        }
        let title = if many {
            format!("Run {} fixes?", targets.len())
        } else {
            format!("Run this fix?  {}", targets[0].title())
        };
        let ns = targets.iter().map(|t| t.n).collect();
        let yes = if many {
            format!("Run {}", targets.len())
        } else {
            "Run".into()
        };
        self.screen = Screen::Confirm(Confirm {
            title,
            code,
            note,
            yes,
            action: Action::RunFixes(ns),
        });
    }

    /// Runs fixes in the plain terminal; called by the loop while suspended.
    pub fn run_fixes(&mut self, ns: &[usize]) {
        use std::io::Write;
        let Some(run) = self.run().cloned() else {
            return;
        };
        let targets: Vec<&Item> = run.items.iter().filter(|i| ns.contains(&i.n)).collect();
        let many = targets.len() > 1;
        let mut out = std::io::stdout();
        let mut ok = Vec::new();
        for (i, t) in targets.iter().enumerate() {
            let step = if many {
                format!("[{}/{}] ", i + 1, targets.len())
            } else {
                String::new()
            };
            let _ = writeln!(
                out,
                "\n  \x1b[1m{step}{}\x1b[0m\n  \x1b[2m$ {}\x1b[0m",
                t.title(),
                t.finding.fix
            );
            let status = Command::new("bash").arg("-c").arg(&t.finding.fix).status();
            if status.as_ref().is_ok_and(std::process::ExitStatus::success) {
                let _ = writeln!(out, "  \x1b[32m✓ done\x1b[0m");
                ok.push(t.n);
            } else {
                let code = status
                    .ok()
                    .and_then(|s| s.code())
                    .map_or("?".into(), |c| c.to_string());
                let _ = writeln!(out, "  \x1b[31m✗ exit status {code}\x1b[0m");
            }
        }
        let _ = write!(
            out,
            "\n  {} of {} succeeded.\n  Press Enter to return to Security Audit… ",
            ok.len(),
            targets.len()
        );
        let _ = out.flush();
        let mut line = String::new();
        let _ = std::io::stdin().read_line(&mut line);
        for n in &ok {
            let _ = run.mark_applied(*n);
            self.marked.remove(n);
        }
        let failed = targets.len() - ok.len();
        if failed > 0 {
            self.notify(format!(
                "{failed} fix{} failed; they stay selected.",
                if failed == 1 { "" } else { "es" }
            ));
        } else {
            self.notify(format!(
                "{} fix{} applied. Press r to rescan and confirm.",
                ok.len(),
                if many { "es" } else { "" }
            ));
        }
    }

    fn toggle_accept(&mut self) {
        let Some(item) = self.selected() else { return };
        if !item.is_issue() {
            return;
        }
        let (n, was_accepted, key) = (item.n, item.accepted, item.accept_key());
        let (title, section) = (item.title().to_owned(), item.finding.section.clone());
        let msg = if was_accepted {
            self.accepted.remove(&key);
            format!("Moved back to Issues: {title}")
        } else {
            self.marked.remove(&n);
            self.accepted.insert(
                key,
                Accepted {
                    section,
                    msg: title.clone(),
                    at: now(),
                },
            );
            format!("Accepted, no longer scored: {title}. Press a again to undo.")
        };
        if let Err(e) = self.store.save_accepted(&self.accepted) {
            self.notify(format!("Couldn't save accepted risks: {e}"));
            return;
        }
        for r in &mut self.runs {
            r.recount(&self.accepted);
        }
        self.clamp();
        self.notify(msg);
    }

    fn copy_fix(&mut self) {
        let Some(text) = self.selected().map(|i| {
            if i.finding.fix.is_empty() {
                i.finding.msg.clone()
            } else {
                i.finding.fix.clone()
            }
        }) else {
            return;
        };
        self.notify(if copy(&text) {
            "Copied to clipboard"
        } else {
            "No clipboard tool (wl-copy) found"
        });
    }

    fn open_log(&mut self) {
        let Some(run) = self.run() else { return };
        let text = std::fs::read_to_string(run.path.join("output.txt"))
            .unwrap_or_else(|_| "No output saved for this scan.".into());
        self.screen = Screen::Log {
            lines: text.lines().map(str::to_owned).collect(),
            scroll: 0,
        };
    }

    fn export(&mut self) {
        let Some(run) = self.run() else { return };
        let started = local_time(run.meta.started, "%Y-%m-%d %H:%M");
        match run.export_markdown(&started) {
            Ok(path) => {
                let shown = path.display().to_string();
                let copied = copy(&shown);
                let home = std::env::var("HOME").unwrap_or_default();
                let shown = if home.is_empty() {
                    shown
                } else {
                    shown.replacen(&home, "~", 1)
                };
                self.notify(format!(
                    "Report saved: {shown}{}",
                    if copied { " (path copied)" } else { "" }
                ));
            }
            Err(e) => self.notify(format!("Couldn't write the report: {e}")),
        }
    }

    fn chooser_key(&mut self, key: KeyEvent) {
        let mode = match key.code {
            KeyCode::Char('q') => "quick",
            KeyCode::Char('f') | KeyCode::Enter => "full",
            KeyCode::Char('d') => "deep",
            _ => return,
        };
        self.start_scan(mode);
    }

    pub fn start_scan(&mut self, mode: &str) {
        if mode == "quick" {
            self.launch(mode);
        } else {
            self.suspend = Some(Suspend::Sudo(mode.to_owned()));
        }
    }

    /// Starts the engine; for full and deep, after sudo was confirmed.
    pub fn launch(&mut self, mode: &str) {
        match Scan::start(&self.store, mode) {
            Ok(scan) => self.screen = Screen::Scan(Box::new(scan)),
            Err(e) => self.notify(format!("Couldn't start the scan: {e}")),
        }
    }

    fn scan_key(&mut self, mut scan: Box<Scan>, key: KeyEvent) {
        if key.code == KeyCode::Esc {
            if scan.failed.is_none() && !scan.finished {
                scan.cancel();
                self.notify("Scan cancelled");
            }
            return;
        }
        self.screen = Screen::Scan(scan);
    }

    fn scan_done(&mut self, dir: &std::path::Path) {
        self.screen = Screen::Main;
        self.runs = self.store.runs();
        self.current = self.runs.iter().position(|r| r.path == dir);
        self.section = None;
        self.sel = 0;
        self.marked.clear();
        if let Some(run) = self.run() {
            let c = run.counts();
            let body = format!(
                "Score {} ({}) · {} fail · {} warn",
                c.score(),
                c.grade(),
                c.fail,
                c.warn
            );
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
            self.notify(format!("Scan finished. {body}"));
        }
    }

    fn confirm_key(&mut self, c: Confirm, key: KeyEvent) {
        match key.code {
            KeyCode::Char('y') | KeyCode::Enter => match c.action {
                Action::RunFixes(ns) => self.suspend = Some(Suspend::Fixes(ns)),
                Action::DeleteRun(path) => {
                    let _ = std::fs::remove_dir_all(&path);
                    self.runs = self.store.runs();
                    self.current = (!self.runs.is_empty()).then_some(0);
                    self.screen = Screen::History { sel: 0 };
                    self.notify("Scan deleted");
                }
            },
            KeyCode::Char('n') | KeyCode::Esc => {
                if let Action::DeleteRun(_) = c.action {
                    self.screen = Screen::History {
                        sel: self.current.unwrap_or(0),
                    };
                }
            }
            _ => self.screen = Screen::Confirm(c),
        }
    }

    fn history_key(&mut self, mut sel: usize, key: KeyEvent) {
        let n = self.runs.len();
        match key.code {
            KeyCode::Esc | KeyCode::Char('q') => return,
            KeyCode::Down | KeyCode::Char('j') => sel = (sel + 1).min(n.saturating_sub(1)),
            KeyCode::Up | KeyCode::Char('k') => sel = sel.saturating_sub(1),
            KeyCode::Enter if n > 0 => {
                self.current = Some(sel);
                self.sel = 0;
                self.section = None;
                self.marked.clear();
                return;
            }
            KeyCode::Delete | KeyCode::Char('d') => {
                if let Some(run) = self.runs.get(sel) {
                    let when = local_time(run.meta.started, "%a %-d %b %H:%M");
                    self.screen = Screen::Confirm(Confirm {
                        title: format!("Delete the scan from {when}?"),
                        code: Vec::new(),
                        note: "Its findings, output and report are removed. Accepted risks stay."
                            .into(),
                        yes: "Delete".into(),
                        action: Action::DeleteRun(run.path.clone()),
                    });
                    return;
                }
            }
            _ => {}
        }
        self.screen = Screen::History { sel };
    }

    fn log_key(&mut self, lines: Vec<String>, mut scroll: usize, key: KeyEvent) {
        match key.code {
            KeyCode::Esc | KeyCode::Char('q') => return,
            KeyCode::Down | KeyCode::Char('j') => scroll += 1,
            KeyCode::Up | KeyCode::Char('k') => scroll = scroll.saturating_sub(1),
            KeyCode::PageDown | KeyCode::Char(' ') => scroll += 20,
            KeyCode::PageUp => scroll = scroll.saturating_sub(20),
            _ => {}
        }
        scroll = scroll.min(lines.len().saturating_sub(1));
        self.screen = Screen::Log { lines, scroll };
    }
}

/// Copies to the Wayland clipboard; false if `wl-copy` isn't there.
fn copy(text: &str) -> bool {
    use std::io::Write;
    Command::new("wl-copy")
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .ok()
        .and_then(|mut c| {
            c.stdin.take()?.write_all(text.as_bytes()).ok()?;
            c.wait().ok()
        })
        .is_some_and(|s| s.success())
}

/// The local UTC offset in seconds, asked of `date` once.
// ponytail: one offset per session; a DST change mid-session shifts times by an hour until restart.
fn utc_offset() -> i64 {
    static OFFSET: std::sync::OnceLock<i64> = std::sync::OnceLock::new();
    *OFFSET.get_or_init(|| {
        let z = Command::new("date")
            .arg("+%z")
            .output()
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_owned());
        let z = z.unwrap_or_default();
        let sign = if z.starts_with('-') { -1 } else { 1 };
        let digits = z.trim_start_matches(['+', '-']);
        let h: i64 = digits.get(..2).and_then(|v| v.parse().ok()).unwrap_or(0);
        let m: i64 = digits.get(2..4).and_then(|v| v.parse().ok()).unwrap_or(0);
        sign * (h * 3600 + m * 60)
    })
}

const WD: [&str; 7] = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];
const MON: [&str; 12] = [
    "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
];

/// Formats epoch seconds in local time. Supports `%Y %m %d %-d %H %M %a %b`.
pub fn local_time(epoch: u64, fmt: &str) -> String {
    let t = i64::try_from(epoch).unwrap_or(0) + utc_offset();
    let days = t.div_euclid(86_400);
    let secs = t.rem_euclid(86_400);
    let (y, mo, d) = civil(days);
    let wday = (days + 4).rem_euclid(7); // 1970-01-01 was a Thursday
    let idx = |v: i64| usize::try_from(v).unwrap_or(0);
    fmt.replace("%Y", &y.to_string())
        .replace("%m", &format!("{mo:02}"))
        .replace("%-d", &d.to_string())
        .replace("%d", &format!("{d:02}"))
        .replace("%H", &format!("{:02}", secs / 3600))
        .replace("%M", &format!("{:02}", secs % 3600 / 60))
        .replace("%a", WD.get(idx(wday)).unwrap_or(&""))
        .replace("%b", MON.get(idx(mo - 1)).unwrap_or(&""))
}

/// (year, month, day) from days since 1970-01-01 (Howard Hinnant's algorithm).
fn civil(days: i64) -> (i64, i64, i64) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    (yoe + era * 400 + i64::from(m <= 2), m, d)
}

/// "5m ago" style.
pub fn ago(epoch: u64) -> String {
    let s = now().saturating_sub(epoch);
    match s {
        0..60 => "just now".into(),
        60..3600 => format!("{}m ago", s / 60),
        3600..86_400 => format!("{}h ago", s / 3600),
        _ => format!("{}d ago", s / 86_400),
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn civil_dates() {
        assert_eq!(super::civil(0), (1970, 1, 1));
        assert_eq!(super::civil(20_371), (2025, 10, 10));
        assert_eq!(super::civil(11_016), (2000, 2, 29));
    }
}
