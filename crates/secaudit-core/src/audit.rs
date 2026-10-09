//! The engine: who is audited, how findings are collected, and the score.
//!
//! Each report section is a plain function over [`Ctx`] that records
//! findings in a [`Report`], like the Bash helpers `ok`/`wn`/`bad`/`info`/`skp`.

use std::fs::{self, Metadata};
use std::io::Read;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Arc;
use std::time::{Duration, Instant};
use std::{env, thread};

use crate::contract::{CONTRACT_SCHEMA, Finding, Severity, Target};

/// Output of an external command.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct CmdOut {
    /// Exited with status 0.
    pub success: bool,
    /// Standard output, lossily decoded, at most [`MAX_OUTPUT`] bytes.
    pub stdout: String,
}

impl CmdOut {
    /// A result, for fake runners in tests.
    #[must_use]
    pub fn new(success: bool, stdout: &str) -> Self {
        Self {
            success,
            stdout: stdout.to_owned(),
        }
    }
}

/// Runs external commands. A seam so tests can fake the system.
pub trait CommandRunner: Send + Sync + std::fmt::Debug {
    /// Runs `argv` (no shell). `None` if it can't start or times out.
    fn run(&self, argv: &[&str]) -> Option<CmdOut>;
}

/// Longest any audit command may run.
pub const TIMEOUT: Duration = Duration::from_secs(30);

/// Most stdout kept from one command.
pub const MAX_OUTPUT: u64 = 4 * 1024 * 1024;

/// The real system: `std::process::Command` with `LC_ALL=C`, stdin closed,
/// stderr discarded, and a [`TIMEOUT`].
#[derive(Debug, Default)]
pub struct System;

impl CommandRunner for System {
    fn run(&self, argv: &[&str]) -> Option<CmdOut> {
        let (program, rest) = argv.split_first()?;
        let mut child = Command::new(program)
            .args(rest)
            .env("LC_ALL", "C")
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .ok()?;
        let mut out = child.stdout.take()?;
        let reader = thread::spawn(move || {
            let mut buf = Vec::new();
            let _ = out.by_ref().take(MAX_OUTPUT).read_to_end(&mut buf);
            buf
        });
        let deadline = Instant::now() + TIMEOUT;
        // ponytail: kills the command, not its children; none of the audit's
        // commands fork. Use a process group if one ever does.
        let status = loop {
            match child.try_wait() {
                Ok(Some(status)) => break Some(status),
                Ok(None) if Instant::now() < deadline => thread::sleep(Duration::from_millis(2)),
                _ => {
                    let _ = child.kill();
                    let _ = child.wait();
                    break None;
                }
            }
        };
        let stdout = reader.join().ok()?;
        Some(CmdOut {
            success: status?.success(),
            stdout: String::from_utf8_lossy(&stdout).into_owned(),
        })
    }
}

/// Who and what is being audited.
#[derive(Clone, Debug)]
#[non_exhaustive]
pub struct Ctx {
    /// The audited user's home. Under sudo this is the invoking user's.
    pub home: PathBuf,
    /// The audited user's name.
    pub user: String,
    /// Running as root, so root-only checks run instead of being skipped.
    pub is_root: bool,
    /// `--deep`: slower, wider scans.
    pub deep: bool,
    /// `$PATH` of the audit, whose directories are checked.
    pub path_env: String,
    /// Where `/` is; tests point it at a fake system tree.
    pub root: PathBuf,
    /// How external commands run.
    pub cmd: Arc<dyn CommandRunner>,
}

impl Ctx {
    /// The real machine: `SUDO_USER`'s home under sudo, else `$USER`'s `$HOME`.
    #[must_use]
    pub fn from_env() -> Self {
        let sudo_user = env::var("SUDO_USER").ok();
        let user = sudo_user
            .clone()
            .or_else(|| env::var("USER").ok())
            .unwrap_or_default();
        let home = sudo_user
            .and_then(|u| passwd_home(&u))
            .or_else(|| env::var_os("HOME").map(PathBuf::from))
            .unwrap_or_default();
        Self {
            home,
            user,
            is_root: effective_uid() == Some(0),
            ..Self::for_home("")
        }
    }

