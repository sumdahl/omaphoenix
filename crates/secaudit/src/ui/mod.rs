//! `secaudit-ui`: the exact Rust port of the gum/Bash `secaudit-ui`, line
//! for line: the same screens, texts, colours (bash `printf` padding
//! included, which counts bytes) and menus, with `gum` built in.

#![allow(
    clippy::many_single_char_names,
    clippy::similar_names,
    reason = "a line-for-line port of the Bash script: f, w, p, s, g as there"
)]

mod gum;

use std::collections::BTreeSet;
use std::fmt::Write as _;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use ratatui::crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
use ratatui::crossterm::terminal;
use secaudit_core::store::{KEEP_RUNS, Run, now, sev_name};

use crate::tui::times::{ago, local_time};
use gum::Env;

const RED: &str = "\x1b[31m";
const YEL: &str = "\x1b[33m";
const GRN: &str = "\x1b[32m";
const BLU: &str = "\x1b[34m";
const B: &str = "\x1b[1m";
const I: &str = "\x1b[3m";
const N: &str = "\x1b[0m";
const EL: &str = "\x1b[K";
const CLEAR: &str = "\x1b[H\x1b[2J\x1b[3J";
const FRAMES: [&str; 10] = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];

fn icon(sev: &str) -> &'static str {
    match sev {
        "fail" => "✗",
        "warn" => "!",
        "pass" => "✓",
        "skip" => "–",
        _ => "i",
    }
}

fn label(sev: &str) -> &'static str {
    match sev {
        "fail" => "FAIL",
        "warn" => "WARN",
        "pass" => "PASS",
        "skip" => "SKIP",
        _ => "NOTE",
    }
}

/// bash `printf '%-Ns'`: pads to N bytes.
fn lpad(s: &str, w: usize) -> String {
    format!("{s}{}", " ".repeat(w.saturating_sub(s.len())))
}

/// bash `printf '%Ns'`.
fn rpad(s: &str, w: usize) -> String {
    format!("{}{s}", " ".repeat(w.saturating_sub(s.len())))
}

/// `trunc()`: at most `m` characters, the last one an ellipsis.
fn trunc(t: &str, m: usize) -> String {
    if t.chars().count() > m {
        let mut s: String = t.chars().take(m.saturating_sub(1)).collect();
        s.push('…');
        s
    } else {
        t.to_owned()
    }
}

fn mmss(s: u64) -> String {
    format!("{}:{:02}", s / 60, s % 60)
}

fn cols() -> usize {
    terminal::size().map_or(80, |(c, _)| usize::from(c))
}

fn lines() -> usize {
    terminal::size().map_or(24, |(_, r)| usize::from(r))
}

/// `width()`.
fn width() -> usize {
    let c = cols();
    if c > 96 { 92 } else { c.saturating_sub(4) }
}

fn print(s: &str) {
    let mut out = io::stdout();
    let _ = out.write_all(s.as_bytes());
    let _ = out.flush();
}

fn score_of(f: usize, w: usize) -> usize {
    100usize.saturating_sub(10 * f + 2 * w)
}

fn grade_of(s: usize) -> char {
    match s {
        90.. => 'A',
        80..=89 => 'B',
        70..=79 => 'C',
        60..=69 => 'D',
        _ => 'F',
    }
}

fn grade_color(g: char) -> &'static str {
    match g {
        'A' | 'B' => GRN,
        'C' | 'D' => YEL,
        _ => RED,
    }
}

/// Raw counts (fail, warn, pass, info, skip): this front end does not
/// know about accepted risks.
fn counts(run: &Run) -> [usize; 5] {
    let mut c = [0; 5];
    for i in &run.items {
        let k = match sev_name(i.finding.sev) {
            "fail" => 0,
            "warn" => 1,
            "pass" => 2,
            "info" => 3,
            _ => 4,
        };
        if let Some(v) = c.get_mut(k) {
            *v += 1;
        }
    }
    c
}

/// `issue_keys()`: "sev\tsection\tmsg" of fails and warnings.
fn issue_keys(run: &Run) -> BTreeSet<String> {
    run.items
        .iter()
        .filter(|i| matches!(sev_name(i.finding.sev), "fail" | "warn"))
        .map(|i| {
            format!(
                "{}\t{}\t{}",
                sev_name(i.finding.sev),
                i.finding.section,
                i.finding.msg
            )
        })
        .collect()
}

