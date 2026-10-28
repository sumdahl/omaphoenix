use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use phx_contract::{Manifest, Rules};

use super::{App, AppError, RestoreOptions, StageReport};
use crate::category::{self, Env, MACHINE_SPECIFIC, Outcome};
use crate::store::Snapshot;
use crate::sys;

impl App<'_> {
    /// Applies a snapshot in category order; replaced files are kept as `.pre-restore`.
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
        category::files::apply_private(tree.path());
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
            // Saved per stage so a restore cut off midway can still be undone.
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
}