    /// An audit of `home` as a normal user on the real system. For `--home` and tests.
    #[must_use]
    pub fn for_home(home: impl Into<PathBuf>) -> Self {
        Self {
            home: home.into(),
            user: env::var("USER").unwrap_or_default(),
            is_root: false,
            deep: false,
            path_env: env::var("PATH").unwrap_or_default(),
            root: PathBuf::from("/"),
            cmd: Arc::new(System),
        }
    }

    /// Runs a command; a failure to start counts as an unsuccessful, silent run.
    #[must_use]
    pub fn run(&self, argv: &[&str]) -> CmdOut {
        self.cmd.run(argv).unwrap_or_default()
    }

    /// Where absolute system path `abs` (e.g. `/etc/shadow`) is under [`Ctx::root`].
    #[must_use]
    pub fn sys(&self, abs: &str) -> PathBuf {
        self.root.join(abs.trim_start_matches('/'))
    }

    /// The system path as the report prints it, i.e. without [`Ctx::root`].
    #[must_use]
    pub fn unsys(&self, path: &Path) -> String {
        match path.strip_prefix(&self.root) {
            Ok(rest) => format!("/{}", rest.display()),
            Err(_) => path.display().to_string(),
        }
    }

    /// Lines of a system text file; empty if missing or unreadable.
    #[must_use]
    pub fn read_lines(&self, abs: &str) -> Vec<String> {
        read_text(&self.sys(abs))
            .lines()
            .map(str::to_owned)
            .collect()
    }

    /// Name for a uid or gid from `/etc/passwd` or `/etc/group`, or `UNKNOWN` as `stat` prints.
    #[must_use]
    pub fn id_name(&self, db: &str, id: u32) -> String {
        let id = id.to_string();
        self.read_lines(db)
            .iter()
            .find_map(|l| {
                let f: Vec<&str> = l.split(':').collect();
                (f.get(2) == Some(&id.as_str()))
                    .then(|| f.first().map(|n| (*n).to_owned()))
                    .flatten()
            })
            .unwrap_or_else(|| "UNKNOWN".to_owned())
    }

    /// `path` with the home prefix shown as `~`, as the report prints it.
    #[must_use]
    pub fn tilde(&self, path: &Path) -> String {
        match path.strip_prefix(&self.home) {
            Ok(rest) if !self.home.as_os_str().is_empty() => format!("~/{}", rest.display()),
            _ => path.display().to_string(),
        }
    }
}

/// Home directory of `user` from `/etc/passwd`.
fn passwd_home(user: &str) -> Option<PathBuf> {
    let passwd = fs::read_to_string("/etc/passwd").ok()?;
    passwd.lines().find_map(|line| {
        let f: Vec<&str> = line.split(':').collect();
        (f.first() == Some(&user))
            .then(|| f.get(5).map(PathBuf::from))
            .flatten()
    })
}

/// Small config files are read whole, up to this size.
const MAX_TEXT: u64 = 1024 * 1024;

/// A small text file, at most [`MAX_TEXT`] bytes; empty if unreadable.
#[must_use]
pub fn read_text(path: &Path) -> String {
    let mut buf = Vec::new();
    let _ = fs::File::open(path).and_then(|f| f.take(MAX_TEXT).read_to_end(&mut buf));
    String::from_utf8_lossy(&buf).into_owned()
}

