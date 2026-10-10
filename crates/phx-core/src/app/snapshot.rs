use std::fs;

use phx_contract::{Item, Manifest, Rules};

use super::{App, AppError, rfc3339_now};
use crate::category::{self, Env, read_json, write_json};
use crate::store::Snapshot;
use crate::sys;

impl App<'_> {
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

    /// Recaptures every selected item and commits if anything changed; `None` otherwise.
    ///
    /// # Errors
    /// When not initialised, locked, or a category fails.
    pub fn snapshot(&self) -> Result<Option<Snapshot>, AppError> {
        if !self.repo.exists() {
            return Err(AppError::NotInitialised);
        }
        let lock = self.repo.lock()?;
        for f in &lock.cleared {
            (self.log)(
                "warn",
                &format!("removed {}, left by an interrupted run", f.display()),
            );
        }
        let rules = self.rules()?;
        // Recapture from scratch; `copy_file` moves unchanged files back from `prev`.
        let prev = self.repo.dir.join(".git/omaphoenix-prev");
        if prev.exists() {
            fs::remove_dir_all(&prev)?;
        }
        fs::create_dir_all(&prev)?;
        for e in fs::read_dir(&self.repo.dir)?.flatten() {
            let name = e.file_name();
            if [
                ".git",
                "selection.conf",
                super::REVIEWED,
                category::secret::SECRETS_DIR,
            ]
            .iter()
            .any(|k| name == *k)
            {
                continue;
            }
            fs::rename(e.path(), prev.join(&name))?;
        }
        let before: Option<Manifest> = read_json(&prev.join("manifest.json")).ok().flatten();
        category::files::set_reuse(Some((self.repo.dir.clone(), prev.clone())));
        let captured = self.capture_into(&rules);
        category::files::set_reuse(None);
        let _ = fs::remove_dir_all(&prev);
        let m = captured?;
        write_json(&self.repo.dir.join("manifest.json"), &m)?;
        (self.log)("step", "saving the snapshot");
        let message = super::summary::commit_message(&rfc3339_now(), before.as_ref(), &m);
        if !self.repo.commit(&message)? {
            // Nothing new, but retry a push an earlier run failed.
            if self.repo.unpushed()
                && let Err(e) = self.repo.push()
            {
                (self.log)(
                    "warn",
                    &format!("push failed, snapshot kept locally and pushed next run: {e}"),
                );
            }
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

    fn capture_into(&self, rules: &Rules) -> Result<Manifest, AppError> {
        let mut env = Env::new(&self.home, &self.repo.dir, self.log);
        let mut m = Self::manifest_for_machine();
        if crate::gate::enabled() {
            (self.log)("step", "auditing this machine first (quick, read-only)");
            let gate = crate::gate::run(&self.home, Some(&self.state_dir()), self.log);
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
        Ok(m)
    }
}
