use std::fs;
use std::path::Path;

use super::bundle::apply_work;
use super::git::git;
use super::{DirEntry, DirManifest, GitDirs, GitRef};
use crate::category::files::{MAX_ASSET_FILE, copy_tree, exists};
use crate::category::{Env, Error, Outcome, read_json};
use crate::sys::{self, CmdError, WatchError, Watchdog};

/// Clones under the watchdog; a stopped clone leaves no partial directory behind.
fn clone_at(r: &GitRef, target: &Path) -> Result<(), WatchError> {
    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent).map_err(|e| CmdError {
            cmd: "mkdir".into(),
            status: e.to_string(),
            stderr: String::new(),
        })?;
    }
    let target_s = target.to_string_lossy();
    let mut argv = vec!["git"];
    argv.extend_from_slice(sys::git_auth());
    argv.extend_from_slice(&["clone", "-q", "--", &r.url, &target_s]);
    if let Err(e) = sys::interactive_watched(Watchdog::default(), &argv, sys::WATCH_INTERVAL) {
        let _ = fs::remove_dir_all(target);
        return Err(e);
    }
    if r.bundle.is_empty() {
        git(target, &["checkout", "-q", &r.commit])?;
        return Ok(());
    }
    apply_work(target, r).map_err(|e| {
        CmdError {
            cmd: "apply bundle".into(),
            status: e.to_string(),
            stderr: String::new(),
        }
        .into()
    })
}

/// A failure worded like a command's, for the restore report.
fn failure(what: &str, why: impl std::fmt::Display) -> WatchError {
    CmdError {
        cmd: what.to_owned(),
        status: why.to_string(),
        stderr: String::new(),
    }
    .into()
}

impl GitDirs {
    /// Installs like Omarchy's installers do, pinned to the snapshot's commit, without a running shell.
    fn clone_entry(&self, r: &GitRef, target: &Path) -> Result<(), WatchError> {
        if sys::has("omarchy-git-url-check") {
            sys::output(None, &["omarchy-git-url-check", &r.url])?;
        }
        let name = target
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        let stage = target.with_file_name(format!(".omaphoenix.{name}"));
        let _ = fs::remove_dir_all(&stage);
        clone_at(r, &stage)?;
        if self.name == "plugin"
            && sys::has("omarchy-plugin-validate")
            && let Err(e) =
                sys::output(None, &["omarchy-plugin-validate", &stage.to_string_lossy()])
        {
            let _ = fs::remove_dir_all(&stage);
            return Err(e.into());
        }
        fs::rename(&stage, target).map_err(|e| {
            CmdError {
                cmd: "rename".into(),
                status: e.to_string(),
                stderr: String::new(),
            }
            .into()
        })
    }

    fn set_active(env: &Env<'_>, name: &str, res: &mut Outcome) {
        if name.is_empty() {
            return;
        }
        let current = fs::read_to_string(env.home.join(".local/state/omarchy/current/theme.name"))
            .unwrap_or_default();
        if current.trim() == name {
            return;
        }
        if env.dry_run {
            res.planned.push(format!("set active theme {name}"));
        } else if !sys::has("omarchy-theme-set") {
            res.failed.push(format!(
                "omarchy-theme-set not found; active theme {name} not applied"
            ));
        } else if let Err(e) = sys::output(None, &["omarchy-theme-set", name]) {
            res.failed.push(format!("set theme {name}: {e}"));
        }
    }

    /// Installs one entry by kind: a clone (with bundle), a copy out of the themes-repo clone, or a copy.
    fn install_entry(
        &self,
        env: &Env<'_>,
        m: &DirManifest,
        e: &DirEntry,
        target: &Path,
        root_clone: &mut Option<Result<tempfile::TempDir, String>>,
    ) -> Result<(), WatchError> {
        let copy = |src: &Path| {
            copy_tree(env, src, target, MAX_ASSET_FILE, &|_, _, _| false)
                .map(|_| ())
                .map_err(|err| failure("copy", err))
        };
        match e.kind.as_str() {
            "git" | "bundle" => {
                let mut r = e
                    .git_ref
                    .clone()
                    .ok_or_else(|| failure(&e.id, "manifest has no ref"))?;
                if !r.bundle.is_empty() {
                    r.bundle = env.tree.join(&r.bundle).to_string_lossy().into_owned();
                }
                self.clone_entry(&r, target)
            }
            "root" => {
                let clone = root_clone.get_or_insert_with(|| {
                    let dir = tempfile::Builder::new()
                        .prefix("omaphoenix-root-")
                        .tempdir()
                        .map_err(|e| e.to_string())?;
                    let r = m.root.clone().ok_or("manifest has no root ref")?;
                    clone_at(&r, &dir.path().join("r")).map_err(|e| e.to_string())?;
                    Ok(dir)
                });
                match clone {
                    Ok(dir) => copy(&dir.path().join("r").join(&e.id)),
                    Err(err) => Err(failure("clone themes repo", err.as_str())),
                }
            }
            _ => copy(&self.copy_dir(env, &e.id)),
        }
    }

    pub(super) fn restore_items(&self, env: &Env<'_>) -> Result<Outcome, Error> {
        let mut res = Outcome::default();
        let Some(m): Option<DirManifest> = read_json(&self.manifest_file(env))? else {
            return Ok(res);
        };
        let base = env.home.join(self.rel);
        let mut root_clone: Option<Result<tempfile::TempDir, String>> = None;
        let mut installed = 0;
        for e in &m.entries {
            if !env.want(&e.id) {
                continue;
            }
            let target = base.join(&e.id);
            if exists(&target) {
                res.present += 1;
                continue;
            }
            if env.dry_run {
                res.planned
                    .push(format!("{} {} ({})", self.name, e.id, e.kind));
                continue;
            }
            let result = self.install_entry(env, &m, e, &target, &mut root_clone);
            match result {
                Ok(()) => {
                    res.changed("dir", &target.to_string_lossy(), "", false);
                    res.applied += 1;
                    installed += 1;
                }
                Err(err @ WatchError::Deferred { .. }) => {
                    res.deferred.push(format!("{}: {err}", e.id));
                }
                Err(err) => res.failed.push(format!("{}: {err}", e.id)),
            }
        }
        if installed > 0
            && self.name == "plugin"
            && sys::has("omarchy-shell")
            && let Err(e) = sys::output(None, &["omarchy-shell", "shell", "rescanPlugins"])
        {
            env.warn(&format!("plugins are installed but the Omarchy shell did not rescan ({e}); they load on its next start"));
        }
        if env.items.is_empty() {
            Self::set_active(env, &m.active, &mut res);
        }
        Ok(res)
    }
}
