//! A scan in progress: the engine (this same binary) as a child process,
//! writing `findings.jsonl` into a new run directory while its report lines
//! stream back for live progress. Full and deep scans run under `sudo -n`,
//! after the TUI has made sure sudo has the password cached.

use std::fs::File;
use std::io::{BufRead, BufReader, Write};
use std::os::unix::process::CommandExt;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::mpsc::{self, Receiver};
use std::thread;
use std::time::Instant;

use secaudit_core::checks::SECTIONS;
use secaudit_core::store::{Meta, Store, now};

/// What the engine reported, line by line.
pub enum Event {
    /// A `== Section ==` header: the previous section is done.
    Section(String),
    /// A `[WARN]` or `[FAIL]` line in the current section.
    Issue { fail: bool, msg: String },
    /// The engine exited.
    Exited,
}

/// Progress of one section.
#[derive(Clone, Debug, Default)]
pub struct Row {
    pub name: &'static str,
    pub state: RowState,
    pub fail: usize,
    pub warn: usize,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum RowState {
    #[default]
    Waiting,
    Running,
    Done,
}

pub struct Scan {
    pub mode: String,
    pub dir: PathBuf,
    pub meta: Meta,
    pub started: Instant,
    pub rows: Vec<Row>,
    pub log: Vec<(bool, String)>,
    pub failed: Option<String>,
    pub finished: bool,
    child: Child,
    sudo: bool,
    events: Receiver<Event>,
}

impl Scan {
    /// Starts the engine for `mode` (`quick`, `full`, `deep`).
    pub fn start(store: &Store, mode: &str) -> std::io::Result<Self> {
        let host = std::fs::read_to_string("/etc/hostname")
            .unwrap_or_default()
            .trim()
            .to_owned();
        let kernel = std::fs::read_to_string("/proc/sys/kernel/osrelease")
            .unwrap_or_default()
            .trim()
            .to_owned();
        let meta = Meta {
            mode: mode.to_owned(),
            started: now(),
            host,
            user: std::env::var("USER").unwrap_or_default(),
            kernel,
            finished: None,
        };
        let stamp = stamp();
        let dir = store.begin(&stamp, &meta)?;
        let sudo = mode != "quick";
        let (child, events) = spawn_engine(&dir, mode, sudo)?;
        let rows = SECTIONS
            .iter()
            .map(|s| Row {
                name: s.name,
                ..Row::default()
            })
            .collect();
        Ok(Self {
            mode: mode.to_owned(),
            dir,
            meta,
            started: Instant::now(),
            rows,
            log: Vec::new(),
            failed: None,
            finished: false,
            child,
            sudo,
            events,
        })
    }

    /// Applies engine events; returns true when something changed.
    pub fn poll(&mut self, store: &Store) -> bool {
        let mut changed = false;
        while let Ok(event) = self.events.try_recv() {
            changed = true;
            match event {
                Event::Section(name) => {
                    for r in &mut self.rows {
                        if r.state == RowState::Running {
                            r.state = RowState::Done;
                        }
                        if r.name == name {
                            r.state = RowState::Running;
                        }
                    }
                }
                Event::Issue { fail, msg } => {
                    if let Some(r) = self.rows.iter_mut().find(|r| r.state == RowState::Running) {
                        if fail {
                            r.fail += 1;
                        } else {
                            r.warn += 1;
                        }
                        self.log
                            .push((fail, format!("{}: {}", r.name, msg.trim_end_matches(':'))));
                    }
                }
                Event::Exited => self.exited(store),
            }
        }
        changed
    }

    fn exited(&mut self, store: &Store) {
        let _ = self.child.wait();
        let findings = self.dir.join("findings.jsonl");
        if std::fs::metadata(&findings).map_or(0, |m| m.len()) == 0 {
            let out = std::fs::read_to_string(self.dir.join("output.txt")).unwrap_or_default();
            let tail: Vec<&str> = out
                .lines()
                .rev()
                .take(3)
                .collect::<Vec<_>>()
                .into_iter()
                .rev()
                .collect();
            let mut reason = if tail.is_empty() {
                "the engine produced no findings".to_owned()
            } else {
                tail.join("\n")
            };
            if reason.contains("password is required") {
                reason.push_str("\nsudo did not keep your password for this terminal. Start the scan again; if it keeps happening, run 'sudo -v' first.");
            }
            let _ = std::fs::remove_dir_all(&self.dir);
            self.failed = Some(reason);
            return;
        }
        for r in &mut self.rows {
            r.state = RowState::Done;
        }
        let _ = store.finish(&self.dir, &mut self.meta);
        self.finished = true;
    }

