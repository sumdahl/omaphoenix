//! `pkg-repo` and `pkg-aur`: explicit packages, restored with Omarchy's own installers.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use phx_contract::Item;

use super::{Category, Env, Error, Outcome};
use crate::sys;

/// A pacman package category.
pub struct Pkg {
    name: &'static str,
    query_flag: &'static str,
    installer: &'static str,
    repo: bool,
}

/// Explicit packages from the repos.
#[must_use]
pub fn pkg_repo() -> Pkg {
    Pkg {
        name: "pkg-repo",
        query_flag: "-Qqen",
        installer: "omarchy-pkg-add",
        repo: true,
    }
}

/// Explicit foreign (AUR) packages.
#[must_use]
pub fn pkg_aur() -> Pkg {
    Pkg {
        name: "pkg-aur",
        query_flag: "-Qqem",
        installer: "omarchy-pkg-aur-add",
        repo: false,
    }
}

const SYNC_DIR: &str = "/var/lib/pacman/sync";

impl Pkg {
    fn file(&self, env: &Env<'_>) -> PathBuf {
        env.tree.join("packages").join(format!("{}.txt", self.name))
    }

    fn available(&self, p: &str) -> bool {
        !self.repo || sys::output(None, &["pacman", "-Si", p]).is_ok()
    }
}

/// Whether every configured repo has a sync database (a fresh install has only offline.db).
fn db_synced() -> bool {
    match sys::lines(None, &["pacman-conf", "--repo-list"]) {
        Ok(repos) if !repos.is_empty() => repos
            .iter()
            .all(|r| Path::new(SYNC_DIR).join(format!("{r}.db")).exists()),
        _ => true, // can't tell; let pacman -Si speak for itself
    }
}

impl Category for Pkg {
    fn name(&self) -> &'static str {
        self.name
    }

    fn detect(&self, _: &Env<'_>) -> Result<Vec<Item>, Error> {
        if !sys::has("pacman") {
            return Ok(Vec::new());
        }
        match sys::lines(None, &["pacman", self.query_flag]) {
            Ok(names) => Ok(names.into_iter().map(|n| Item::new(n, "")).collect()),
            Err(e) if e.is_empty_query() => Ok(Vec::new()),
            Err(e) => Err(e.into()),
        }
    }

    fn capture(&self, env: &Env<'_>, items: &[Item]) -> Result<(), Error> {
        let mut names: Vec<&str> = items.iter().map(|i| i.id.as_str()).collect();
        names.sort_unstable();
        let file = self.file(env);
        if let Some(dir) = file.parent() {
            std::fs::create_dir_all(dir)?;
        }
        std::fs::write(&file, format!("{}\n", names.join("\n")))?;
        Ok(())
    }

    fn restore(&self, env: &Env<'_>) -> Result<Outcome, Error> {
        let mut res = Outcome::default();
        let wanted = match std::fs::read_to_string(self.file(env)) {
            Ok(t) => t,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(res),
            Err(e) => return Err(e.into()),
        };
        let have: BTreeSet<String> = sys::lines(None, &["pacman", "-Qq"])?.into_iter().collect();
        // No sync databases yet: update via `omarchy-update -y`, since Omarchy refuses a direct -Syu.
        let mut synced = db_synced();
        if !synced && self.repo {
            let update: &[&str] = if sys::has("omarchy-update") {
                &["omarchy-update", "-y"]
            } else {
                &["sudo", "pacman", "-Syu", "--noconfirm"]
            };
            if env.dry_run {
                res.planned.push(format!(
                    "bring the system up to date first ({}): this is a fresh install",
                    update.join(" ")
                ));
            } else if sys::interactive(update).is_err() || !db_synced() {
                res.failed.push(format!(
                    "{} did not complete; packages not restored",
                    update.join(" ")
                ));
                return Ok(res);
            } else {
                synced = true;
            }
        }
        let mut missing = Vec::new();
        for n in wanted.split_whitespace() {
            if !env.want(n) {
                continue;
            }
            if have.contains(n) {
                res.present += 1;
            } else if synced && !self.available(n) {
                res.failed.push(format!("{n} (not in the repos any more)"));
            } else {
                missing.push(n);
            }
        }
        if missing.is_empty() {
            return Ok(res);
        }
        if env.dry_run {
            res.planned.push(format!("install {}", missing.join(" ")));
            return Ok(res);
        }
        if !sys::has(self.installer) {
            res.failed
                .push(format!("{} not found — is this Omarchy?", self.installer));
            return Ok(res);
        }
        // Batch first; on failure go one by one so a bad package fails or defers alone.
        let mut batch = Outcome::default();
        let mut argv = vec![self.installer];
        argv.extend(missing.iter().copied());
        if batch.install(self.name, &argv) {
            res.applied += missing.len();
            for n in &missing {
                res.changed("package", n, self.name, false);
            }
            return Ok(res);
        }
        for n in missing {
            if sys::output(None, &["pacman", "-Q", n]).is_ok() {
                res.applied += 1; // the batch got this far before it stopped
                res.changed("package", n, self.name, false);
                continue;
            }
            if res.install(n, &[self.installer, n]) {
                res.changed("package", n, self.name, false);
            }
        }
        Ok(res)
    }
}
