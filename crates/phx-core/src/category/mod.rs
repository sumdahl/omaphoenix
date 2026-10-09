//! One implementation per kind of thing a snapshot holds. Each category
//! detects candidates on the live system, captures the selected ones into
//! the snapshot tree, and restores them from an extracted tree.

#![allow(
    clippy::case_sensitive_file_extension_comparisons,
    reason = "Linux file names are case-sensitive, and Go compared them the same way"
)]

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::io;
use std::path::{Path, PathBuf};

use phx_contract::Item;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::sys::{self, CmdError, WatchError, Watchdog};

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

/// Where and how a category works.
pub struct Env<'a> {
    /// The user's home.
    pub home: PathBuf,
    /// Snapshot tree: written by capture, read by restore.
    pub tree: PathBuf,
    /// Plan only.
    pub dry_run: bool,
    /// `(level, message)`; level is `info`, `warn` or `step`.
    pub log: &'a dyn Fn(&str, &str),
    /// Asked only when encrypted secrets are restored.
    pub passphrase: Option<&'a dyn Fn() -> Result<zeroize::Zeroizing<String>, String>>,
    /// Restore only these item globs; empty means all.
    pub items: Vec<String>,
    /// The category being restored (for machine-specific patterns).
    pub category: String,
    /// Restoring onto different hardware than the snapshot's.
    pub foreign: bool,
    /// Hardware-specific patterns per category, skipped when foreign.
    pub machine: BTreeMap<String, Vec<String>>,
    /// Items skipped because they are hardware-specific.
    pub skipped: RefCell<Vec<String>>,
    /// Files the audit gate leaves out of a capture (absolute paths).
    pub excluded: std::collections::BTreeSet<PathBuf>,
    /// Services (`scope/unit`) the audit gate leaves out of a capture.
    pub excluded_units: std::collections::BTreeSet<String>,
}

impl<'a> Env<'a> {
    /// An environment for `home` and `tree`.
    pub fn new(home: &Path, tree: &Path, log: &'a dyn Fn(&str, &str)) -> Self {
        Self {
            home: home.to_path_buf(),
            tree: tree.to_path_buf(),
            dry_run: false,
            log,
            passphrase: None,
            items: Vec::new(),
            category: String::new(),
            foreign: false,
            machine: BTreeMap::new(),
            skipped: RefCell::new(Vec::new()),
            excluded: std::collections::BTreeSet::new(),
            excluded_units: std::collections::BTreeSet::new(),
        }
    }

    /// Whether the audit gate leaves `path` out, logging it when it does.
    pub fn audit_excludes(&self, path: &Path) -> bool {
        let out = self.excluded.contains(path);
        if out {
            self.info(&format!("left out by the audit: {}", path.display()));
        }
        out
    }

    /// Logs at `info`.
    pub fn info(&self, msg: &str) {
        (self.log)("info", msg);
    }

    /// Logs at `warn`.
    pub fn warn(&self, msg: &str) {
        (self.log)("warn", msg);
    }

    /// Whether item `id` is in scope for a partial restore. A pattern
    /// matches the id itself or any parent directory of it. On foreign
    /// hardware, hardware-specific items are skipped unless named.
    pub fn want(&self, id: &str) -> bool {
        let explicit = !self.items.is_empty() && matches_id(&self.items, id);
        if !self.items.is_empty() && !explicit {
            return false;
        }
        if self.foreign
            && !explicit
            && self
                .machine
                .get(&self.category)
                .is_some_and(|p| matches_id(p, id))
        {
            let mut skipped = self.skipped.borrow_mut();
            if !skipped.iter().any(|s| s == id) {
                skipped.push(id.to_owned());
            }
            return false;
        }
        true
    }
}

/// Whether a pattern matches `id` or one of its parent directories.
#[must_use]
pub fn matches_id(patterns: &[impl AsRef<str>], id: &str) -> bool {
    patterns.iter().any(|p| {
        let mut cur = id;
        loop {
            if cur.is_empty() || cur == "." || cur == "/" {
                return false;
            }
            if phx_contract::glob::path_match(p.as_ref(), cur) == Some(true) {
                return true;
            }
            cur = match cur.rfind('/') {
                Some(0) => "/",
                Some(i) => cur.get(..i).unwrap_or_default(),
                None => ".",
            };
        }
    })
}