    /// Stops the engine and removes the partial run.
    pub fn cancel(&mut self) {
        let pid = self.child.id().to_string();
        // Quick scans own a process group; sudo relays SIGTERM to the root engine.
        let target = if self.sudo { pid } else { format!("-{pid}") };
        let _ = Command::new("kill").args(["-TERM", "--", &target]).status();
        let _ = self.child.wait();
        let _ = std::fs::remove_dir_all(&self.dir);
    }

    /// Sections finished, for the progress bar.
    pub fn done(&self) -> usize {
        self.rows
            .iter()
            .filter(|r| r.state == RowState::Done)
            .count()
    }
}

/// Starts the engine writing into `dir`, and a reader thread that saves its
/// output to `output.txt` and turns report lines into events.
fn spawn_engine(
    dir: &std::path::Path,
    mode: &str,
    sudo: bool,
) -> std::io::Result<(Child, Receiver<Event>)> {
    let exe = std::env::current_exe()?;
    let mut cmd = if sudo {
        let mut c = Command::new("sudo");
        c.arg("-n").arg(&exe);
        c
    } else {
        let mut c = Command::new(&exe);
        // Own process group, so cancel stops the whole tree.
        c.process_group(0);
        c
    };
    cmd.arg("--no-color")
        .arg("--json")
        .arg(dir.join("findings.jsonl"));
    if mode == "deep" {
        cmd.arg("--deep");
    }
    let mut child = cmd
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    let stdout = child.stdout.take();
    let stderr = child.stderr.take();
    File::create(dir.join("output.txt"))?;
    let append = || File::options().append(true).open(dir.join("output.txt"));
    let mut errors = append()?;
    let mut output = append()?;
    let (tx, events) = mpsc::channel();
    let stderr_done = thread::spawn(move || {
        // Small: sudo's complaint or an engine error.
        for line in stderr
            .into_iter()
            .flat_map(|e| BufReader::new(e).lines().map_while(Result::ok))
        {
            let _ = writeln!(errors, "{line}");
        }
    });
    thread::spawn(move || {
        for line in stdout
            .into_iter()
            .flat_map(|o| BufReader::new(o).lines().map_while(Result::ok))
        {
            let _ = writeln!(output, "{line}");
            if let Some(e) = parse(&line)
                && tx.send(e).is_err()
            {
                return;
            }
        }
        // Wait for stderr, so a failure's reason is in output.txt before it is read.
        let _ = stderr_done.join();
        let _ = tx.send(Event::Exited);
    });
    Ok((child, events))
}

/// A report line as an event: a section header, or a WARN/FAIL finding.
fn parse(line: &str) -> Option<Event> {
    if let Some(name) = line.strip_prefix("== ").and_then(|l| l.strip_suffix(" ==")) {
        return Some(Event::Section(name.to_owned()));
    }
    let t = line.trim_start();
    t.strip_prefix("[FAIL] ")
        .map(|m| Event::Issue {
            fail: true,
            msg: m.to_owned(),
        })
        .or_else(|| {
            t.strip_prefix("[WARN] ").map(|m| Event::Issue {
                fail: false,
                msg: m.to_owned(),
            })
        })
}

/// `YYYYmmdd-HHMMSS` in local time, as the other front ends name runs.
fn stamp() -> String {
    Command::new("date")
        .arg("+%Y%m%d-%H%M%S")
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_owned())
        .ok()
        .filter(|s| s.len() == 15)
        .unwrap_or_else(|| now().to_string())
}

/// Makes sure sudo has a cached password, asking in the plain terminal if
/// needed. The caller has already left the TUI screen.
pub fn ensure_sudo(mode: &str) -> bool {
    if Command::new("sudo")
        .args(["-n", "true"])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|s| s.success())
    {
        return true;
    }
    let mut out = std::io::stdout();
    let _ = writeln!(out, "\n  \x1b[1mSecurity Audit · {mode} scan\x1b[0m\n");
    let _ = writeln!(
        out,
        "  This scan reads root-only files: shadow, sudoers, firewall rules, LUKS headers."
    );
    let _ = writeln!(
        out,
        "  \x1b[2mIt never changes anything; sudo is only used to look.\x1b[0m\n"
    );
    Command::new("sudo")
        .arg("-v")
        .status()
        .is_ok_and(|s| s.success())
}

#[cfg(test)]
mod tests {
    use super::{Event, parse};

    #[test]
    fn parses_report_lines() {
        assert!(matches!(parse("== SSH =="), Some(Event::Section(s)) if s == "SSH"));
        assert!(
            matches!(parse("  [FAIL] x"), Some(Event::Issue { fail: true, msg }) if msg == "x")
        );
        assert!(matches!(
            parse("  [WARN] y:"),
            Some(Event::Issue { fail: false, .. })
        ));
        assert!(parse("  [PASS] z").is_none());
        assert!(parse("         detail").is_none());
    }
}
