use super::{Repo, Snapshot, StoreError};
use crate::sys;

impl Repo {
    // The user's own identity when they have one, so a personal backup repo shows only its owner.
    fn identity(&self) -> &'static [&'static str] {
        if self
            .git(&["config", "user.email"])
            .is_ok_and(|e| !e.is_empty())
        {
            &[]
        } else {
            &[
                "-c",
                "user.name=omaphoenix",
                "-c",
                "user.email=omaphoenix@localhost",
            ]
        }
    }

    /// Stages everything and commits only if the tree changed.
    ///
    /// # Errors
    /// When git fails.
    pub fn commit(&self, msg: &str) -> Result<bool, StoreError> {
        self.git(&["add", "-A"])?;
        if self.git(&["diff", "--cached", "--quiet"]).is_ok() {
            return Ok(false);
        }
        let mut args = self.identity().to_vec();
        args.extend_from_slice(&["commit", "-q", "-m", msg]);
        self.git(&args)?;
        Ok(true)
    }

    /// Pushes to `origin` if there is one.
    ///
    /// # Errors
    /// When the push fails.
    pub fn push(&self) -> Result<(), StoreError> {
        if self.remote().is_empty() {
            return Ok(());
        }
        let mut args = sys::git_auth().to_vec();
        args.extend_from_slice(&["push", "-q", "-u", "origin", "main"]);
        self.git(&args)?;
        Ok(())
    }

    /// Whether `main` has commits `origin` lacks; false without a remote.
    #[must_use]
    pub fn unpushed(&self) -> bool {
        if self.remote().is_empty() {
            return false;
        }
        match self.git(&["rev-list", "--count", "origin/main..main"]) {
            Ok(n) => n.trim() != "0",
            // No origin/main yet: never pushed.
            Err(_) => self.git(&["rev-parse", "--verify", "-q", "main"]).is_ok(),
        }
    }

    /// Snapshots, newest first.
    ///
    /// # Errors
    /// When git fails (an empty repo is not an error).
    pub fn list(&self) -> Result<Vec<Snapshot>, StoreError> {
        match sys::lines(Some(&self.dir), &["git", "log", "--format=%h%x09%cI%x09%s"]) {
            Ok(lines) => Ok(lines
                .iter()
                .filter_map(|l| {
                    let mut p = l.splitn(3, '\t');
                    Some(Snapshot {
                        id: p.next()?.into(),
                        time: p.next()?.into(),
                        subject: p.next()?.into(),
                    })
                })
                .collect()),
            Err(e) if e.stderr.contains("does not have any commits") => Ok(Vec::new()),
            Err(e) => Err(e.into()),
        }
    }

    /// The newest snapshot whose id or timestamp starts with `at` (empty: the latest).
    ///
    /// # Errors
    /// [`StoreError::NotFound`] when nothing matches.
    pub fn resolve(&self, at: &str) -> Result<Snapshot, StoreError> {
        self.list()?
            .into_iter()
            .find(|s| at.is_empty() || s.id.starts_with(at) || s.time.starts_with(at))
            .ok_or_else(|| {
                StoreError::NotFound(if at.is_empty() {
                    "no snapshots yet".into()
                } else {
                    format!("no snapshot matches {at:?}")
                })
            })
    }
}
