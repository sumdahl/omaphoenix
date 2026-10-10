use std::io::Read;
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

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

/// Real commands: `LC_ALL=C`, no stdin, no stderr, bounded by [`TIMEOUT`].
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
        // ponytail: kills the command, not its children; use a process group if one ever forks.
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
