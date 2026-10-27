use std::fs::File;
use std::io::{BufRead, BufReader, Read};
use std::path::Path;
use std::sync::LazyLock;

use regex::bytes::Regex;

/// Token shapes, matched on bytes as Bash's `grep -aE` under `LC_ALL=C` does.
pub(super) static SECRET_RE: LazyLock<Option<Regex>> = LazyLock::new(|| {
    Regex::new(
        r"(?-u)(AKIA[0-9A-Z]{16}|ghp_[A-Za-z0-9]{36}|github_pat_[A-Za-z0-9_]{40,}|glpat-[A-Za-z0-9_-]{20}|sk-ant-[A-Za-z0-9_-]{20,}|sk-[A-Za-z0-9]{40,}|xox[baprs]-[A-Za-z0-9-]{10,}|-----BEGIN [A-Z ]*PRIVATE KEY-----|(password|passwd|secret|token)=[^ ]{6,})",
    )
    .ok()
});

/// Longest history line examined; the rest of a longer line is skipped.
const MAX_LINE: u64 = 64 * 1024;

/// Lines that look like secrets, streamed with a bounded line buffer.
pub(super) fn count_secret_lines(path: &Path) -> std::io::Result<usize> {
    let Some(re) = SECRET_RE.as_ref() else {
        return Ok(0);
    };
    let mut reader = BufReader::new(File::open(path)?);
    let mut line = Vec::new();
    let mut n = 0;
    loop {
        line.clear();
        let read = reader
            .by_ref()
            .take(MAX_LINE)
            .read_until(b'\n', &mut line)?;
        if read == 0 {
            return Ok(n);
        }
        if re.is_match(&line) {
            n += 1;
        }
        if line.last() != Some(&b'\n') && read as u64 == MAX_LINE {
            reader.skip_until(b'\n')?;
        }
    }
}
