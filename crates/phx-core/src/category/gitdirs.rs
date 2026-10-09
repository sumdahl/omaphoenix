//! `plugin` and `theme`: a directory of named subdirectories. Each entry is
//! recorded as a git ref when that ref can be cloned back exactly, as a
//! bundle on top of the remote when it has unpushed or uncommitted work,
//! and copied otherwise, so local work is never lost.
//!
//! A themes directory that is itself a git checkout (`root_repo`) lets
//! tracked, clean entries ride on the parent's ref.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::LazyLock;

use phx_contract::Item;
use regex::Regex;
use serde::{Deserialize, Serialize};

use super::files::{MAX_ASSET_FILE, SECRET_SHAPE, copy_tree, exists, has_files, is_junk};
use super::{Category, Env, Error, Outcome, read_json, write_json};
use crate::sys::{self, CmdError, WatchError, Watchdog};

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

/// scp-style remotes, including host aliases such as
/// `git@github.com-personal:owner/repo.git` that only one machine resolves.
static SSH_ALIAS: LazyLock<Option<Regex>> =
    LazyLock::new(|| Regex::new(r"^[^@/]+@(github\.com)[^:]*:(.+)$").ok());

/// Rewrites GitHub ssh remotes to https, so a fresh machine without keys
/// or aliases can clone them.
fn https_url(url: &str) -> String {
    if let Some(c) = SSH_ALIAS.as_ref().and_then(|re| re.captures(url)) {
        let (host, path) = (
            c.get(1).map_or("", |m| m.as_str()),
            c.get(2).map_or("", |m| m.as_str()),
        );
        return format!("https://{host}/{path}");
    }
    url.strip_prefix("ssh://git@github.com/").map_or_else(
        || url.to_owned(),
        |rest| format!("https://github.com/{rest}"),
    )
}

