//! External commands. Every system interaction goes through here, so tests
//! and dry runs have one seam.

use std::io::Read;
use std::path::Path;
use std::process::{Command, ExitStatus, Stdio};
use std::sync::OnceLock;
use std::thread;
use std::time::{Duration, Instant};

/// Variables every child process gets (e.g. `GIT_TERMINAL_PROMPT=0`).
/// Front ends set them here: `std::env::set_var` is unsafe in edition 2024,
/// and this workspace forbids unsafe code.
static CHILD_ENV: std::sync::Mutex<Vec<(String, String)>> = std::sync::Mutex::new(Vec::new());

/// Adds a variable for every command run through this module.
pub fn set_child_env(key: &str, value: &str) {
    if let Ok(mut env) = CHILD_ENV.lock() {
        env.retain(|(k, _)| k != key);
        env.push((key.to_owned(), value.to_owned()));
    }
}

/// A `Command` with the child environment applied. Use instead of
/// `Command::new` everywhere.
#[must_use]
pub fn command(program: &str) -> Command {
    let mut c = Command::new(program);
    if let Ok(env) = CHILD_ENV.lock() {
        for (k, v) in env.iter() {
            c.env(k, v);
        }
    }
    c
}

/// A command that failed: what ran, how it ended, and its stderr.
#[derive(Debug, thiserror::Error)]
#[error("{cmd}: {status}{}", if stderr.is_empty() { String::new() } else { format!(": {stderr}") })]
pub struct CmdError {
    /// The command line, for the message.
    pub cmd: String,
    /// How it ended, e.g. "exit status 1".
    pub status: String,
    /// Trimmed stderr.
    pub stderr: String,
}

impl CmdError {
    /// pacman's "no packages matched": status 1 with nothing on stderr.
    #[must_use]
    pub fn is_empty_query(&self) -> bool {
        self.status == "exit status 1" && self.stderr.is_empty()
    }
}

fn describe(status: ExitStatus) -> String {
    status.code().map_or_else(
        || "killed by a signal".to_owned(),
        |c| format!("exit status {c}"),
    )
}

/// Runs a command in `dir` (or the current directory) with extra
/// environment, and returns trimmed stdout.
///
/// # Errors
/// When it can't start or exits unsuccessfully.
pub fn output_env(
    dir: Option<&Path>,
    env: &[(&str, &str)],
    argv: &[&str],
) -> Result<String, CmdError> {
    let cmd_line = argv.join(" ");
    let Some((program, rest)) = argv.split_first() else {
        return Err(CmdError {
            cmd: cmd_line,
            status: "empty command".into(),
            stderr: String::new(),
        });
    };
    let mut cmd = command(program);
    cmd.args(rest).stdin(Stdio::null());
    if let Some(d) = dir {
        cmd.current_dir(d);
    }
    for (k, v) in env {
        cmd.env(k, v);
    }
    let out = cmd.output().map_err(|e| CmdError {
        cmd: cmd_line.clone(),
        status: e.to_string(),
        stderr: String::new(),
    })?;
    if !out.status.success() {
        return Err(CmdError {
            cmd: cmd_line,
            status: describe(out.status),
            stderr: String::from_utf8_lossy(&out.stderr).trim().to_owned(),
        });
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_owned())
}

/// [`output_env`] without extra environment.
///
/// # Errors
/// When it can't start or exits unsuccessfully.
pub fn output(dir: Option<&Path>, argv: &[&str]) -> Result<String, CmdError> {
    output_env(dir, &[], argv)
}

/// Runs a command and returns (succeeded, stdout) whatever the exit status;
/// `None` when it can't start. For tools like `pacman -Qo` that exit 1 when
/// any one of many arguments has no answer.
#[must_use]
pub fn capture(argv: &[&str]) -> Option<(bool, String)> {
    let (program, rest) = argv.split_first()?;
    let out = command(program)
        .args(rest)
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .ok()?;
    Some((
        out.status.success(),
        String::from_utf8_lossy(&out.stdout).into_owned(),
    ))
}

