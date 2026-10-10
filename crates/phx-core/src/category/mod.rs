//! One implementation per kind of thing a snapshot holds: detect, capture, restore.

#![allow(
    clippy::case_sensitive_file_extension_comparisons,
    reason = "Linux file names are case-sensitive, and Go compared them the same way"
)]

mod bin;
mod config;
mod etc;
pub mod files;
mod gitdirs;
mod local;
mod pkg;
mod pkg_other;
pub mod secret;
mod service;

pub use bin::Bin;
pub use config::Config;
pub use etc::Etc;
pub use gitdirs::{GitDirs, plugins, themes};
pub use local::Local;
pub use pkg::{Pkg, pkg_aur, pkg_repo};
pub use pkg_other::PkgOther;
pub use secret::Secret;
pub use service::Service;

mod env;
mod json;
mod machine;
mod outcome;

use std::io;

use phx_contract::Item;

use crate::sys::{self, CmdError};

pub use env::{Env, matches_id};
pub use json::{read_json, write_json};
pub use machine::{MACHINE_SPECIFIC, hardware};
pub use outcome::{Change, Outcome, file_hash, hex};

/// Why a category step failed outright (per-item failures go in [`Outcome`]).
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum Error {
    /// Filesystem trouble.
    #[error(transparent)]
    Io(#[from] io::Error),
    /// A command failed.
    #[error(transparent)]
    Cmd(#[from] CmdError),
    /// A manifest file is broken.
    #[error(transparent)]
    Json(#[from] serde_json::Error),
    /// Anything else, already worded for the user.
    #[error("{0}")]
    Other(String),
}

/// A kind of thing a snapshot holds.
pub trait Category {
    /// Its name, as in `selection.conf` and the manifest.
    fn name(&self) -> &'static str;
    /// Candidate items on the live system.
    ///
    /// # Errors
    /// When detection can't run at all.
    fn detect(&self, env: &Env<'_>) -> Result<Vec<Item>, Error>;
    /// Writes the selected items into the snapshot tree.
    ///
    /// # Errors
    /// When capture fails; per-file problems are logged and skipped.
    fn capture(&self, env: &Env<'_>, items: &[Item]) -> Result<(), Error>;
    /// Applies the snapshot tree to the machine.
    ///
    /// # Errors
    /// When the stage can't run; per-item failures go in [`Outcome::failed`].
    fn restore(&self, env: &Env<'_>) -> Result<Outcome, Error>;
}

/// `YYYYmmdd-HHMMSS` in local time, for `.pre-restore.<stamp>` names.
#[must_use]
pub fn stamp() -> String {
    sys::output(None, &["date", "+%Y%m%d-%H%M%S"]).unwrap_or_else(|_| "restore".into())
}

/// Every category in restore order: packages, plugins/themes, config, then /etc before services.
#[must_use]
pub fn all() -> Vec<Box<dyn Category>> {
    vec![
        Box::new(pkg_repo()),
        Box::new(pkg_aur()),
        Box::new(plugins()),
        Box::new(themes()),
        Box::new(Config),
        Box::new(Local),
        Box::new(Bin),
        Box::new(PkgOther),
        Box::new(Secret),
        Box::new(Etc),
        Box::new(Service),
    ]
}
