use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

use super::config::model;
use super::prompts::SYSTEM;

/// The saved answer for `prompt`.
#[must_use]
pub fn cache_path(state: &Path, prompt: &str) -> PathBuf {
    let digest = Sha256::digest(format!("{}\0{SYSTEM}\0{prompt}", model()).as_bytes());
    let hex = digest.iter().fold(String::new(), |mut s, b| {
        let _ = write!(s, "{b:02x}");
        s
    });
    state
        .join("ai-cache")
        .join(format!("{}.md", hex.get(..24).unwrap_or_default()))
}