fn git(dir: &Path, args: &[&str]) -> Result<String, CmdError> {
    let mut cmd = vec!["git"];
    cmd.extend_from_slice(args);
    sys::output(Some(dir), &cmd)
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

/// The git state of `dir` when it is the root of its own checkout with an
/// origin remote.
fn inspect(dir: &Path) -> Option<GitState> {
    let top = git(dir, &["rev-parse", "--show-toplevel"]).ok()?;
    if Path::new(&top) != fs::canonicalize(dir).ok()? {
        return None;
    }
    let url = git(dir, &["remote", "get-url", "origin"])
        .ok()
        .filter(|u| !u.is_empty())?;
    let commit = git(dir, &["rev-parse", "HEAD"]).ok()?;
    let count = |args: &[&str]| {
        git(dir, args)
            .ok()
            .and_then(|n| n.parse().ok())
            .unwrap_or(0)
    };
    Some(GitState {
        git_ref: GitRef {
            url: https_url(&url),
            commit,
            ..GitRef::default()
        },
        unpushed: count(&["rev-list", "--count", "HEAD", "--not", "--remotes"]),
        total: count(&["rev-list", "--count", "HEAD"]),
        dirty: git(dir, &["status", "--porcelain"]).is_ok_and(|s| !s.is_empty()),
    })
}

const TIP_REF: &str = "refs/omaphoenix/tip";
const IDENTITY: &[&str] = &[
    "-c",
    "user.name=omaphoenix",
    "-c",
    "user.email=omaphoenix@localhost",
];

/// Writes a bundle of everything in `dir` the remote lacks and records how
/// to replay it. Refuses when that work contains something shaped like a
/// secret, so the caller falls back to a scanned copy.
fn capture_work(
    dir: &Path,
    bundle: &Path,
    st: &GitState,
    git_ref: &mut GitRef,
) -> Result<(), Error> {
    let mut tip = git_ref.commit.clone();
    if st.dirty {
        let index = tempfile::NamedTempFile::new()?.into_temp_path();
        let index_s = index.to_string_lossy().into_owned();
        fs::remove_file(&index)?;
        let idx = [("GIT_INDEX_FILE", index_s.as_str())];
        sys::output_env(Some(dir), &idx, &["git", "add", "-A"])?;
        let tree = sys::output_env(Some(dir), &idx, &["git", "write-tree"])?;
        // Dates pinned to HEAD's, so unchanged work hashes the same every
        // run; otherwise each timer run would see a "new" commit.
        let date = git(dir, &["show", "-s", "--format=%cI", &git_ref.commit])?;
        let pinned = [
            ("GIT_AUTHOR_DATE", date.as_str()),
            ("GIT_COMMITTER_DATE", date.as_str()),
        ];
        let mut argv = vec!["git"];
        argv.extend_from_slice(IDENTITY);
        argv.extend_from_slice(&[
            "commit-tree",
            &tree,
            "-p",
            &git_ref.commit,
            "-m",
            "omaphoenix: uncommitted work",
        ]);
        let work = sys::output_env(Some(dir), &pinned, &argv)?;
        git_ref.work.clone_from(&work);
        tip = work;
    }
    let diff = git(dir, &["log", "-p", "--format=", &tip, "--not", "--remotes"])?;
    if SECRET_SHAPE
        .as_ref()
        .is_some_and(|re| re.is_match(diff.as_bytes()))
    {
        return Err(Error::Other(
            "its local changes contain something shaped like a secret".into(),
        ));
    }
    git(dir, &["update-ref", TIP_REF, &tip])?;
    let result = (|| -> Result<(), Error> {
        if let Some(parent) = bundle.parent() {
            fs::create_dir_all(parent)?;
        }
        let bundle_s = bundle.to_string_lossy();
        // One pack thread keeps the bundle bytes identical for identical commits.
        git(
            dir,
            &[
                "-c",
                "pack.threads=1",
                "bundle",
                "create",
                "-q",
                &bundle_s,
                TIP_REF,
                "--not",
                "--remotes",
            ],
        )?;
        if let Ok(b) = git(dir, &["symbolic-ref", "-q", "--short", "HEAD"]) {
            git_ref.branch = b;
        }
        Ok(())
    })();
    let _ = git(dir, &["update-ref", "-d", TIP_REF]);
    result
}

/// Replays a bundle into a fresh clone: fetch the commits, put the branch
/// at the snapshot's HEAD, then lay the uncommitted state on the working
/// tree without committing it.
fn apply_work(clone: &Path, r: &GitRef) -> Result<(), Error> {
    git(
        clone,
        &["fetch", "-q", &r.bundle, &format!("{TIP_REF}:{TIP_REF}")],
    )
    .map_err(|e| Error::Other(format!("bundle: {e}")))?;
    let result = (|| -> Result<(), Error> {
        if r.branch.is_empty() {
            git(clone, &["checkout", "-q", &r.commit])?;
        } else {
            git(clone, &["checkout", "-q", "-B", &r.branch, &r.commit])?;
        }
        if r.work.is_empty() {
            return Ok(());
        }
        git(clone, &["checkout", &r.work, "--", "."])?;
        let deleted = sys::lines(
            Some(clone),
            &[
                "git",
                "diff",
                "--name-only",
                "--diff-filter=D",
                &r.commit,
                &r.work,
            ],
        )?;
        for f in deleted {
            let _ = fs::remove_file(clone.join(f));
        }
        git(clone, &["reset", "-q"])?;
        Ok(())
    })();
    let _ = git(clone, &["update-ref", "-d", TIP_REF]);
    result
}

fn bundle_name(id: &str) -> String {
    format!("{}.bundle", id.replace('/', "_"))
}

/// Clones `r` into `target` under the watchdog; a stopped clone leaves no
/// partial directory that the next run would mistake for "present".
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

    /// How an entry is captured, and why it fell back to a copy.
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

    /// Installs one entry by the same steps Omarchy's installers take (URL
    /// check, clone, plugin validation), pinned to the snapshot's commit and
    /// without a running shell, which a fresh install doesn't have yet.
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
    /// Installs one entry by its kind: a clone (with its bundle), a copy out
    /// of the shared themes-repo clone, or a plain copy.
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
        let base = env.home.join(self.rel);
        let root = self.root(&base);
        let mut m = DirManifest::default();
        let mut uses_root = false;
        for it in items {
            let (mut kind, mut why) = Self::classify(&base, root.as_ref(), &it.id);
            let mut entry = DirEntry {
                id: it.id.clone(),
                kind: kind.into(),
                git_ref: None,
            };
            if kind == "bundle" {
                let dir = base.join(&it.id);
                if let Some(st) = inspect(&dir) {
                    let rel = format!("files/{}-bundles/{}", self.name, bundle_name(&it.id));
                    let mut r = st.git_ref.clone();
                    match capture_work(&dir, &env.tree.join(&rel), &st, &mut r) {
                        Ok(()) => {
                            r.bundle = rel;
                            entry.git_ref = Some(r);
                            env.info(&format!("{} {}: bundled {why}", self.name, it.id));
                        }
                        Err(e) => {
                            env.warn(&format!(
                                "{} {}: can't bundle its {why} ({e}) — copying files instead",
                                self.name, it.id
                            ));
                            (kind, why) = ("copy", String::new());
                            entry.kind = "copy".into();
                        }
                    }
                }
            }
            match kind {
                "git" => entry.git_ref = inspect(&base.join(&it.id)).map(|s| s.git_ref),
                "root" => uses_root = true,
                "copy" => {
                    if !why.is_empty() {
                        env.warn(&format!(
                            "{} {} has {why} — copying files instead of the git ref",
                            self.name, it.id
                        ));
                    }
                    let src = fs::canonicalize(base.join(&it.id))?;
                    copy_tree(
                        env,
                        &src,
                        &self.copy_dir(env, &it.id),
                        MAX_ASSET_FILE,
                        &|_, _, _| false,
                    )?;
                }
                _ => {}
            }
            m.entries.push(entry);
        }
        if uses_root {
            m.root = root.map(|r| r.git_ref);
        }
        if self.name == "theme"
            && let Ok(name) =
                fs::read_to_string(env.home.join(".local/state/omarchy/current/theme.name"))
        {
            name.trim().clone_into(&mut m.active);
        }
        write_json(&self.manifest_file(env), &m)
    }

    fn restore(&self, env: &Env<'_>) -> Result<Outcome, Error> {
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

#[cfg(test)]
mod tests {
    use super::{https_url, inspect};
    use crate::category::{Category, Env};
    use std::fs;
    use std::path::Path;

    #[test]
    fn rewrites_ssh_remotes_like_go() {
        assert_eq!(
            https_url("git@github.com-personal:sumdahl/media.git"),
            "https://github.com/sumdahl/media.git"
        );
        assert_eq!(
            https_url("ssh://git@github.com/a/b"),
            "https://github.com/a/b"
        );
        assert_eq!(
            https_url("https://gitlab.com/a/b"),
            "https://gitlab.com/a/b"
        );
    }

    fn git(dir: &Path, args: &[&str]) {
        let mut cmd = vec![
            "git",
            "-c",
            "user.name=t",
            "-c",
            "user.email=t@t",
            "-c",
            "init.defaultBranch=main",
        ];
        cmd.extend_from_slice(args);
        crate::sys::output(Some(dir), &cmd).unwrap_or_else(|e| unreachable!("{e}"));
    }

    /// A theme with unpushed and uncommitted work round-trips through a
    /// bundle onto a fresh clone of its remote, with nothing lost. (A theme,
    /// not a plugin: Omarchy's real plugin validator would reject the fake.)
    #[test]
    fn bundled_work_restores_exactly() {
        let t = tempfile::tempdir().unwrap_or_else(|e| unreachable!("{e}"));
        let remote = t.path().join("remote.git");
        fs::create_dir_all(&remote).ok();
        git(&remote, &["init", "-q", "--bare"]);
        let home = t.path().join("home");
        let plugin = home.join(".config/omarchy/themes/p");
        fs::create_dir_all(&plugin).ok();
        git(&plugin, &["init", "-q"]);
        fs::write(plugin.join("a.qml"), "pushed").ok();
        git(&plugin, &["add", "-A"]);
        git(&plugin, &["commit", "-q", "-m", "one"]);
        git(
            &plugin,
            &["remote", "add", "origin", &remote.to_string_lossy()],
        );
        git(&plugin, &["push", "-q", "origin", "main"]);
        fs::write(plugin.join("a.qml"), "unpushed").ok();
        git(&plugin, &["commit", "-qam", "two"]);
        fs::write(plugin.join("b.qml"), "uncommitted").ok();
        let st = inspect(&plugin).unwrap_or_else(|| unreachable!("a checkout"));
        assert_eq!((st.unpushed, st.dirty), (1, true));

        let tree = t.path().join("tree");
        let log = |_: &str, _: &str| {};
        let env = Env::new(&home, &tree, &log);
        let cat = super::themes();
        let items = cat.detect(&env).unwrap_or_default();
        assert_eq!(items.first().map(|i| i.note.as_str()), Some("bundle"));
        cat.capture(&env, &items)
            .unwrap_or_else(|e| unreachable!("capture: {e}"));

        let fresh = t.path().join("fresh");
        let env2 = Env::new(&fresh, &tree, &log);
        let res = cat.restore(&env2).unwrap_or_default();
        assert_eq!(res.applied, 1, "{:?}", res.failed);
        let restored = fresh.join(".config/omarchy/themes/p");
        assert_eq!(
            fs::read_to_string(restored.join("a.qml")).ok().as_deref(),
            Some("unpushed")
        );
        assert_eq!(
            fs::read_to_string(restored.join("b.qml")).ok().as_deref(),
            Some("uncommitted")
        );
        let log =
            crate::sys::output(Some(&restored), &["git", "log", "--format=%s"]).unwrap_or_default();
        assert_eq!(
            log, "two\none",
            "history kept, uncommitted work not committed"
        );
    }
}
