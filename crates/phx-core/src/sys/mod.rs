//! External commands: the one seam for every system interaction.

mod exec;
mod tools;
mod watchdog;
mod watched;

use std::io::Read;
use std::path::Path;

pub use exec::{CmdError, capture, command, interactive, lines, output, output_env, set_child_env};
pub use tools::{git_auth, has, pacman_owned, which};
pub use watchdog::{Verdict, Watchdog};
pub use watched::{WATCH_INTERVAL, WatchError, interactive_watched};

/// Reads at most `limit` bytes of a file.
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