/// Which of `paths` some package owns, asked of pacman in batches rather
/// than loading every owned path (RUST.md §9: ask, don't dump).
#[must_use]
pub fn pacman_owned(paths: &[String]) -> std::collections::BTreeSet<String> {
    let mut owned = std::collections::BTreeSet::new();
    for chunk in paths.chunks(200) {
        let mut argv = vec!["pacman", "-Qo", "--"];
        argv.extend(chunk.iter().map(String::as_str));
        let Some((_, out)) = capture(&argv) else {
            continue;
        };
        // "<path> is owned by <pkg> <ver>"
        for line in out.lines() {
            if let Some((path, _)) = line.split_once(" is owned by ") {
                owned.insert(path.to_owned());
            }
        }
    }
    owned
}

/// Non-empty, trimmed lines of a command's stdout.
///
/// # Errors
/// When it can't start or exits unsuccessfully.
pub fn lines(dir: Option<&Path>, argv: &[&str]) -> Result<Vec<String>, CmdError> {
    Ok(output(dir, argv)?
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .map(str::to_owned)
        .collect())
}

/// Runs a command attached to the terminal, for installers that prompt
/// (sudo, AUR builds).
///
/// # Errors
/// When it can't start or exits unsuccessfully.
pub fn interactive(argv: &[&str]) -> Result<(), CmdError> {
    let cmd_line = argv.join(" ");
    let Some((program, rest)) = argv.split_first() else {
        return Err(CmdError {
            cmd: cmd_line,
            status: "empty command".into(),
            stderr: String::new(),
        });
    };
    let status = command(program).args(rest).status().map_err(|e| CmdError {
        cmd: cmd_line.clone(),
        status: e.to_string(),
        stderr: String::new(),
    })?;
    if status.success() {
        Ok(())
    } else {
        Err(CmdError {
            cmd: cmd_line,
            status: describe(status),
            stderr: String::new(),
        })
    }
}

/// Whether a command is on `PATH`.
#[must_use]
pub fn has(name: &str) -> bool {
    use std::os::unix::fs::PermissionsExt;
    std::env::var_os("PATH").is_some_and(|paths| {
        std::env::split_paths(&paths).any(|d| {
            std::fs::metadata(d.join(name))
                .is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
        })
    })
}

/// git `-c` flags that authenticate https remotes with gh's token, so
/// private repos clone on a fresh machine before ssh keys exist.
#[must_use]
pub fn git_auth() -> &'static [&'static str] {
    static AUTH: OnceLock<bool> = OnceLock::new();
    if *AUTH.get_or_init(|| has("gh")) {
        &[
            "-c",
            "credential.helper=",
            "-c",
            "credential.helper=!gh auth git-credential",
        ]
    } else {
        &[]
    }
}

/// What the watchdog says about a long install.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Verdict {
    /// Keep going.
    Continue,
    /// Progressing, but too slowly to be worth waiting for.
    TooSlow,
    /// No progress at all for too long.
    Stalled,
}

/// Decides, from cumulative progress samples, whether a long install should
/// keep going. It holds no clock and no process, so the policy is testable
/// on synthetic timelines. "Progress" is bytes the process tree has read
/// (network and disk), so a compile reading sources counts as progress.
#[derive(Clone, Debug)]
pub struct Watchdog {
    /// No verdict before this: connection setup, dependency resolution.
    pub grace: Duration,
    /// Rate is measured over this trailing window.
    pub window: Duration,
    /// Bytes/s; below this the window counts as slow.
    pub min_rate: f64,
    /// Continuously slow for this long: too slow.
    pub slow_for: Duration,
    /// No progress at all for this long: stalled.
    pub stall_for: Duration,
    samples: std::collections::VecDeque<(Duration, u64)>,
    slow_since: Option<Duration>,
    last_move: Duration,
    last_bytes: u64,
}