/// One thing a restore did, recorded so `undo` can reverse it.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct Change {
    /// `file`, `dir`, `service` or `package`.
    pub kind: String,
    /// What changed: a path, `scope/unit`, or a package name.
    pub path: String,
    /// Where the previous file was moved; empty when the restore created it.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub backup: String,
    /// sha256 of what the restore wrote, so undo can spot later edits.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub hash: String,
    /// Reverting needs sudo.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub root: bool,
    /// For packages: the manager.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub note: String,
}

impl Change {
    fn new(kind: &str, path: &str) -> Self {
        Self {
            kind: kind.into(),
            path: path.into(),
            backup: String::new(),
            hash: String::new(),
            root: false,
            note: String::new(),
        }
    }
}

/// What one category's restore did. `failed` is non-empty when any part
/// did not apply, so the report never shows success it didn't have.
#[derive(Clone, Debug, Default)]
pub struct Outcome {
    /// Items applied.
    pub applied: usize,
    /// Items already as in the snapshot.
    pub present: usize,
    /// What failed, worded for the user.
    pub failed: Vec<String>,
    /// What a dry run would do.
    pub planned: Vec<String>,
    /// Stopped because the download crawled or stalled; rerun to continue.
    pub deferred: Vec<String>,
    /// What this restore changed, in order, for `undo`.
    pub changes: Vec<Change>,
}

impl Outcome {
    /// Records a change.
    pub fn changed(&mut self, kind: &str, path: &str, note: &str, root: bool) {
        let mut c = Change::new(kind, path);
        c.note = note.into();
        c.root = root;
        self.changes.push(c);
    }

    /// Records a written file, hashing it now so undo can tell whether the
    /// user edited it afterwards.
    pub fn wrote_file(&mut self, path: &Path, backup: Option<&Path>, root: bool) {
        let mut c = Change::new("file", &path.to_string_lossy());
        c.backup = backup
            .map(|b| b.to_string_lossy().into_owned())
            .unwrap_or_default();
        c.hash = file_hash(path);
        c.root = root;
        self.changes.push(c);
    }

    /// Runs one watched install and files the outcome: applied, deferred
    /// (too slow, try again later) or failed.
    pub fn install(&mut self, label: &str, argv: &[&str]) -> bool {
        match sys::interactive_watched(Watchdog::default(), argv, sys::WATCH_INTERVAL) {
            Ok(()) => {
                self.applied += 1;
                true
            }
            Err(e @ WatchError::Deferred { .. }) => {
                self.deferred.push(format!("{label}: {e}"));
                false
            }
            Err(e) => {
                self.failed.push(format!("{label}: {e}"));
                false
            }
        }
    }
}

/// sha256 of a file's content, or of a symlink's target text; empty when
/// unreadable. Streamed.
#[must_use]
pub fn file_hash(path: &Path) -> String {
    let mut h = Sha256::new();
    if let Ok(meta) = std::fs::symlink_metadata(path)
        && meta.file_type().is_symlink()
    {
        let Ok(t) = std::fs::read_link(path) else {
            return String::new();
        };
        h.update(t.as_os_str().as_encoded_bytes());
        return hex(&h.finalize());
    }
    let Ok(mut f) = std::fs::File::open(path) else {
        return String::new();
    };
    let mut buf = vec![0u8; 64 * 1024];
    loop {
        match io::Read::read(&mut f, &mut buf) {
            Ok(0) => return hex(&h.finalize()),
            Ok(n) => h.update(buf.get(..n).unwrap_or_default()),
            Err(_) => return String::new(),
        }
    }
}

/// Lowercase hex.
#[must_use]
pub fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write;
    bytes
        .iter()
        .fold(String::with_capacity(bytes.len() * 2), |mut s, b| {
            let _ = write!(s, "{b:02x}");
            s
        })
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