struct Ui {
    env: Env,
    engine: PathBuf,
    runs_dir: PathBuf,
    a: String,
    m: String,
    host: String,
    user: String,
}

/// `hex_fg()`: truecolor, or the given palette entry for a bad value.
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

    /// Completed and crashed runs with findings, newest first.
    fn runs_newest_first(&self) -> Vec<PathBuf> {
        let mut dirs: Vec<PathBuf> = std::fs::read_dir(&self.runs_dir)
            .map(|rd| {
                rd.flatten()
                    .map(|e| e.path())
                    .filter(|p| p.is_dir())
                    .collect()
            })
            .unwrap_or_default();
        dirs.sort();
        dirs.reverse();
        dirs.retain(|d| std::fs::metadata(d.join("findings.jsonl")).is_ok_and(|m| m.len() > 0));
        dirs
    }

    fn load(dir: &Path) -> Option<Run> {
        Run::load(dir, &std::collections::BTreeMap::new()).ok()
    }

    fn previous_run(&self, dir: &Path) -> Option<PathBuf> {
        let runs = self.runs_newest_first();
        let i = runs.iter().position(|r| r == dir)?;
        runs.get(i + 1).cloned()
    }

    /// `engine_sections()`: from the engine script, else the built-in list.
    fn engine_sections(&self) -> Vec<String> {
        let text = std::fs::read(&self.engine)
            .map(|b| String::from_utf8_lossy(&b).into_owned())
            .unwrap_or_default();
        let from_script: Vec<String> = text
            .lines()
            .filter_map(|l| {
                l.strip_prefix("section \"")
                    .and_then(|r| r.split('"').next())
            })
            .filter(|s| *s != "Summary")
            .map(str::to_owned)
            .collect();
        if from_script.is_empty() {
            secaudit_core::checks::SECTIONS
                .iter()
                .map(|s| s.name)
                .filter(|s| *s != "Summary")
                .map(str::to_owned)
                .collect()
        } else {
            from_script
        }
    }

    fn run_summary_line(&self, run: &Run) -> String {
        let [f, w, ..] = counts(run);
        let s = score_of(f, w);
        let g = grade_of(s);
        format!(
            "{}{B}{s} {g}{N}    {RED}{f} fail{N}  {YEL}{w} warn{N}  {}· {} scan · {}{N}",
            grade_color(g),
            self.m,
            run.meta.mode,
            ago(run.meta.started)
        )
    }

    // -- scanning -------------------------------------------------------------

    fn render_progress(&self, dir: &Path, mode: &str, frame: &str, elapsed: u64, done: bool) {
        let w = width();
        let secs = self.engine_sections();
        let total = secs.len().max(1);
        // section -> (warn, fail) from the live text log, in order seen.
        let out = std::fs::read_to_string(dir.join("output.txt")).unwrap_or_default();
        let mut order: Vec<(String, usize, usize)> = Vec::new();
        let mut cur: Option<usize> = None;
        for line in out.lines() {
            if let Some(sec) = line.strip_prefix("== ").and_then(|l| l.strip_suffix(" ==")) {
                cur = Some(order.iter().position(|o| o.0 == sec).unwrap_or_else(|| {
                    order.push((sec.to_owned(), 0, 0));
                    order.len() - 1
                }));
            }
            if let Some(o) = cur.and_then(|i| order.get_mut(i)) {
                if line.contains("[WARN]") {
                    o.1 += 1;
                }
                if line.contains("[FAIL]") {
                    o.2 += 1;
                }
            }
        }
        order.retain(|o| o.0 != "Summary");
        let current = order.last().map(|o| o.0.clone()).unwrap_or_default();
        let completed = if done {
            total
        } else {
            order.len().saturating_sub(1)
        };
        let who = if mode == "quick" {
            format!("as {}", self.user)
        } else {
            "as root".into()
        };
        let mut cap = mode.to_owned();
        if let Some(f) = cap.get_mut(0..1) {
            f.make_ascii_uppercase();
        }
        print("\x1b[H");
        self.title_bar(
            "Security Audit",
            &format!("{cap} scan · {who} · {}", mmss(elapsed)),
        );
        let mut s = format!("{EL}\n");
        let (a, m) = (&self.a, &self.m);
        for name in &secs {
            let seen = order.iter().find(|o| &o.0 == name);
            match seen {
                Some((_, sw, sf)) if *name != current || done => {
                    let mut tag = String::new();
                    let ic = if *sf > 0 {
                        tag = format!("{RED}{sf} fail{N}");
                        format!("{RED}✗{N}")
                    } else if *sw > 0 {
                        format!("{YEL}!{N}")
                    } else {
                        format!("{GRN}✓{N}")
                    };
                    if *sw > 0 {
                        let sep = if tag.is_empty() { "" } else { "  " };
                        let _ = write!(tag, "{sep}{YEL}{sw} warn{N}");
                    }
                    let _ = writeln!(s, "   {ic}  {} {tag}{EL}", lpad(name, 34));
                }
                _ if *name == current => {
                    let _ = writeln!(
                        s,
                        "   {a}{frame}{N}  {B}{}{N} {m}{I}checking…{N}{EL}",
                        lpad(name, 34)
                    );
                }
                _ => {
                    let _ = writeln!(s, "   {m}·  {name}{N}{EL}");
                }
            }
        }
        let bw = w.saturating_sub(14);
        let filled = bw * completed / total;
        let _ = write!(
            s,
            "{EL}\n   {a}{}{N}{m}{}{N}  {m}{} / {total}{N}{EL}\n",
            "━".repeat(filled.max(1)),
            "─".repeat(bw.saturating_sub(filled).max(1)),
            rpad(&completed.to_string(), 2)
        );
        if !done {
            let _ = write!(s, "\n   {m}ctrl+c to cancel{N}{EL}");
        }
        s.push_str("\x1b[J");
        print(&s.replace('\n', "\r\n"));
    }

    #[allow(clippy::too_many_lines, reason = "mirrors scan() in the script")]
    fn scan(&self, mode: &str) {
        print(CLEAR);
        let sudo = mode != "quick";
        let mut cap = mode.to_owned();
        if let Some(f) = cap.get_mut(0..1) {
            f.make_ascii_uppercase();
        }
        if sudo {
            self.title_bar("Security Audit", &format!("{cap} scan"));
            print(&format!(
                "\n  {cap} scan reads root-only files (shadow, sudoers, firewall rules, LUKS headers).\n"
            ));
            print(&format!(
                "  {}The audit is read-only; sudo is only used to look.{N}\n\n",
                self.m
            ));
            if !Command::new("sudo")
                .arg("-v")
                .status()
                .is_ok_and(|s| s.success())
            {
                print(&format!("\n  {RED}No sudo access, scan cancelled.{N}\n"));
                self.pause();
                return;
            }
            print(CLEAR);
        }
        let stamp = Command::new("date")
            .arg("+%Y%m%d-%H%M%S")
            .output()
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_owned())
            .unwrap_or_default();
        let dir = self.runs_dir.join(stamp);
        let started = now();
        let _ = std::fs::create_dir_all(&dir);
        let kernel = std::fs::read_to_string("/proc/sys/kernel/osrelease")
            .unwrap_or_default()
            .trim()
            .to_owned();
        let meta = serde_json::json!({"mode": mode, "started": started, "host": self.host, "user": self.user, "kernel": kernel});
        let _ = std::fs::write(dir.join("meta.json"), format!("{meta}\n"));
        let findings = dir.join("findings.jsonl");
        let Ok(out) = std::fs::File::create(dir.join("output.txt")) else {
            return;
        };
        let err = out.try_clone().ok();
        let mut cmd = if sudo {
            let mut c = Command::new("sudo");
            c.arg("-n").arg(&self.engine);
            c
        } else {
            Command::new(&self.engine)
        };
        if mode == "deep" {
            cmd.arg("--deep");
        }
        cmd.arg("--json")
            .arg(&findings)
            .stdin(Stdio::null())
            .stdout(out);
        if let Some(e) = err {
            cmd.stderr(e);
        }
        let Ok(mut child) = cmd.spawn() else { return };
        let t0 = Instant::now();
        let _ = terminal::enable_raw_mode();
        print("\x1b[?25l");
        print(CLEAR);
        let mut i = 0usize;
        let cancelled = loop {
            if child.try_wait().ok().flatten().is_some() {
                break false;
            }
            let ctrl_c = event::poll(Duration::from_millis(120)).unwrap_or(false)
                && matches!(event::read(), Ok(Event::Key(k)) if k.code == KeyCode::Char('c') && k.modifiers.contains(KeyModifiers::CONTROL));
            if ctrl_c {
                // The engine may run as root; kill its whole process tree.
                let pattern = format!("--json {}", findings.display());
                let mut pk = if sudo {
                    Command::new("sudo")
                } else {
                    Command::new("pkill")
                };
                if sudo {
                    pk.args(["-n", "pkill"]);
                }
                let _ = pk
                    .args(["-TERM", "-f", "--", &pattern])
                    .stderr(Stdio::null())
                    .status();
                let _ = child.kill();
                let _ = child.wait();
                let _ = terminal::disable_raw_mode();
                print("\x1b[?25h");
                let _ = std::fs::remove_dir_all(&dir);
                print(&format!("\n\n   {YEL}Scan cancelled.{N}\n"));
                std::thread::sleep(Duration::from_secs(1));
                break true;
            }
            let frame = FRAMES.get(i % FRAMES.len()).copied().unwrap_or("⠋");
            i += 1;
            self.render_progress(&dir, mode, frame, t0.elapsed().as_secs(), false);
        };
        if cancelled {
            return;
        }
        let _ = child.wait();
        let finished = now();
        let mut meta = meta;
        meta["finished"] = finished.into();
        let _ = std::fs::write(dir.join("meta.json"), format!("{meta}\n"));
        self.render_progress(&dir, mode, "", finished.saturating_sub(started), true);
        let _ = terminal::disable_raw_mode();
        print("\x1b[?25h");
        if std::fs::metadata(&findings).map_or(0, |m| m.len()) == 0 {
            print(&format!(
                "\n\n   {RED}The scan produced no results. Last lines of output:{N}\n\n"
            ));
            let out = std::fs::read_to_string(dir.join("output.txt")).unwrap_or_default();
            let l: Vec<&str> = out.lines().collect();
            for line in l.get(l.len().saturating_sub(5)..).unwrap_or_default() {
                print(&format!("     {line}\n"));
            }
            let _ = std::fs::remove_dir_all(&dir);
            self.pause();
            return;
        }
        for old in self.runs_newest_first().iter().skip(KEEP_RUNS) {
            let _ = std::fs::remove_dir_all(old);
        }
        if let Some(run) = Self::load(&dir) {
            let [f, w, ..] = counts(&run);
            let s = score_of(f, w);
            let _ = Command::new("notify-send")
                .args([
                    "-a",
                    "secaudit",
                    "-i",
                    "security-high-symbolic",
                    "Security audit finished",
                ])
                .arg(format!("Score {s} ({}) · {f} fail · {w} warn", grade_of(s)))
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn();
        }
        std::thread::sleep(Duration::from_millis(600));
        self.dashboard(&dir);
    }

    // -- results --------------------------------------------------------------

    fn render_dashboard(&self, dir: &Path, run: &Run) {
        let w = width();
        let [f, wn, p, inf, sk] = counts(run);
        let s = score_of(f, wn);
        let g = grade_of(s);
        let gc = grade_color(g);
        let m = &self.m;
        print(CLEAR);
        let mode = &run.meta.mode;
        self.title_bar(
            "Security Audit",
            &format!(
                "{} · {mode} scan",
                local_time(run.meta.started, "%a %-d %b %H:%M")
            ),
        );
        let bw = w.saturating_sub(22);
        let filled = bw * s / 100;
        let mut o = format!("\n   {B}{gc}{}{N} {B}{gc}{g}{N}  ", rpad(&s.to_string(), 3));
        let _ = write!(o, "{gc}{}{N}", "━".repeat(filled.max(1)));
        let _ = writeln!(
            o,
            "{m}{}{N}   {m}/100{N}",
            "─".repeat(bw.saturating_sub(filled).max(1))
        );
        let _ = writeln!(
            o,
            "   {RED}✗ {f} fail{N}   {YEL}! {wn} warn{N}   {GRN}✓ {p} pass{N}   {m}i {inf} notes   – {sk} skipped{N}"
        );
        let mut extra = String::new();
        if let Some(fin) = run.meta.finished {
            extra = format!("took {}", mmss(fin.saturating_sub(run.meta.started)));
        }
        if let Some(prev) = self.previous_run(dir).and_then(|p| Self::load(&p)) {
            let (a, b) = (issue_keys(&prev), issue_keys(run));
            let (new, resolved) = (b.difference(&a).count(), a.difference(&b).count());
            if !extra.is_empty() {
                extra.push_str(" · ");
            }
            extra.push_str("vs previous: ");
            if new > 0 {
                let _ = write!(extra, "{RED}{new} new{m}");
            } else {
                extra.push_str("0 new");
            }
            extra.push_str(", ");
            if resolved > 0 {
                let _ = write!(extra, "{GRN}{resolved} resolved{m}");
            } else {
                extra.push_str("0 resolved");
            }
        }
        if mode == "quick" {
            if !extra.is_empty() {
                extra.push_str(" · ");
            }
            extra.push_str("some checks need a full scan");
        }
        let _ = write!(o, "   {m}{extra}{N}\n\n");
        let _ = writeln!(
            o,
            "   {m}{} {} {} {}{N}",
            lpad("SECTION", 36),
            rpad("PASS", 5),
            rpad("WARN", 5),
            rpad("FAIL", 5)
        );
        for sec in self.engine_sections() {
            let of = |sev: &str| {
                run.items
                    .iter()
                    .filter(|i| i.finding.section == sec && sev_name(i.finding.sev) == sev)
                    .count()
            };
            if !run.items.iter().any(|i| i.finding.section == sec) {
                continue;
            }
            let (sp, sw, sf) = (of("pass"), of("warn"), of("fail"));
            let ic = if sf > 0 {
                format!("{RED}✗{N}")
            } else if sw > 0 {
                format!("{YEL}!{N}")
            } else {
                format!("{GRN}✓{N}")
            };
            let _ = writeln!(
                o,
                "   {ic} {} {} {}{}{N} {}{}{N}",
                lpad(&trunc(&sec, 34), 34),
                rpad(&sp.to_string(), 5),
                if sw > 0 { YEL } else { m },
                rpad(&sw.to_string(), 5),
                if sf > 0 { RED } else { m },
                rpad(&sf.to_string(), 5)
            );
        }
        o.push('\n');
        print(&o);
    }

    fn dashboard(&self, dir: &Path) {
        loop {
            let Some(run) = Self::load(dir) else { return };
            self.render_dashboard(dir, &run);
            let [f, wn, p, inf, _] = counts(&run);
            let prev = self.previous_run(dir);
            let mut items: Vec<(String, String)> = Vec::new();
            let it = |l: String, v: &str| (l, v.to_owned());
            if f + wn > 0 {
                items.push(it(format!("Fix issues  ({})", f + wn), "issues"));
            }
            if inf > 0 {
                items.push(it(format!("Notes  ({inf})"), "notes"));
            }
            items.push(it(format!("Passed checks  ({p})"), "passed"));
            if prev.is_some() {
                items.push(it("Changes since previous scan".into(), "diff"));
            }
            for (l, v) in [
                ("Full output", "log"),
                ("Export Markdown report", "export"),
                ("Scan again", "rescan"),
                ("Back", "back"),
            ] {
                items.push(it(l.into(), v));
            }
            let Some(choice) = self.choose("", 9, &items) else {
                return;
            };
            match choice.as_str() {
                "issues" => self.browse(dir, &["fail", "warn"]),
                "notes" => self.browse(dir, &["info", "skip"]),
                "passed" => self.browse(dir, &["pass"]),
                "diff" => {
                    if let Some(p) = &prev {
                        self.show_diff(p, dir);
                    }
                }
                "log" => {
                    let _ = Command::new("less")
                        .arg("-R")
                        .arg("+g")
                        .arg(dir.join("output.txt"))
                        .status();
                }
                "export" => self.export_md(&run),
                "rescan" => {
                    self.scan(&run.meta.mode);
                    return;
                }
                _ => return,
            }
        }
    }

    fn browse(&self, dir: &Path, sevs: &[&str]) {
        let w = width();
        loop {
            let Some(run) = Self::load(dir) else { return };
            print(CLEAR);
            let header = match sevs.first() {
                Some(&"fail") => "Issues · fail first · enter for details and fix",
                Some(&"info") => "Notes · context worth knowing, not problems",
                _ => "Passed checks",
            };
            self.title_bar("Security Audit", &format!("{} scan", run.meta.mode));
            print("\n");
            let applied = run.applied();
            let mut rows: Vec<_> = run
                .items
                .iter()
                .filter(|i| sevs.contains(&sev_name(i.finding.sev)))
                .collect();
            rows.sort_by_key(|i| (secaudit_core::store::sev_order(i.finding.sev), i.n));
            let m = &self.m;
            let mut items: Vec<(String, String)> = rows
                .iter()
                .map(|i| {
                    let sev = sev_name(i.finding.sev);
                    let mark = if applied.contains(&i.n) {
                        format!("{GRN}✓ {N}")
                    } else {
                        "  ".into()
                    };
                    let msg = i.finding.msg.strip_suffix(':').unwrap_or(&i.finding.msg);
                    let l = format!(
                        "{}{} {}{N}  {m}{}{N} {mark}{}",
                        sev_color(sev, m),
                        icon(sev),
                        lpad(label(sev), 4),
                        lpad(&trunc(&i.finding.section, 22), 22),
                        trunc(msg, w.saturating_sub(40))
                    );
                    (l, i.n.to_string())
                })
                .collect();
            items.push((format!("{m}← Back{N}"), "back".into()));
            let Some(choice) = self.choose(header, lines().saturating_sub(8), &items) else {
                return;
            };
            if choice == "back" {
                return;
            }
            if let Ok(n) = choice.parse() {
                self.detail_view(dir, n);
            }
        }
    }

    fn detail_view(&self, dir: &Path, n: usize) {
        let Some(run) = Self::load(dir) else { return };
        let Some(item) = run.items.iter().find(|i| i.n == n) else {
            return;
        };
        let fd = &item.finding;
        let sev = sev_name(fd.sev);
        let w = width();
        let (a, m) = (&self.a, &self.m);
        loop {
            print(CLEAR);
            self.title_bar("Security Audit", &fd.section);
            let msg = fd.msg.strip_suffix(':').unwrap_or(&fd.msg);
            let mut o = format!(
                "\n   {B}{}{} {}{N}  {B}{msg}{N}\n",
                sev_color(sev, m),
                icon(sev),
                label(sev)
            );
            if !fd.detail.is_empty() {
                let max = lines().saturating_sub(18).max(4);
                let dl: Vec<&str> = fd.detail.lines().collect();
                let _ = writeln!(o, "\n   {m}Details{N}");
                for l in dl.iter().take(max) {
                    let _ = writeln!(o, "     {}", trunc(l, w.saturating_sub(6)));
                }
                if dl.len() > max {
                    let _ = writeln!(o, "     {m}… {} more (see Full output){N}", dl.len() - max);
                }
            }
            if !fd.fix.is_empty() {
                let _ = writeln!(o, "\n   {m}Suggested fix{N}");
                if fd.fix.chars().count() > w.saturating_sub(6) {
                    let _ = writeln!(o, "     {a}{}{N}", trunc(&fd.fix, w.saturating_sub(6)));
                    let _ = writeln!(
                        o,
                        "     {m}({} chars; copy to see it all){N}",
                        fd.fix.chars().count()
                    );
                } else {
                    let _ = writeln!(o, "     {a}{}{N}", fd.fix);
                }
            }
            if !fd.hint.is_empty() {
                let _ = writeln!(o, "\n   {m}How to address{N}\n     {I}{}{N}", fd.hint);
            }
            if run.applied().contains(&n) {
                let _ = writeln!(
                    o,
                    "\n   {GRN}✓ Fix applied this session; scan again to verify{N}"
                );
            }
            o.push('\n');
            print(&o);
            let mut items = Vec::new();
            if !fd.fix.is_empty() {
                items.push(("Run fix".to_owned(), "run".to_owned()));
                items.push(("Copy fix to clipboard".to_owned(), "copy".to_owned()));
            }
            if fd.fix.is_empty() && !fd.hint.is_empty() {
                items.push(("Copy advice to clipboard".to_owned(), "copyhint".to_owned()));
            }
            items.push(("← Back".to_owned(), "back".to_owned()));
            let Some(choice) = self.choose("", 4, &items) else {
                return;
            };
            match choice.as_str() {
                "run" => self.run_fix(&run, n, &fd.fix),
                "copy" => self.copy_text(&fd.fix),
                "copyhint" => self.copy_text(&fd.hint),
                _ => return,
            }
        }
    }

    fn copy_text(&self, text: &str) {
        let copied = Command::new("wl-copy")
            .stdin(Stdio::piped())
            .spawn()
            .ok()
            .and_then(|mut c| {
                c.stdin.take()?.write_all(text.as_bytes()).ok()?;
                c.wait().ok()
            });
        match copied {
            Some(s) if s.success() => {
                // gum style --foreground 2
                print(&format!("{}   ✓ Copied\x1b[0m\n", self.env.fg("2")));
                std::thread::sleep(Duration::from_millis(700));
            }
            Some(_) => {}
            None => {
                print(&format!("\n   {RED}wl-copy not available{N}\n"));
                self.pause();
            }
        }
    }

    fn run_fix(&self, run: &Run, n: usize, cmd: &str) {
        print(CLEAR);
        self.title_bar("Security Audit", "Run fix");
        print("\n   This will run:\n\n");
        let fw = width().saturating_sub(6).max(1);
        let chars: Vec<char> = cmd.chars().collect();
        for line in cmd.lines() {
            let lc: Vec<char> = line.chars().collect();
            for chunk in lc.chunks(fw) {
                print(&format!(
                    "     {}{}{N}\n",
                    self.a,
                    chunk.iter().collect::<String>()
                ));
            }
        }
        if chars.is_empty() {
            print(&format!("     {}{N}\n", self.a));
        }
        print("\n");
        if !gum::confirm(&self.env, "Run this command now?", "Run", "Cancel") {
            return;
        }
        print("\n");
        let rc = Command::new("bash")
            .arg("-c")
            .arg(cmd)
            .status()
            .ok()
            .and_then(|s| s.code())
            .unwrap_or(1);
        if rc == 0 {
            let _ = run.mark_applied(n);
            print(&format!(
                "\n   {GRN}✓ Done.{N} {}Run a new scan to confirm the finding is gone.{N}\n",
                self.m
            ));
        } else {
            print(&format!("\n   {RED}✗ Command exited with status {rc}{N}\n"));
        }
        self.pause();
    }

    fn show_diff(&self, prev: &Path, cur: &Path) {
        let (Some(p), Some(c)) = (Self::load(prev), Self::load(cur)) else {
            return;
        };
        let (a, b) = (issue_keys(&p), issue_keys(&c));
        let m = &self.m;
        print(CLEAR);
        self.title_bar(
            "Security Audit",
            &format!(
                "vs scan from {} ({})",
                local_time(p.meta.started, "%-d %b %H:%M"),
                p.meta.mode
            ),
        );
        let mut o = format!("\n   {B}New issues{N}\n");
        let new: Vec<&String> = b.difference(&a).collect();
        if new.is_empty() {
            let _ = writeln!(o, "     {m}none{N}");
        }
        for k in new {
            let mut parts = k.splitn(3, '\t');
            let (sev, sec, msg) = (
                parts.next().unwrap_or(""),
                parts.next().unwrap_or(""),
                parts.next().unwrap_or(""),
            );
            let _ = writeln!(
                o,
                "     {}{}{N} {m}{sec}:{N} {}",
                sev_color(sev, m),
                icon(sev),
                msg.strip_suffix(':').unwrap_or(msg)
            );
        }
        let _ = write!(o, "\n   {B}Resolved{N}\n");
        let resolved: Vec<&String> = a.difference(&b).collect();
        if resolved.is_empty() {
            let _ = writeln!(o, "     {m}none{N}");
        }
        for k in resolved {
            let mut parts = k.splitn(3, '\t');
            let (_, sec, msg) = (
                parts.next(),
                parts.next().unwrap_or(""),
                parts.next().unwrap_or(""),
            );
            let _ = writeln!(
                o,
                "     {GRN}✓{N} {m}{sec}:{N} {}",
                msg.strip_suffix(':').unwrap_or(msg)
            );
        }
        if p.meta.mode != c.meta.mode {
            let _ = writeln!(
                o,
                "\n   {m}The scans used different modes, so some differences come from coverage, not changes.{N}"
            );
        }
        print(&o);
        self.pause();
    }

    fn export_md(&self, run: &Run) {
        let out = run.path.join("report.md");
        let [f, wn, ..] = counts(run);
        let s = score_of(f, wn);
        let mut md = format!("# Security audit: {}\n\n", run.meta.host);
        let _ = write!(
            md,
            "{} · {} scan · kernel {} · score **{s} ({})** · {f} fail · {wn} warn\n\n",
            local_time(run.meta.started, "%Y-%m-%d %H:%M"),
            run.meta.mode,
            run.meta.kernel,
            grade_of(s)
        );
        md.push_str(&checklist(run));
        let _ = std::fs::write(&out, md);
        let path = out.display().to_string();
        let _ = Command::new("wl-copy")
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .ok()
            .and_then(|mut c| {
                c.stdin.take()?.write_all(path.as_bytes()).ok()?;
                c.wait().ok()
            });
        let home = std::env::var("HOME").unwrap_or_default();
        let shown = if !home.is_empty() && path.starts_with(&home) {
            path.replacen(&home, "~", 1)
        } else {
            path
        };
        print(&format!(
            "\n   {GRN}✓ Saved{N} {shown}\n   {}(path copied to clipboard){N}\n",
            self.m
        ));
        self.pause();
    }

    fn history_view(&self) {
        loop {
            print(CLEAR);
            self.title_bar("Security Audit", "History");
            print("\n");
            let m = &self.m;
            let mut items: Vec<(String, String)> = self
                .runs_newest_first()
                .iter()
                .filter_map(|d| {
                    let run = Self::load(d)?;
                    let [f, w, ..] = counts(&run);
                    let s = score_of(f, w);
                    let g = grade_of(s);
                    let l = format!(
                        "{}  {m}{}{N}  {}{} {g}{N}   {RED}{} fail{N}  {YEL}{} warn{N}",
                        local_time(run.meta.started, "%a %-d %b %H:%M"),
                        lpad(&run.meta.mode, 5),
                        grade_color(g),
                        rpad(&s.to_string(), 3),
                        rpad(&f.to_string(), 2),
                        rpad(&w.to_string(), 2)
                    );
                    Some((l, d.display().to_string()))
                })
                .collect();
            items.push((format!("{m}← Back{N}"), "back".into()));
            let Some(choice) = self.choose(
                "Past scans · newest first",
                lines().saturating_sub(8),
                &items,
            ) else {
                return;
            };
            if choice == "back" {
                return;
            }
            self.dashboard(Path::new(&choice));
        }
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

fn sev_color<'a>(sev: &str, muted: &'a str) -> &'a str {
    match sev {
        "fail" => RED,
        "warn" => YEL,
        "pass" => GRN,
        "skip" => muted,
        _ => BLU,
    }
}