impl Default for Watchdog {
    /// Tuned for package and toolchain downloads: patient with a slow mirror
    /// or a long compile, but gives up on dial-up speeds (10 kB/s took ~5 h
    /// for one 193 MB JDK).
    fn default() -> Self {
        Self {
            grace: Duration::from_secs(45),
            window: Duration::from_secs(120),
            min_rate: 32.0 * 1024.0,
            slow_for: Duration::from_secs(300),
            stall_for: Duration::from_secs(180),
            samples: std::collections::VecDeque::new(),
            slow_since: None,
            last_move: Duration::ZERO,
            last_bytes: 0,
        }
    }
}

impl Watchdog {
    /// Records cumulative `bytes` at elapsed time `t`; samples arrive in order.
    pub fn observe(&mut self, t: Duration, bytes: u64) -> Verdict {
        self.samples.push_back((t, bytes));
        if bytes > self.last_bytes {
            self.last_bytes = bytes;
            self.last_move = t;
        }
        // Keep one sample at or before the window start so the rate spans it.
        while self.samples.len() > 2 && self.samples.get(1).is_some_and(|s| s.0 + self.window <= t)
        {
            self.samples.pop_front();
        }
        if t < self.grace {
            return Verdict::Continue;
        }
        if t.saturating_sub(self.last_move) >= self.stall_for {
            return Verdict::Stalled;
        }
        let Some(&(t0, b0)) = self.samples.front() else {
            return Verdict::Continue;
        };
        if t.saturating_sub(t0) < self.window {
            return Verdict::Continue; // not a full window yet
        }
        #[allow(clippy::cast_precision_loss, reason = "byte counts far below 2^52")]
        let rate = bytes.saturating_sub(b0) as f64 / t.saturating_sub(t0).as_secs_f64();
        if rate >= self.min_rate {
            self.slow_since = None;
            return Verdict::Continue;
        }
        let since = *self.slow_since.get_or_insert(t);
        if t.saturating_sub(since) >= self.slow_for {
            Verdict::TooSlow
        } else {
            Verdict::Continue
        }
    }

    /// Current trailing-window rate in bytes/s, for messages.
    #[must_use]
    pub fn rate(&self) -> f64 {
        match (self.samples.front(), self.samples.back()) {
            (Some(a), Some(b)) if b.0 > a.0 => {
                #[allow(clippy::cast_precision_loss, reason = "byte counts far below 2^52")]
                let bytes = b.1.saturating_sub(a.1) as f64;
                bytes / b.0.saturating_sub(a.0).as_secs_f64()
            }
            _ => 0.0,
        }
    }
}

