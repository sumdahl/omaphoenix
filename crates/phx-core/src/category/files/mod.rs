//! Shared file handling for categories: names, secret scanning, walks and copies.

mod copy;
mod names;
mod private;
mod scan;
#[cfg(test)]
mod tests;
mod walk;

use std::fs;
use std::path::Path;

pub use copy::{copy_file, copy_tree, same_file, set_reuse};
pub use names::{JUNK, SECRET_NAMES, is_junk, is_secret, match_any};
pub use private::{PRIVATE, apply_private, less_private, record_private};
pub use scan::{SECRET_SHAPE, has_secret_shape, is_binary};
pub use walk::{Walk, dir_size, has_files, walk};

/// Size cap under `~/.config`; anything larger is a database, binary or dump.
pub const MAX_CONFIG_FILE: u64 = 5 << 20;
/// Limit for plugin, theme, font and icon files.
pub const MAX_ASSET_FILE: u64 = 50 << 20;

/// `lstat` succeeds.
#[must_use]
pub fn exists(path: &Path) -> bool {
    fs::symlink_metadata(path).is_ok()
}
