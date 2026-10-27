//! The app's state types: tabs, focus, modals, screens and click regions.

use crate::tui::scan::Scan;
use phx_audit::store::{Item, sev_name};
use std::path::PathBuf;
use std::time::Instant;

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
    AiSetup(crate::tui::ai_screen::AiSetup),
}

pub enum Screen {
    Main,
    Scan(Box<Scan>),
    /// `HistoryScreen` with its cursor row.
    History(usize),
    /// `LogScreen`: lines, scroll offset, title date.
    Log(Vec<String>, usize, String),
    Ai(Box<crate::tui::ai_screen::AiView>),
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