/// The machine model from firmware (DMI), readable without root, e.g.
/// "Dell Inc. Latitude 7430". Survives a reinstall and a rename. Empty
/// when unknown. `OMAPHOENIX_HARDWARE` overrides it for tests.
#[must_use]
pub fn hardware() -> String {
    if let Ok(h) = std::env::var("OMAPHOENIX_HARDWARE") {
        return h;
    }
    ["sys_vendor", "product_name", "product_version"]
        .iter()
        .filter_map(|f| std::fs::read_to_string(Path::new("/sys/class/dmi/id").join(f)).ok())
        .map(|s| s.trim().to_owned())
        .filter(|s| !s.is_empty() && !placeholder_dmi(s))
        .collect::<Vec<_>>()
        .join(" ")
}

/// Firmware fills unset DMI fields with filler text.
fn placeholder_dmi(s: &str) -> bool {
    let l = s.to_lowercase();
    l.contains("to be filled") || l == "default string" || l == "system product name" || l == "none"
}

/// Items that only make sense on the hardware they came from. Users add
/// more with `machine:` lines in `selection.conf`.
pub const MACHINE_SPECIFIC: &[(&str, &[&str])] = &[
    ("config", &[".config/hypr/monitors.*"]),
    (
        "local",
        &[
            "state/omarchy/toggles/hypr/internal-monitor-scale",
            "state/omarchy/toggles/hypr/touchpad-*",
            "state/omarchy/toggles/hypr/*touchpads*",
        ],
    ),
    ("etc", &["/etc/systemd/system/battery-threshold.service"]),
    (
        "service",
        &[
            "system/battery-threshold.service",
            "system/intel_lpmd.service",
            "system/thermald.service",
        ],
    ),
];

/// `YYYYmmdd-HHMMSS` in local time, for `.pre-restore.<stamp>` names.
#[must_use]
pub fn stamp() -> String {
    sys::output(None, &["date", "+%Y%m%d-%H%M%S"]).unwrap_or_else(|_| "restore".into())
}

/// Every category in restore order: packages before the plugins that need
/// them, plugins and themes before the config that references them, /etc
/// and config before the services whose unit files they bring back.
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

#[cfg(test)]
mod tests {
    use super::{Env, file_hash, matches_id};
    use std::collections::BTreeMap;

    #[test]
    fn matches_parents_like_go() {
        assert!(matches_id(&[".config/hypr"], ".config/hypr/bindings.lua"));
        assert!(matches_id(
            &[".config/hypr/monitors.*"],
            ".config/hypr/monitors.lua"
        ));
        assert!(!matches_id(&[".config/hypr"], ".config/hyprlock.conf"));
        assert!(matches_id(&["/etc/x"], "/etc/x/y"));
        assert!(!matches_id(&["*"], ""));
    }

    #[test]
    fn want_scopes_and_skips_hardware_items() {
        let log = |_: &str, _: &str| {};
        let mut env = Env::new("/h".as_ref(), "/t".as_ref(), &log);
        assert!(env.want("anything"));
        env.foreign = true;
        env.category = "config".into();
        env.machine = BTreeMap::from([(
            "config".to_owned(),
            vec![".config/hypr/monitors.*".to_owned()],
        )]);
        assert!(!env.want(".config/hypr/monitors.lua"));
        assert!(env.want(".config/hypr/bindings.lua"));
        assert_eq!(
            env.skipped.borrow().as_slice(),
            [".config/hypr/monitors.lua"]
        );
        env.items = vec![".config/hypr/monitors.lua".into()];
        assert!(
            env.want(".config/hypr/monitors.lua"),
            "named explicitly, so restored anyway"
        );
        assert!(!env.want(".config/hypr/bindings.lua"), "not named");
    }

    #[test]
    fn hashes_files_and_links() {
        let t = tempfile::tempdir().unwrap_or_else(|e| unreachable!("{e}"));
        let f = t.path().join("f");
        std::fs::write(&f, "abc").ok();
        assert_eq!(
            file_hash(&f),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        std::os::unix::fs::symlink("abc", t.path().join("l")).ok();
        assert_eq!(
            file_hash(&t.path().join("l")),
            file_hash(&f),
            "a link hashes its target text"
        );
        assert_eq!(file_hash(&t.path().join("missing")), "");
    }
}