/// Walks `start` depth-first in directory order, exactly as `find -xdev`
/// does: `visit` sees `start` and every entry below it (return `false` to
/// stop the walk); `enter` decides whether to descend into a directory on
/// the same filesystem as `start`. Unreadable directories are skipped.
pub fn walk(
    start: &Path,
    mut enter: impl FnMut(&Path, &Metadata) -> bool,
    mut visit: impl FnMut(&Path, &Metadata) -> bool,
) {
    let Ok(meta) = fs::symlink_metadata(start) else {
        return;
    };
    if !visit(start, &meta) || !meta.is_dir() || !enter(start, &meta) {
        return;
    }
    let dev = meta.dev();
    let mut stack: Vec<fs::ReadDir> = fs::read_dir(start).into_iter().collect();
    while let Some(dir) = stack.last_mut() {
        let Some(entry) = dir.next() else {
            stack.pop();
            continue;
        };
        let Ok(entry) = entry else { continue };
        let path = entry.path();
        let Ok(meta) = fs::symlink_metadata(&path) else {
            continue;
        };
        if !visit(&path, &meta) {
            return;
        }
        if meta.is_dir()
            && meta.dev() == dev
            && enter(&path, &meta)
            && let Ok(rd) = fs::read_dir(&path)
        {
            stack.push(rd);
        }
    }
}

/// Effective uid from `/proc/self/status`, without `unsafe` libc calls.
fn effective_uid() -> Option<u32> {
    let status = fs::read_to_string("/proc/self/status").ok()?;
    let uids = status.lines().find_map(|l| l.strip_prefix("Uid:"))?;
    uids.split_whitespace().nth(1)?.parse().ok()
}

/// Findings collected during a run, in report order.
#[derive(Debug, Default)]
pub struct Report {
    section: String,
    findings: Vec<Finding>,
}

impl Report {
    /// Starts a report section; later findings belong to it.
    pub fn section(&mut self, name: &str) {
        name.clone_into(&mut self.section);
    }

    fn push(&mut self, sev: Severity, id: &str, msg: String) -> &mut Finding {
        let index = self.findings.len();
        self.findings.push(Finding {
            schema: CONTRACT_SCHEMA,
            id: Some(id.to_owned()),
            sev,
            section: self.section.clone(),
            msg,
            detail: String::new(),
            fix: String::new(),
            hint: String::new(),
            targets: Vec::new(),
        });
        &mut self.findings[index]
    }

    /// Records a passing check.
    pub fn ok(&mut self, id: &str, msg: impl Into<String>) -> &mut Finding {
        self.push(Severity::Pass, id, msg.into())
    }
    /// Records a warning.
    pub fn warn(&mut self, id: &str, msg: impl Into<String>) -> &mut Finding {
        self.push(Severity::Warn, id, msg.into())
    }
    /// Records a failure.
    pub fn fail(&mut self, id: &str, msg: impl Into<String>) -> &mut Finding {
        self.push(Severity::Fail, id, msg.into())
    }
    /// Records context.
    pub fn info(&mut self, id: &str, msg: impl Into<String>) -> &mut Finding {
        self.push(Severity::Info, id, msg.into())
    }
    /// Records a check that could not run.
    pub fn skip(&mut self, id: &str, msg: impl Into<String>) -> &mut Finding {
        self.push(Severity::Skip, id, msg.into())
    }

    /// The findings so far.
    #[must_use]
    pub fn findings(&self) -> &[Finding] {
        &self.findings
    }

    /// `100 − 10×fail − 2×warn`, floored at 0, and its letter grade.
    #[must_use]
    pub fn score(&self) -> (u32, char) {
        let count = |s| {
            u32::try_from(self.findings.iter().filter(|f| f.sev == s).count()).unwrap_or(u32::MAX)
        };
        let lost = count(Severity::Fail)
            .saturating_mul(10)
            .saturating_add(count(Severity::Warn).saturating_mul(2));
        let score = 100u32.saturating_sub(lost);
        let grade = match score {
            90.. => 'A',
            80..=89 => 'B',
            70..=79 => 'C',
            60..=69 => 'D',
            _ => 'F',
        };
        (score, grade)
    }
}

impl Finding {
    /// Adds a target the finding is about.
    pub fn target(&mut self, target: Target) -> &mut Self {
        self.targets.push(target);
        self
    }
    /// Adds a file target.
    pub fn file(&mut self, path: &Path) -> &mut Self {
        self.target(Target::File {
            path: path.display().to_string(),
        })
    }
    /// Sets the fix command.
    pub fn fix(&mut self, cmd: impl Into<String>) -> &mut Self {
        self.fix = cmd.into();
        self
    }
    /// Sets the advice.
    pub fn hint(&mut self, text: impl Into<String>) -> &mut Self {
        self.hint = text.into();
        self
    }
    /// Sets the detail.
    pub fn detail(&mut self, text: impl Into<String>) -> &mut Self {
        self.detail = text.into();
        self
    }
}