/// A watched command was stopped: too slow or stalled. Nothing is wrong with
/// what it installed; running the same step again continues it.
#[derive(Debug, thiserror::Error)]
pub enum WatchError {
    /// Stopped by the watchdog.
    #[error("{}", deferred_msg(*.verdict, *.rate))]
    Deferred {
        /// Why.
        verdict: Verdict,
        /// Bytes/s over the last window when stopped.
        rate: f64,
    },
    /// Failed on its own.
    #[error(transparent)]
    Failed(#[from] CmdError),
}

fn deferred_msg(verdict: Verdict, rate: f64) -> String {
    if verdict == Verdict::Stalled {
        return "no progress for several minutes".into();
    }
    let human = if rate >= 1_048_576.0 {
        format!("{:.1} MB/s", rate / 1_048_576.0)
    } else if rate >= 1024.0 {
        format!("{:.0} kB/s", rate / 1024.0)
    } else {
        format!("{rate:.0} B/s")
    };
    format!("download too slow ({human})")
}

/// How often progress is sampled.
pub const WATCH_INTERVAL: Duration = Duration::from_secs(5);

/// Runs a command attached to the terminal under `w`. If it crawls or
/// stalls, its whole process tree is stopped (TERM, then KILL after 10 s).
/// The child stays in our process group so it can still prompt (sudo).
///
/// # Errors
/// [`WatchError::Deferred`] when stopped, [`WatchError::Failed`] otherwise.
pub fn interactive_watched(
    mut w: Watchdog,
    argv: &[&str],
    interval: Duration,
) -> Result<(), WatchError> {
    let cmd_line = argv.join(" ");
    let fail = |status: String| CmdError {
        cmd: cmd_line.clone(),
        status,
        stderr: String::new(),
    };
    let Some((program, rest)) = argv.split_first() else {
        return Err(fail("empty command".into()).into());
    };
    let mut child = command(program)
        .args(rest)
        .spawn()
        .map_err(|e| fail(e.to_string()))?;
    let start = Instant::now();
    let root = child.id();
    let mut next = start + interval;
    loop {
        if let Some(status) = child.try_wait().map_err(|e| fail(e.to_string()))? {
            return if status.success() {
                Ok(())
            } else {
                Err(fail(describe(status)).into())
            };
        }
        if Instant::now() >= next {
            next += interval;
            let verdict = w.observe(start.elapsed(), tree_read_bytes(root));
            if verdict != Verdict::Continue {
                stop_tree(root, &mut child);
                return Err(WatchError::Deferred {
                    verdict,
                    rate: w.rate(),
                });
            }
        }
        thread::sleep(Duration::from_millis(100));
    }
}

/// Sum of `rchar` over a process and its descendants. Processes we can't
/// read (root under sudo) are skipped, which only makes the count conservative.
fn tree_read_bytes(root: u32) -> u64 {
    descendants(root)
        .iter()
        .filter_map(|pid| std::fs::read_to_string(format!("/proc/{pid}/io")).ok())
        .filter_map(|io| {
            io.lines().find_map(|l| {
                l.strip_prefix("rchar: ")
                    .and_then(|v| v.trim().parse::<u64>().ok())
            })
        })
        .sum()
}

/// `root` and every process below it, from `/proc/*/stat`.
fn descendants(root: u32) -> Vec<u32> {
    let mut children: std::collections::HashMap<u32, Vec<u32>> = std::collections::HashMap::new();
    for e in std::fs::read_dir("/proc").into_iter().flatten().flatten() {
        let Ok(pid) = e.file_name().to_string_lossy().parse::<u32>() else {
            continue;
        };
        let Ok(stat) = std::fs::read_to_string(e.path().join("stat")) else {
            continue;
        };
        // The name is in parentheses and may contain spaces; the parent pid
        // is the second field after the closing one.
        let Some(rest) = stat.rfind(')').and_then(|i| stat.get(i + 1..)) else {
            continue;
        };
        if let Some(ppid) = rest.split_whitespace().nth(1).and_then(|p| p.parse().ok()) {
            children.entry(ppid).or_default().push(pid);
        }
    }
    let mut out = vec![root];
    let mut i = 0;
    while let Some(&p) = out.get(i) {
        out.extend(children.get(&p).into_iter().flatten().copied());
        i += 1;
    }
    out
}

/// Asks the whole tree to stop, then forces it, children first so a parent
/// can't respawn them. Uses `kill(1)`: no `unsafe` libc calls.
fn stop_tree(root: u32, child: &mut std::process::Child) {
    let signal = |sig: &str| {
        let pids: Vec<String> = descendants(root).iter().rev().map(u32::to_string).collect();
        let mut args = vec![sig, "--"];
        args.extend(pids.iter().map(String::as_str));
        let _ = Command::new("kill")
            .args(&args)
            .stderr(Stdio::null())
            .status();
    };
    signal("-TERM");
    let deadline = Instant::now() + Duration::from_secs(10);
    while Instant::now() < deadline {
        if matches!(child.try_wait(), Ok(Some(_))) {
            return;
        }
        thread::sleep(Duration::from_millis(100));
    }
    signal("-KILL");
    let _ = child.wait();
}

/// Reads at most `limit` bytes of a file; for small config files.
///
/// # Errors
/// When the file can't be opened or read.
pub fn read_bounded(path: &Path, limit: u64) -> std::io::Result<Vec<u8>> {
    let mut buf = Vec::new();
    std::fs::File::open(path)?
        .take(limit)
        .read_to_end(&mut buf)?;
    Ok(buf)
}

#[cfg(test)]
mod tests {
    use super::{Verdict, Watchdog};
    use std::time::Duration;

