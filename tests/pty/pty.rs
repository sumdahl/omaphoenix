//! Real-terminal tests, shared by the TUIs (included with `#[path]`): the
//! app runs under `script`, a real pty that answers no queries, so a
//! blocking terminal query shows up as a stall. Checks the time to the
//! first frame (DESIGN.md §12: < 100 ms in release builds), that no query
//! sequence is sent, that `q` quits, and that the terminal is restored.

use std::io::{Read, Write};
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};

/// Release builds must draw within the DESIGN.md budget; debug builds get
/// room for being unoptimised.
pub const BUDGET: Duration = if cfg!(debug_assertions) {
    Duration::from_millis(1500)
} else {
    Duration::from_millis(100)
};

/// Escape sequences that ask the terminal something and wait for the answer.
const QUERIES: [&str; 5] = ["\x1b[6n", "\x1b[c", "\x1b]10;?", "\x1b]11;?", "\x1b[?u"];

pub struct Run {
    pub first_frame: Duration,
    pub output: String,
    pub exited: bool,
}

/// Runs `cmd` (a shell command line) in a 100×30 pty with `home` as HOME,
/// waits for `marker` on screen, then types `quit`.
pub fn run(cmd: &str, home: &Path, marker: &str, quit: &[u8]) -> Run {
    let line = format!("stty cols 100 rows 30; exec {cmd}");
    let mut child = Command::new("script")
        .args(["-qfec", &line, "/dev/null"])
        .env("HOME", home)
        .env("XDG_STATE_HOME", home.join(".local/state"))
        .env("XDG_CONFIG_HOME", home.join(".config"))
        .env("XDG_DATA_HOME", home.join(".local/share"))
        .env("TERM", "xterm-256color")
        .env_remove("NO_COLOR")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap_or_else(|e| unreachable!("script: {e}"));
    let started = Instant::now();
    let mut stdout = child
        .stdout
        .take()
        .unwrap_or_else(|| unreachable!("stdout"));
    let (tx, rx) = mpsc::channel::<Vec<u8>>();
    std::thread::spawn(move || {
        let mut buf = [0u8; 4096];
        while let Ok(n) = stdout.read(&mut buf) {
            if n == 0 || tx.send(buf.get(..n).unwrap_or_default().to_vec()).is_err() {
                break;
            }
        }
    });
    let mut out = Vec::new();
    let mut first_frame = None;
    let deadline = started + Duration::from_secs(10);
    while Instant::now() < deadline {
        if let Ok(chunk) = rx.recv_timeout(Duration::from_millis(5)) {
            out.extend(chunk);
            if first_frame.is_none() && String::from_utf8_lossy(&out).contains(marker) {
                first_frame = Some(started.elapsed());
                let _ = child.stdin.as_mut().map(|i| i.write_all(quit));
            }
        }
        if first_frame.is_some() && child.try_wait().ok().flatten().is_some() {
            break;
        }
    }
    let exited = child.try_wait().ok().flatten().is_some_and(|s| s.success());
    if !exited {
        let _ = child.kill();
    }
    while let Ok(chunk) = rx.recv_timeout(Duration::from_millis(50)) {
        out.extend(chunk);
    }
    Run {
        first_frame: first_frame.unwrap_or(Duration::MAX),
        output: String::from_utf8_lossy(&out).into_owned(),
        exited,
    }
}

/// The checks every TUI must pass.
pub fn assert_well_behaved(r: &Run, name: &str) {
    assert!(
        r.first_frame < BUDGET,
        "{name}: first frame after {:?}, budget {BUDGET:?}",
        r.first_frame
    );
    for q in QUERIES {
        assert!(
            !r.output.contains(q),
            "{name} queried the terminal with {q:?}"
        );
    }
    assert!(r.exited, "{name}: q did not quit");
    assert!(
        r.output.contains("\x1b[?1049h") && r.output.contains("\x1b[?1049l"),
        "{name}: alternate screen not entered and left"
    );
    let shown = r.output.rfind("\x1b[?25h");
    let hidden = r.output.rfind("\x1b[?25l");
    assert!(shown > hidden, "{name}: cursor left hidden");
}