/// The issue checklist, as the original's jq program writes it.
fn checklist(run: &Run) -> String {
    let issues: Vec<_> = run
        .items
        .iter()
        .filter(|i| matches!(sev_name(i.finding.sev), "fail" | "warn"))
        .collect();
    let mut sections: Vec<&str> = Vec::new();
    for i in &issues {
        if !sections.contains(&i.finding.section.as_str()) {
            sections.push(&i.finding.section);
        }
    }
    let mut out = String::new();
    for sec in sections {
        let mut items: Vec<_> = issues.iter().filter(|i| i.finding.section == sec).collect();
        items.sort_by_key(|i| sev_name(i.finding.sev) != "fail");
        let body: Vec<String> = items
            .iter()
            .map(|i| {
                let fd = &i.finding;
                let mut s = format!(
                    "- [ ] **{}** {}",
                    sev_name(fd.sev).to_uppercase(),
                    fd.msg.strip_suffix(':').unwrap_or(&fd.msg)
                );
                if !fd.detail.is_empty() {
                    let d: Vec<String> = fd.detail.split('\n').map(|l| format!("  {l}")).collect();
                    let _ = write!(s, "\n\n  ```\n{}\n  ```", d.join("\n"));
                }
                if !fd.fix.is_empty() {
                    let _ = write!(s, "\n\n  Fix: `{}`", fd.fix);
                }
                if !fd.hint.is_empty() {
                    let _ = write!(s, "\n\n  {}", fd.hint);
                }
                s
            })
            .collect();
        let _ = writeln!(out, "## {sec}\n\n{}\n", body.join("\n\n"));
    }
    out
}

#[cfg(test)]
mod tests {
    #[test]
    fn pads_by_bytes_like_bash_printf() {
        assert_eq!(super::lpad("é", 5), "é   ");
        assert_eq!(
            super::trunc("Secrets & credentials in home", 22),
            "Secrets & credentials…"
        );
        assert_eq!(
            super::lpad("Secrets & credentials…", 22),
            "Secrets & credentials…"
        );
    }
}
