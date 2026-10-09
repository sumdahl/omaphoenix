//! Orchestration: snapshot, restore, undo journal. Front ends (CLI, TUI,
//! desktop) pass a log function and render the results.

use std::collections::BTreeMap;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use phx_contract::{Item, Manifest, Rules, selection};
use serde::{Deserialize, Serialize};

use crate::category::{self, Change, Env, Error, MACHINE_SPECIFIC, Outcome, read_json, write_json};
use crate::store::{Repo, Snapshot, StoreError};
use crate::sys;

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

/// Everything the last real restore changed, in order.
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct Journal {
    /// Snapshot id.
    pub snapshot: String,
    /// RFC 3339 time.
    pub time: String,
    /// The changes.
    #[serde(default)]
    pub changes: Vec<Change>,
}

/// What the last restore deferred for slow downloads.
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct Deferred {
    /// Snapshot id.
    pub snapshot: String,
    /// `"<category> <item>: <why>"`.
    #[serde(default)]
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

    fn manifest_for_machine() -> Manifest {
        let mut m = Manifest::new();
        m.omarchy_version = sys::output(None, &["omarchy", "version"]).unwrap_or_default();
        fs::read_to_string("/proc/sys/kernel/hostname")
            .unwrap_or_default()
            .trim()
            .clone_into(&mut m.host);
        m.hardware = category::hardware();
        m.user = std::env::var("USER").unwrap_or_default();
        if let Ok(line) = sys::output(None, &["getent", "passwd", &m.user]) {
            line.rsplit(':')
                .next()
                .unwrap_or_default()
                .clone_into(&mut m.shell);
        }
        m
    }

    /// Recaptures every selected item from scratch so deletions show, then
    /// commits only if something changed. `None` when nothing changed.
    ///
    /// # Errors
    /// When not initialised, locked, or a category fails.
    pub fn snapshot(&self) -> Result<Option<Snapshot>, AppError> {
        if !self.repo.exists() {
            return Err(AppError::NotInitialised);
        }
        let _lock = self.repo.lock()?;
        let rules = self.rules()?;
        // Recapture from scratch so removed items disappear. The selection
        // and the secrets key pair are user state, not captured output.
        for e in fs::read_dir(&self.repo.dir)?.flatten() {
            let name = e.file_name();
            if [".git", "selection.conf", category::secret::SECRETS_DIR]
                .iter()
                .any(|k| name == *k)
            {
                continue;
            }
            let path = e.path();
            if e.file_type()?.is_dir() {
                fs::remove_dir_all(&path)?;
            } else {
                fs::remove_file(&path)?;
            }
        }
        let mut env = Env::new(&self.home, &self.repo.dir, self.log);
        let mut m = Self::manifest_for_machine();
        if crate::gate::enabled() {
            let gate = crate::gate::run(&self.home, self.log);
            env.excluded = gate.excluded_paths;
            env.excluded_units = gate.excluded_units;
            m.audit = gate.summary;
        }
        for c in category::all() {
            let found = c
                .detect(&env)
                .map_err(|e| AppError::Other(format!("{}: detect: {e}", c.name())))?;
            let picked: Vec<Item> = found
                .iter()
                .filter(|it| rules.selected(c.name(), &it.id))
                .cloned()
                .collect();
            c.capture(&env, &picked)
                .map_err(|e| AppError::Other(format!("{}: capture: {e}", c.name())))?;
            (self.log)(
                "info",
                &format!(
                    "{:<9} {} captured ({} found)",
                    c.name(),
                    picked.len(),
                    found.len()
                ),
            );
            m.items.insert(c.name().to_owned(), picked);
        }
        write_json(&self.repo.dir.join("manifest.json"), &m)?;
        if !self.repo.commit(&format!("snapshot {}", rfc3339_now()))? {
            return Ok(None);
        }
        if let Err(e) = self.repo.push() {
            (self.log)(
                "warn",
                &format!("push failed, snapshot kept locally and pushed next run: {e}"),
            );
        }
        Ok(Some(self.repo.resolve("")?))
    }

    /// Applies a snapshot in category order and reports every stage. Never
    /// deletes: files it replaces are kept as `.pre-restore`.
    ///
    /// # Errors
    /// When the snapshot can't be found, extracted or read.
    pub fn restore(&self, opt: &RestoreOptions) -> Result<(Snapshot, Vec<StageReport>), AppError> {
        let snap = self.repo.resolve(&opt.at)?;
        let tree = tempfile::Builder::new()
            .prefix("omaphoenix-restore-")
            .tempdir()?;
        self.repo
            .extract(&snap.id, tree.path())
            .map_err(|e| AppError::Other(format!("extract {}: {e}", snap.id)))?;
        let text = fs::read_to_string(tree.path().join("manifest.json"))?;
        let m =
            Manifest::parse(&text).map_err(|e| AppError::Other(format!("read manifest: {e}")))?;
        self.warn_mismatch(&m, opt.dry_run);
        let mut env = Env::new(&self.home, tree.path(), self.log);
        env.dry_run = opt.dry_run;
        env.passphrase = self.passphrase;
        env.items.clone_from(&opt.items);
        if !opt.all_hardware
            && let Some(machine) = self.foreign_hardware(&m, tree.path())
        {
            env.foreign = true;
            env.machine = machine;
        }
        let wanted = |name: &str| opt.only.is_empty() || opt.only.iter().any(|o| o == name);
        let mut reports = Vec::new();
        for c in category::all() {
            if !wanted(c.name()) {
                continue;
            }
            (self.log)("step", c.name());
            c.name().clone_into(&mut env.category);
            env.skipped.borrow_mut().clear();
            let (result, err) = match c.restore(&env) {
                Ok(r) => (r, None),
                Err(e) => (Outcome::default(), Some(e.to_string())),
            };
            let skipped = env.skipped.borrow().clone();
            reports.push(StageReport {
                category: c.name(),
                result,
                err,
                skipped,
            });
            // Saved after every stage, so a restore cut off midway can still
            // be undone up to where it got.
            if !opt.dry_run {
                self.save_journal(&snap, &reports);
            }
        }
        if !opt.dry_run
            && !opt.no_restart
            && sys::has("omarchy")
            && (wanted("plugin") || wanted("config"))
            && let Err(e) = sys::output(None, &["omarchy", "restart", "shell"])
        {
            (self.log)("warn", &format!("could not restart the Omarchy shell: {e}"));
        }
        if !opt.dry_run {
            self.save_deferred(&snap, &reports);
        }
        Ok((snap, reports))
    }

    /// Warns about an Omarchy version or user name that differs from the
    /// snapshot's, and (dry run) lists what the audit left out.
    fn warn_mismatch(&self, m: &Manifest, dry_run: bool) {
        let here_version = sys::output(None, &["omarchy", "version"]).unwrap_or_default();
        if !m.omarchy_version.is_empty() && here_version != m.omarchy_version {
            (self.log)(
                "warn",
                &format!(
                    "snapshot was taken on Omarchy {}, this machine runs {here_version}; consider `omarchy update` first",
                    m.omarchy_version
                ),
            );
        }
        let me = std::env::var("USER").unwrap_or_default();
        if !m.user.is_empty() && m.user != me {
            (self.log)(
                "warn",
                &format!(
                    "snapshot was taken as user {:?}, this is {me:?}: paths saved as /home/{}/… in configs and symlinks will point at the wrong home; reinstall with the same username for an exact restore",
                    m.user, m.user
                ),
            );
        }
        if dry_run {
            for g in m.audit.iter().flat_map(|a| &a.excluded) {
                (self.log)(
                    "info",
                    &format!(
                        "left out of this snapshot by the audit: {} ({})",
                        g.item, g.id
                    ),
                );
            }
        }
    }

    /// On different hardware than the snapshot's: the hardware-specific
    /// patterns to skip, built-in plus the snapshot's own `machine:` rules.
    fn foreign_hardware(&self, m: &Manifest, tree: &Path) -> Option<BTreeMap<String, Vec<String>>> {
        let here = category::hardware();
        if m.hardware.is_empty() || here.is_empty() || m.hardware == here {
            return None;
        }
        let mut machine: BTreeMap<String, Vec<String>> = BTreeMap::new();
        for (cat, pats) in MACHINE_SPECIFIC {
            machine
                .entry((*cat).to_owned())
                .or_default()
                .extend(pats.iter().map(|p| (*p).to_owned()));
        }
        if let Ok(t) = fs::read_to_string(tree.join("selection.conf")) {
            for (cat, pats) in Rules::parse(&t).machine() {
                machine
                    .entry(cat.to_owned())
                    .or_default()
                    .extend(pats.iter().map(|p| (*p).to_owned()));
            }
        }
        (self.log)(
            "warn",
            &format!(
                "snapshot is from a {}, this is a {here}: hardware-specific settings (monitor layout, touchpad, battery limits…) are skipped; pass --all-hardware or name an item to restore one anyway",
                m.hardware
            ),
        );
        Some(machine)
    }

    fn journal_file(&self) -> PathBuf {
        self.state_dir().join("last-restore.json")
    }

    fn deferred_file(&self) -> PathBuf {
        self.state_dir().join("deferred.json")
    }

    /// Records this restore's changes for `undo`. A restore that changed
    /// nothing (a re-run, e.g. to finish deferred downloads) keeps the
    /// earlier journal, so the real restore can still be undone. Go
    /// replaced it with an empty one (Go repo docs/10, K2).
    fn save_journal(&self, snap: &Snapshot, reports: &[StageReport]) {
        let changes: Vec<Change> = reports
            .iter()
            .flat_map(|r| r.result.changes.iter().cloned())
            .collect();
        if changes.is_empty() {
            return;
        }
        let j = Journal {
            snapshot: snap.id.clone(),
            time: rfc3339_now(),
            changes,
        };
        if let Err(e) = write_json(&self.journal_file(), &j) {
            (self.log)(
                "warn",
                &format!("could not record this restore for undo: {e}"),
            );
        }
    }

    fn save_deferred(&self, snap: &Snapshot, reports: &[StageReport]) {
        let items: Vec<String> = reports
            .iter()
            .flat_map(|r| {
                r.result
                    .deferred
                    .iter()
                    .map(move |d| format!("{} {d}", r.category))
            })
            .collect();
        if items.is_empty() {
            let _ = fs::remove_file(self.deferred_file());
            return;
        }
        if let Err(e) = write_json(
            &self.deferred_file(),
            &Deferred {
                snapshot: snap.id.clone(),
                items,
            },
        ) {
            (self.log)("warn", &format!("could not record deferred items: {e}"));
        }
    }

    /// What the last restore deferred, if anything.
    #[must_use]
    pub fn last_deferred(&self) -> Option<Deferred> {
        read_json::<Deferred>(&self.deferred_file())
            .ok()
            .flatten()
            .filter(|d| !d.items.is_empty())
    }

    /// The last restore's journal.
    ///
    /// # Errors
    /// When none is recorded or it can't be read.
    pub fn journal(&self) -> Result<Journal, AppError> {
        read_json(&self.journal_file())?.ok_or_else(|| {
            AppError::Other("nothing to undo: no restore has been recorded on this machine".into())
        })
    }

    /// Marks the journal done, keeping it under a name undo won't read again.
    pub fn retire_journal(&self) {
        let stamp = category::stamp();
        let _ = fs::rename(
            self.journal_file(),
            self.state_dir()
                .join(format!("last-restore.undone-{stamp}.json")),
        );
    }

    /// Every category's detected items, ticked as the current rules select
    /// them, with each item's note ("bundle", "binary"…). Reads only.
    ///
    /// # Errors
    /// When not initialised or detection fails.
    pub fn choices(&self) -> Result<Vec<(selection::Choice, Vec<String>)>, AppError> {
        if !self.repo.exists() {
            return Err(AppError::NotInitialised);
        }
        let rules = self.rules()?;
        let quiet = |_: &str, _: &str| {};
        let env = Env::new(&self.home, &self.repo.dir, &quiet);
        let mut out = Vec::new();
        for c in category::all() {
            let items = c
                .detect(&env)
                .map_err(|e| AppError::Other(format!("{}: {e}", c.name())))?;
            let notes = items.iter().map(|i| i.note.clone()).collect();
            let choice = selection::Choice {
                category: c.name().to_owned(),
                items: items
                    .into_iter()
                    .map(|i| {
                        let on = rules.selected(c.name(), &i.id);
                        (i.id, on)
                    })
                    .collect(),
            };
            out.push((choice, notes));
        }
        Ok(out)
    }

    /// Rewrites `selection.conf` to capture exactly what is ticked.
    ///
    /// # Errors
    /// On I/O failure.
    pub fn save_choices(&self, choices: &[selection::Choice]) -> Result<(), AppError> {
        let tmp = self.repo.dir.join("selection.conf.tmp");
        fs::write(&tmp, selection::build(choices))?;
        fs::rename(tmp, self.selection_file())?;
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
        if self.rules()?.selected("secret", ".ssh/id_x") {
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
