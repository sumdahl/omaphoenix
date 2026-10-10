//! Orchestration for front ends: snapshot, restore, undo journal.

mod choices;
mod journal;
mod reaudit;
mod restore;
mod review;
mod snapshot;

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use phx_contract::{Rules, selection};

use crate::category::{self, Error, Outcome};
use crate::store::{Repo, StoreError};
use crate::sys;

pub use journal::{Deferred, Journal};
pub use review::{REVIEWED, key as review_key};

/// A failure of a whole operation.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum AppError {
    /// No repo yet.
    #[error("not initialised; run: omaphoenix init")]
    NotInitialised,
    /// From the snapshot store.
    #[error(transparent)]
    Store(#[from] StoreError),
    /// From a category.
    #[error(transparent)]
    Category(#[from] Error),
    /// Filesystem trouble.
    #[error(transparent)]
    Io(#[from] std::io::Error),
    /// Anything else, worded for the user.
    #[error("{0}")]
    Other(String),
}

/// The engine bound to one user and repo.
pub struct App<'a> {
    /// The user's home.
    pub home: PathBuf,
    /// The snapshot repo.
    pub repo: Repo,
    /// `(level, message)`.
    pub log: &'a dyn Fn(&str, &str),
    /// Asked only when a restore meets encrypted secrets.
    pub passphrase: Option<&'a dyn Fn() -> Result<zeroize::Zeroizing<String>, String>>,
}

/// What one restore stage did.
#[derive(Debug)]
pub struct StageReport {
    /// Category name.
    pub category: &'static str,
    /// Its outcome.
    pub result: Outcome,
    /// The stage failed outright.
    pub err: Option<String>,
    /// Hardware-specific items skipped on different hardware.
    pub skipped: Vec<String>,
}

impl StageReport {
    /// Nothing failed.
    #[must_use]
    pub fn ok(&self) -> bool {
        self.err.is_none() && self.result.failed.is_empty()
    }
}

/// How to restore.
#[derive(Clone, Debug, Default)]
pub struct RestoreOptions {
    /// Snapshot id or timestamp prefix; empty for the latest.
    pub at: String,
    /// Plan only.
    pub dry_run: bool,
    /// Don't restart the Omarchy shell afterwards.
    pub no_restart: bool,
    /// Restore hardware-specific items onto different hardware too.
    pub all_hardware: bool,
    /// Only these categories.
    pub only: Vec<String>,
    /// Only these item globs.
    pub items: Vec<String>,
}

/// `$OMAPHOENIX_REPO`, else `$XDG_DATA_HOME/omaphoenix/repo`.
#[must_use]
pub fn repo_dir(home: &Path) -> PathBuf {
    if let Some(dir) = std::env::var_os("OMAPHOENIX_REPO").filter(|d| !d.is_empty()) {
        return PathBuf::from(dir);
    }
    let data = std::env::var_os("XDG_DATA_HOME")
        .filter(|d| !d.is_empty())
        .map_or_else(|| home.join(".local/share"), PathBuf::from);
    data.join("omaphoenix/repo")
}

fn rfc3339_now() -> String {
    sys::output(None, &["date", "-Iseconds"]).unwrap_or_default()
}

impl<'a> App<'a> {
    /// The app for `$HOME`.
    ///
    /// # Errors
    /// When `$HOME` is unset.
    pub fn from_env(log: &'a dyn Fn(&str, &str)) -> Result<Self, AppError> {
        let home = std::env::var_os("HOME")
            .map(PathBuf::from)
            .ok_or_else(|| AppError::Other("HOME is not set".into()))?;
        let repo = Repo {
            dir: repo_dir(&home),
        };
        Ok(Self {
            home,
            repo,
            log,
            passphrase: None,
        })
    }

    fn selection_file(&self) -> PathBuf {
        self.repo.dir.join("selection.conf")
    }

    /// `$XDG_STATE_HOME/omaphoenix`.
    #[must_use]
    pub fn state_dir(&self) -> PathBuf {
        std::env::var_os("XDG_STATE_HOME")
            .filter(|d| !d.is_empty())
            .map_or_else(|| self.home.join(".local/state"), PathBuf::from)
            .join("omaphoenix")
    }

    /// The selection rules (the default when there's no file).
    ///
    /// # Errors
    /// When the file exists but can't be read.
    pub fn rules(&self) -> Result<Rules, AppError> {
        match fs::read_to_string(self.selection_file()) {
            Ok(t) => Ok(Rules::parse(&t)),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                Ok(Rules::parse(selection::DEFAULT))
            }
            Err(e) => Err(e.into()),
        }
    }

    /// Creates the repo and its default selection.
    ///
    /// # Errors
    /// When already initialised or git fails.
    pub fn init(&self, remote: &str) -> Result<(), AppError> {
        if self.repo.exists() {
            return Err(AppError::Other(format!(
                "already initialised at {}",
                self.repo.dir.display()
            )));
        }
        self.repo.init(remote)?;
        fs::write(self.selection_file(), selection::DEFAULT)?;
        Ok(())
    }

    /// Creates the vault key pair and opts `~/.ssh` into snapshots.
    ///
    /// # Errors
    /// When not initialised, already set up, or on I/O failure.
    pub fn init_secrets(&self, passphrase: &str) -> Result<(), AppError> {
        if !self.repo.exists() {
            return Err(AppError::NotInitialised);
        }
        category::secret::setup_secrets(
            &self.repo.dir.join(category::secret::SECRETS_DIR),
            passphrase,
        )?;
        let rules = self.rules()?;
        let keys = category::secret::private_ssh_keys(&self.home);
        if !keys.is_empty() && keys.iter().all(|k| rules.selected("secret", k)) {
            return Ok(());
        }
        let mut f = fs::File::options()
            .append(true)
            .create(true)
            .open(self.selection_file())?;
        f.write_all(b"secret:.ssh/*\n")?;
        Ok(())
    }
}
