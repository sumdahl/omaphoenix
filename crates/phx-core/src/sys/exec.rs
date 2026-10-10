use std::path::Path;
use std::process::{Command, ExitStatus, Stdio};

// A registry instead of `std::env::set_var`, which is unsafe in edition 2024.
static CHILD_ENV: std::sync::Mutex<Vec<(String, String)>> = std::sync::Mutex::new(Vec::new());

/// Adds a variable for every command run through this module.
pub fn set_child_env(key: &str, value: &str) {
    if let Ok(mut env) = CHILD_ENV.lock() {
        env.retain(|(k, _)| k != key);
        env.push((key.to_owned(), value.to_owned()));
    }
}

/// A `Command` with the child environment applied; use instead of `Command::new`.
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
    /// The command line.
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

pub(super) fn describe(status: ExitStatus) -> String {
    status.code().map_or_else(
        || "killed by a signal".to_owned(),
        |c| format!("exit status {c}"),
    )
}

/// Runs a command in `dir` with extra environment and returns trimmed stdout.
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

/// `(succeeded, stdout)` whatever the exit status; `None` when it can't start.
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

/// Runs a command attached to the terminal, for installers that prompt.
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
