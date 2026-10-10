//! `secaudit-ui`: a line-for-line Rust port of the gum/Bash `secaudit-ui`.

#![allow(
    clippy::many_single_char_names,
    clippy::similar_names,
    reason = "a line-for-line port of the Bash script: f, w, p, s, g as there"
)]

mod dashboard;
mod findings;
mod format;
mod gum;
mod progress;
mod report;
mod runs;
mod scan;

use std::io::{self, Write};
use std::path::{Path, PathBuf};

use ratatui::crossterm::event::{self, Event, KeyEventKind};
use ratatui::crossterm::terminal;

use format::{B, CLEAR, N, print, width};
use gum::Env;

struct Ui {
    env: Env,
    engine: PathBuf,
    runs_dir: PathBuf,
    a: String,
    m: String,
    host: String,
    user: String,
}

fn hex_fg(h: &str, fallback: u8) -> String {
    gum::hex(h).map_or_else(
        || format!("\x1b[3{fallback}m"),
        |(r, g, b)| format!("\x1b[38;2;{r};{g};{b}m"),
    )
}

/// Runs `secaudit-ui` with its arguments; returns the exit code.
pub fn main(args: &[String]) -> u8 {
    let engine = std::env::var_os("SECAUDIT_ENGINE")
        .filter(|e| !e.is_empty())
        .map_or_else(
            || {
                std::env::current_exe()
                    .ok()
                    .and_then(|e| e.parent().map(|d| d.join("secaudit")))
                    .unwrap_or_default()
            },
            PathBuf::from,
        );
    if !is_executable(&engine) {
        let _ = writeln!(
            io::stderr(),
            "secaudit engine not found at {}",
            engine.display()
        );
        return 1;
    }
    let env = Env::load();
    let state = std::env::var_os("XDG_STATE_HOME")
        .filter(|s| !s.is_empty())
        .map_or_else(
            || PathBuf::from(std::env::var("HOME").unwrap_or_default()).join(".local/state"),
            PathBuf::from,
        )
        .join("secaudit");
    let a = hex_fg(env.get("BORDER_FOREGROUND").unwrap_or("#4b8ef4"), 4);
    let m = hex_fg(
        env.get("GUM_INPUT_PLACEHOLDER_FOREGROUND")
            .unwrap_or("#545454"),
        8,
    );
    let ui = Ui {
        env,
        engine,
        runs_dir: state.join("runs"),
        a,
        m,
        host: std::fs::read_to_string("/proc/sys/kernel/hostname")
            .unwrap_or_default()
            .trim()
            .to_owned(),
        user: std::env::var("USER").unwrap_or_default(),
    };
    let _ = std::fs::create_dir_all(&ui.runs_dir);
    match args.first().map(String::as_str) {
        Some("--scan") => {
            let mode = args.get(1).map_or("", String::as_str);
            if !matches!(mode, "quick" | "full" | "deep") {
                let _ = writeln!(io::stderr(), "Usage: secaudit-ui --scan quick|full|deep");
                return 2;
            }
            ui.scan(mode);
        }
        Some("--last") => {
            if let Some(last) = ui.runs_newest_first().first() {
                ui.dashboard(last);
            }
        }
        Some("-h" | "--help") => {
            print("Usage: secaudit-ui [--scan quick|full|deep] [--last]\n");
            return 0;
        }
        _ => {}
    }
    ui.home();
    print("\x1b[?25h");
    0
}

fn is_executable(p: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    std::fs::metadata(p).is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
}

impl Ui {
    fn title_bar(&self, left: &str, right: &str) {
        let w = width();
        let pad = w
            .saturating_sub(left.chars().count() + right.chars().count() + 2)
            .max(1);
        let (a, m) = (&self.a, &self.m);
        print(&format!(
            "\n  {B}{a}◆ {left}{N}{}{m}{right}{N}\n  {m}{}{N}\n",
            " ".repeat(pad),
            "─".repeat(w)
        ));
    }

    fn pause(&self) {
        print(&format!("\n  {}press any key to continue{N}", self.m));
        let _ = terminal::enable_raw_mode();
        loop {
            match event::read() {
                Ok(Event::Key(k)) if k.kind == KeyEventKind::Press => break,
                Err(_) => break,
                _ => {}
            }
        }
        let _ = terminal::disable_raw_mode();
    }

    fn choose(&self, header: &str, height: usize, items: &[(String, String)]) -> Option<String> {
        gum::choose(&self.env, header, height, items)
    }

    fn home(&self) {
        loop {
            print(CLEAR);
            self.title_bar("Security Audit", &format!("{} · {}", self.host, self.user));
            let m = &self.m;
            print(&format!(
                "   {m}Read-only checks for packages, boot, kernel, firewall, network, SSH,\n   accounts, permissions, persistence, containers, secrets and rootkits.{N}\n"
            ));
            let runs = self.runs_newest_first();
            let last = runs.first().cloned();
            match last.as_ref().and_then(|l| Self::load(l)) {
                Some(run) => print(&format!(
                    "\n   {m}Last scan{N}  {}\n",
                    self.run_summary_line(&run)
                )),
                None => print(&format!(
                    "\n   {m}No scans yet. Start with a full scan for complete coverage.{N}\n"
                )),
            }
            print("\n");
            let mut items = vec![
                (
                    "Full scan      sudo · complete coverage · ~20s".to_owned(),
                    "full".to_owned(),
                ),
                (
                    "Quick scan     no sudo · user-level checks · ~15s".to_owned(),
                    "quick".to_owned(),
                ),
                (
                    "Deep scan      sudo · + package integrity, whole-disk SUID · minutes"
                        .to_owned(),
                    "deep".to_owned(),
                ),
            ];
            if last.is_some() {
                items.push(("Last results".into(), "last".into()));
                items.push((
                    format!("History        {} scans", runs.len()),
                    "history".into(),
                ));
            }
            items.push(("Quit".into(), "quit".into()));
            let Some(choice) = self.choose("", 7, &items) else {
                return;
            };
            match choice.as_str() {
                "full" | "quick" | "deep" => self.scan(&choice),
                "last" => {
                    if let Some(l) = &last {
                        self.dashboard(l);
                    }
                }
                "history" => self.history_view(),
                _ => return,
            }
        }
    }
}
