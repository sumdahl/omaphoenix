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

// Unreadable processes (root under sudo) are skipped: the count only gets conservative.
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
    use super::{Watchdog, interactive_watched};
    use std::time::Duration;

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
