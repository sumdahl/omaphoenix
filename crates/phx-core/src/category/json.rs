use std::io;
use std::path::Path;

use serde::{Deserialize, Serialize};

use super::Error;

/// Writes pretty JSON with a trailing newline, as Go's `WriteJSON` does.
///
/// # Errors
/// On I/O or serialisation failure.
pub fn write_json(file: &Path, v: &impl Serialize) -> Result<(), Error> {
    if let Some(dir) = file.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let mut body = serde_json::to_vec_pretty(v)?;
    body.push(b'\n');
    std::fs::write(file, body)?;
    Ok(())
}

/// Reads JSON; `Ok(None)` when the file doesn't exist.
///
/// # Errors
/// On I/O or parse failure.
pub fn read_json<T: for<'de> Deserialize<'de>>(file: &Path) -> Result<Option<T>, Error> {
    match std::fs::File::open(file) {
        Ok(f) => Ok(Some(serde_json::from_reader(io::BufReader::new(f))?)),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e.into()),
    }
}