/// One report section of the audit.
#[derive(Clone, Copy, Debug)]
#[non_exhaustive]
pub struct Section {
    /// Heading, exactly as the Bash version prints it.
    pub name: &'static str,
    /// Runs the section's checks.
    pub run: fn(&Ctx, &mut Report),
}

/// Runs `sections` in order.
#[must_use]
pub fn run(ctx: &Ctx, sections: &[Section]) -> Report {
    let mut report = Report::default();
    for s in sections {
        report.section(s.name);
        (s.run)(ctx, &mut report);
    }
    report
}

/// Quotes `s` for a shell command line, only when needed.
#[must_use]
pub fn shell_quote(s: &str) -> String {
    let safe = |c: char| c.is_ascii_alphanumeric() || "_./~@%+=:,-".contains(c);
    if !s.is_empty() && s.chars().all(safe) {
        s.to_owned()
    } else {
        format!("'{}'", s.replace('\'', r"'\''"))
    }
}

#[cfg(test)]
mod tests {
    use super::{Ctx, Report, shell_quote, walk};
    use std::fs;
    use std::path::Path;

    #[test]
    fn walk_is_preorder_and_stops_on_request() {
        let t = tempfile::tempdir().unwrap_or_else(|e| unreachable!("{e}"));
        for d in ["a/b", "c"] {
            fs::create_dir_all(t.path().join(d)).ok();
        }
        fs::write(t.path().join("a/b/f"), "").ok();
        let mut seen = Vec::new();
        walk(
            t.path(),
            |_, _| true,
            |p, _| {
                seen.push(
                    p.strip_prefix(t.path())
                        .map(|r| r.display().to_string())
                        .unwrap_or_default(),
                );
                true
            },
        );
        let pos = |s: &str| seen.iter().position(|x| x == s);
        assert_eq!(seen.len(), 5, "{seen:?}");
        assert!(
            pos("a") < pos("a/b") && pos("a/b") < pos("a/b/f"),
            "a's subtree right after a: {seen:?}"
        );
        let mut n = 0;
        walk(
            t.path(),
            |_, _| true,
            |_, _| {
                n += 1;
                n < 2
            },
        );
        assert_eq!(n, 2);
    }

    #[test]
    fn sys_paths_map_under_root_and_back() {
        let mut ctx = Ctx::for_home("/home/u");
        ctx.root = "/tmp/fake".into();
        assert_eq!(ctx.sys("/etc/shadow"), Path::new("/tmp/fake/etc/shadow"));
        assert_eq!(ctx.unsys(Path::new("/tmp/fake/etc/shadow")), "/etc/shadow");
    }

    #[test]
    fn score_and_grade_follow_bash() {
        let mut r = Report::default();
        assert_eq!(r.score(), (100, 'A'));
        r.fail("a.b", "x");
        r.warn("a.b", "x");
        r.info("a.b", "x");
        r.ok("a.b", "x");
        assert_eq!(r.score(), (88, 'B'));
        for _ in 0..20 {
            r.fail("a.b", "x");
        }
        assert_eq!(r.score(), (0, 'F'));
    }

    #[test]
    fn tilde_only_replaces_the_home_prefix() {
        let ctx = Ctx::for_home("/home/u");
        assert_eq!(ctx.tilde(Path::new("/home/u/.netrc")), "~/.netrc");
        assert_eq!(ctx.tilde(Path::new("/home/user2/x")), "/home/user2/x");
    }

    #[test]
    fn quotes_only_when_needed() {
        assert_eq!(shell_quote("/home/u/.netrc"), "/home/u/.netrc");
        assert_eq!(shell_quote("/home/u/my key"), "'/home/u/my key'");
        assert_eq!(shell_quote("it's"), r"'it'\''s'");
        assert_eq!(shell_quote(""), "''");
    }
}
