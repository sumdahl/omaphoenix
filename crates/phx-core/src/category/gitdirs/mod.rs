//! `plugin` and `theme`: entries kept as git refs, bundles of local work, or copies.

mod bundle;
mod capture;
mod git;
mod restore;
#[cfg(test)]
mod tests;

use std::fs;
use std::path::{Path, PathBuf};

use phx_contract::Item;
use serde::{Deserialize, Serialize};

use super::files::{has_files, is_junk};
use super::{Category, Env, Error, Outcome};
use git::{git, inspect};

/// Plugins or themes.
pub struct GitDirs {
    name: &'static str,
    rel: &'static str,
    root_repo: bool,
}

/// Omarchy plugins.
#[must_use]
pub fn plugins() -> GitDirs {
    GitDirs {
        name: "plugin",
        rel: ".config/omarchy/plugins",
        root_repo: false,
    }
}

/// Omarchy themes.
#[must_use]
pub fn themes() -> GitDirs {
    GitDirs {
        name: "theme",
        rel: ".config/omarchy/themes",
        root_repo: true,
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq, Eq)]
struct GitRef {
    url: String,
    commit: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    branch: String,
    /// Commit holding uncommitted changes.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    work: String,
    /// Tree-relative bundle of work the remote lacks.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    bundle: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
struct DirEntry {
    id: String,
    /// `git`, `bundle`, `root` or `copy`.
    kind: String,
    #[serde(default, skip_serializing_if = "Option::is_none", rename = "ref")]
    git_ref: Option<GitRef>,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq, Eq)]
struct DirManifest {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    root: Option<GitRef>,
    /// Active theme name.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    active: String,
    entries: Vec<DirEntry>,
}

#[derive(Clone, Debug)]
struct GitState {
    git_ref: GitRef,
    unpushed: u64,
    total: u64,
    dirty: bool,
}

impl GitState {
    fn clean(&self) -> bool {
        self.unpushed == 0 && !self.dirty
    }

    /// The remote holds at least the base of HEAD, so clone + bundle rebuilds it.
    fn on_remote(&self) -> bool {
        self.unpushed < self.total
    }

    fn why(&self) -> String {
        let mut parts = Vec::new();
        if self.unpushed > 0 {
            parts.push(format!("{} unpushed", self.unpushed));
        }
        if self.dirty {
            parts.push("uncommitted changes".to_owned());
        }
        parts.join(", ")
    }
}

impl GitDirs {
    fn manifest_file(&self, env: &Env<'_>) -> PathBuf {
        env.tree.join(format!("{}s.json", self.name))
    }

    fn copy_dir(&self, env: &Env<'_>, id: &str) -> PathBuf {
        env.tree.join("files").join(self.name).join(id)
    }

    fn root(&self, base: &Path) -> Option<GitState> {
        if self.root_repo { inspect(base) } else { None }
    }

    fn classify(base: &Path, root: Option<&GitState>, id: &str) -> (&'static str, String) {
        if let Some(st) = inspect(&base.join(id)) {
            return if st.clean() {
                ("git", String::new())
            } else if st.on_remote() {
                ("bundle", st.why())
            } else {
                ("copy", "never pushed to its remote".into())
            };
        }
        if root.is_some_and(|r| r.unpushed == 0) {
            let tracked = git(base, &["ls-files", "--", id]).unwrap_or_default();
            let status = git(base, &["status", "--porcelain", "--", id]).unwrap_or_default();
            if !tracked.is_empty() && status.is_empty() {
                return ("root", String::new());
            }
        }
        ("copy", String::new())
    }
}

impl Category for GitDirs {
    fn name(&self) -> &'static str {
        self.name
    }

    fn detect(&self, env: &Env<'_>) -> Result<Vec<Item>, Error> {
        let base = env.home.join(self.rel);
        let mut entries: Vec<_> = match fs::read_dir(&base) {
            Ok(rd) => rd.flatten().collect(),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(e) => return Err(e.into()),
        };
        entries.sort_by_key(fs::DirEntry::file_name);
        let root = self.root(&base);
        let mut items = Vec::new();
        for e in entries {
            let name = e.file_name().to_string_lossy().into_owned();
            let dir = base.join(&name);
            if name.starts_with('.')
                || is_junk(&name)
                || !fs::metadata(&dir).is_ok_and(|m| m.is_dir())
                || !has_files(&dir)
            {
                continue; // git can't hold an empty directory: nothing to restore
            }
            let (kind, _) = Self::classify(&base, root.as_ref(), &name);
            items.push(Item::new(name, kind));
        }
        Ok(items)
    }

    fn capture(&self, env: &Env<'_>, items: &[Item]) -> Result<(), Error> {
        self.capture_items(env, items)
    }

    fn restore(&self, env: &Env<'_>) -> Result<Outcome, Error> {
        self.restore_items(env)
    }
}