    /// Feeds a synthetic timeline (rate per 5 s step) and returns the first
    /// non-Continue verdict and when it came.
    fn run(total_s: u64, rate: impl Fn(u64) -> f64) -> (Verdict, u64) {
        let mut w = Watchdog::default();
        let mut bytes = 0.0_f64;
        let mut t = 0;
        while t <= total_s {
            #[allow(
                clippy::cast_possible_truncation,
                clippy::cast_sign_loss,
                reason = "test byte counts"
            )]
            let v = w.observe(Duration::from_secs(t), bytes as u64);
            if v != Verdict::Continue {
                return (v, t);
            }
            bytes += rate(t) * 5.0;
            t += 5;
        }
        (Verdict::Continue, total_s)
    }

    #[test]
    fn watchdog_policy_matches_go() {
        type Rate = Box<dyn Fn(u64) -> f64>;
        let kb = 1024.0;
        let cases: [(&str, Rate, Verdict, u64); 6] = [
            (
                "fast download finishes untouched",
                Box::new(move |_| 2000.0 * kb),
                Verdict::Continue,
                0,
            ),
            (
                "the real JDK case: 10 kB/s",
                Box::new(move |_| 10.0 * kb),
                Verdict::TooSlow,
                480,
            ),
            (
                "dead connection",
                Box::new(move |t| if t < 60 { 500.0 * kb } else { 0.0 }),
                Verdict::Stalled,
                300,
            ),
            (
                "slow start then recovers",
                Box::new(move |t| if t < 180 { 5.0 * kb } else { 800.0 * kb }),
                Verdict::Continue,
                0,
            ),
            (
                "bursty compile",
                Box::new(move |t| if t % 60 < 10 { 3000.0 * kb } else { 0.0 }),
                Verdict::Continue,
                0,
            ),
            (
                "silent during grace",
                Box::new(move |t| if t < 40 { 0.0 } else { 1000.0 * kb }),
                Verdict::Continue,
                0,
            ),
        ];
        for (name, rate, want, before) in cases {
            let (v, at) = run(1800, rate);
            assert_eq!(v, want, "{name}: verdict at {at}s");
            if before > 0 {
                assert!(at <= before, "{name}: came at {at}s, want by {before}s");
            }
        }
    }

    #[test]
    fn watched_command_runs_and_fails_like_a_command() {
        assert!(
            super::interactive_watched(Watchdog::default(), &["true"], Duration::from_millis(50))
                .is_ok()
        );
        assert!(
            super::interactive_watched(Watchdog::default(), &["false"], Duration::from_millis(50))
                .is_err()
        );
    }

    #[test]
    fn stalled_command_is_deferred_and_its_tree_stopped() {
        let w = Watchdog {
            grace: Duration::ZERO,
            stall_for: Duration::from_millis(300),
            ..Watchdog::default()
        };
        let start = std::time::Instant::now();
        let r = super::interactive_watched(
            w,
            &["sh", "-c", "sleep 30 & sleep 30"],
            Duration::from_millis(100),
        );
        assert!(
            matches!(
                r,
                Err(super::WatchError::Deferred {
                    verdict: Verdict::Stalled,
                    ..
                })
            ),
            "{r:?}"
        );
        assert!(start.elapsed() < Duration::from_secs(5), "stopped promptly");
    }
}
