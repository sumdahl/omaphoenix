use std::process::{Child, Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use super::exec::{CmdError, command, describe};
use super::watchdog::{Verdict, Watchdog};

/// A watched command was stopped (too slow or stalled); rerunning continues it.
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

/// Runs a terminal-attached command under `w`, stopping its process tree if it crawls.
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
    // Stays in our process group so it can still prompt (sudo).
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
            let (bytes, blind) = tree_progress(root);
            if blind {
                // Never stop what can't be measured: pacman under sudo must not die mid-transaction (F-018).
                w.unmeasured(start.elapsed(), bytes);
                continue;
            }
            let verdict = w.observe(start.elapsed(), bytes);
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

/// Bytes read by the tree, and whether part of it can't be measured (another user's process, e.g. root under sudo).
fn tree_progress(root: u32) -> (u64, bool) {
    let mut bytes = 0;
    let mut blind = false;
    for pid in descendants(root) {
        match std::fs::read_to_string(format!("/proc/{pid}/io")) {
            Ok(io) => bytes += io_bytes(&io),
            Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied => blind = true,
            Err(_) => {} // exited between listing and reading
        }
    }
    (bytes, blind)
}

/// Bytes read plus bytes written: a download read with `recv` (mise, Rust's std) leaves `rchar` flat
/// while the file it writes grows (F-032).
fn io_bytes(io: &str) -> u64 {
    io.lines()
        .filter_map(|l| {
            l.strip_prefix("rchar: ")
                .or_else(|| l.strip_prefix("wchar: "))
                .and_then(|v| v.trim().parse::<u64>().ok())
        })
        .sum()
}

fn descendants(root: u32) -> Vec<u32> {
    let mut children: std::collections::HashMap<u32, Vec<u32>> = std::collections::HashMap::new();
    for e in std::fs::read_dir("/proc").into_iter().flatten().flatten() {
        let Ok(pid) = e.file_name().to_string_lossy().parse::<u32>() else {
            continue;
        };
        let Ok(stat) = std::fs::read_to_string(e.path().join("stat")) else {
            continue;
        };
        // The name in parentheses may contain spaces; ppid is the 2nd field after it.
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

// TERM then KILL, children first so a parent can't respawn them; kill(1) avoids unsafe libc.
fn stop_tree(root: u32, child: &mut Child) {
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

#[cfg(test)]
mod tests {
    use super::{Watchdog, interactive_watched, io_bytes};
    use std::time::Duration;

    #[test]
    fn a_download_that_only_writes_still_counts_as_progress() {
        // /proc/<pid>/io of `mise install` mid-download: rchar flat, wchar growing.
        let before = "rchar: 1112061\nwchar: 126669\nsyscr: 90\n";
        let after = "rchar: 1112061\nwchar: 919115\nsyscr: 90\n";
        assert!(io_bytes(after) > io_bytes(before));
        assert_eq!(io_bytes(before), 1_112_061 + 126_669);
    }

    #[test]
    fn a_tree_with_another_users_process_counts_as_unmeasured() {
        let root_owned = std::fs::metadata("/proc/1").is_ok_and(|m| {
            use std::os::unix::fs::MetadataExt;
            m.uid() == 0
        });
        let we_are_root = std::fs::read_to_string("/proc/self/status").is_ok_and(|s| {
            s.lines()
                .any(|l| l.starts_with("Uid:") && l.split_whitespace().nth(1) == Some("0"))
        });
        if !root_owned || we_are_root {
            return; // needs an unprivileged user and a root init, as on any desktop and CI runner
        }
        assert!(
            super::tree_progress(1).1,
            "pid 1 runs as root: its io can't be read"
        );
        let me = std::process::id();
        assert!(!super::tree_progress(me).1, "our own process is measurable");
    }

    #[test]
    fn watched_command_runs_and_fails_like_a_command() {
        assert!(
            interactive_watched(Watchdog::default(), &["true"], Duration::from_millis(50)).is_ok()
        );
        assert!(
            interactive_watched(Watchdog::default(), &["false"], Duration::from_millis(50))
                .is_err()
        );
    }
}
