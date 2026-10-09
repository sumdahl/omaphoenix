//! `config`: what the user actually configured, discovered rather than
//! listed: every entry in `~/.config` plus top-level dotfiles in `~`. Files
//! identical to Omarchy's shipped default are skipped (a fresh install has
//! them), and so are app profiles, oversized entries, secrets and junk.

use std::fs;
use std::path::{Path, PathBuf};

use phx_contract::Item;

use super::files::{
    MAX_CONFIG_FILE, Walk, copy_file, copy_tree, dir_size, exists, is_binary, is_junk, is_secret,
    same_file, walk,
};
use super::{Category, Env, Error, Outcome, stamp};

/// `config`.
pub struct Config;

/// Entries bigger than this are offered, not copied silently.
const MAX_ENTRY_SIZE: u64 = 50 << 20;
const OMARCHY_DEFAULT: &str = "/usr/share/omarchy/config";

/// Handled by their own categories.
const OWNED_ELSEWHERE: &[&str] = &[".config/omarchy/plugins", ".config/omarchy/themes"];

/// Browser and Electron profiles hold caches, cookies and session tokens,
/// not configuration.
const PROFILE_MARKERS: &[&str] = &[
    "Local State",
    "Local Storage",
    "GPUCache",
    "Code Cache",
    "IndexedDB",
];

fn tree_dir(env: &Env<'_>) -> PathBuf {
    env.tree.join("files/config")
}

pub(super) fn skip_path(rel: &str, name: &str) -> bool {
    is_secret(name) || OWNED_ELSEWHERE.contains(&rel)
}

fn is_app_profile(dir: &Path) -> bool {
    let check = |d: &Path| PROFILE_MARKERS.iter().any(|m| exists(&d.join(m)));
    check(dir)
        || fs::read_dir(dir)
            .into_iter()
            .flatten()
            .flatten()
            .any(|s| s.file_type().is_ok_and(|t| t.is_dir()) && check(&s.path()))
}

/// Size of `~/<id>` without paths owned by other categories.
fn dir_size_except(home: &Path, id: &str, limit: u64) -> bool {
    let mut total = 0u64;
    let mut subs: Vec<_> = fs::read_dir(home.join(id))
        .into_iter()
        .flatten()
        .flatten()
        .collect();
    subs.sort_by_key(fs::DirEntry::file_name);
    for s in subs {
        let name = s.file_name().to_string_lossy().into_owned();
        if skip_path(&format!("{id}/{name}"), &name) || is_junk(&name) {
            continue;
        }
        let (n, over) = dir_size(&s.path(), limit.saturating_sub(total));
        total += n;
        if over {
            return true;
        }
    }
    false
}

fn is_omarchy_default(rel: &str, path: &Path) -> bool {
    rel.strip_prefix(".config/")
        .is_some_and(|under| same_file(path, &Path::new(OMARCHY_DEFAULT).join(under)))
}

impl Category for Config {
    fn name(&self) -> &'static str {
        "config"
    }

    fn detect(&self, env: &Env<'_>) -> Result<Vec<Item>, Error> {
        let mut items = Vec::new();
        let cfg = env.home.join(".config");
        let mut entries: Vec<_> = match fs::read_dir(&cfg) {
            Ok(rd) => rd.flatten().collect(),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Vec::new(),
            Err(e) => return Err(e.into()),
        };
        entries.sort_by_key(fs::DirEntry::file_name);
        for e in entries {
            let name = e.file_name().to_string_lossy().into_owned();
            let id = format!(".config/{name}");
            if is_junk(&name) || skip_path(&id, &name) {
                continue;
            }
            let path = cfg.join(&name);
            if !fs::metadata(&path).is_ok_and(|m| m.is_dir()) {
                items.push(Item::new(id, ""));
                continue;
            }
            if is_app_profile(&path) {
                env.info(&format!(
                    "config {id} skipped: browser/app profile (caches and sessions, not config)"
                ));
                continue;
            }
            if dir_size_except(&env.home, &id, MAX_ENTRY_SIZE) {
                env.info(&format!(
                    "config {id} skipped: over {} MB (exclude or add it explicitly later)",
                    MAX_ENTRY_SIZE >> 20
                ));
                continue;
            }
            items.push(Item::new(id, ""));
        }

        let mut home: Vec<_> = fs::read_dir(&env.home)?.flatten().collect();
        home.sort_by_key(fs::DirEntry::file_name);
        for e in home {
            let name = e.file_name().to_string_lossy().into_owned();
            if !name.starts_with('.')
                || !e.file_type().is_ok_and(|t| t.is_file())
                || is_junk(&name)
                || is_secret(&name)
            {
                continue;
            }
            if e.metadata().is_ok_and(|m| m.len() <= MAX_CONFIG_FILE) {
                items.push(Item::new(name, ""));
            }
        }
        Ok(items)
    }

    fn capture(&self, env: &Env<'_>, items: &[Item]) -> Result<(), Error> {
        for it in items {
            let src = env.home.join(&it.id);
            let id = it.id.clone();
            let skip = |rel: &Path, name: &str, meta: &fs::Metadata| {
                let full = format!("{id}/{}", rel.display());
                if skip_path(&full, name) {
                    return true;
                }
                let path = src.join(rel);
                meta.is_file() && (is_omarchy_default(&full, &path) || is_binary(&path))
            };
            copy_tree(
                env,
                &src,
                &tree_dir(env).join(&it.id),
                MAX_CONFIG_FILE,
                &skip,
            )
            .map_err(|e| Error::Other(format!("config {}: {e}", it.id)))?;
        }
        Ok(())
    }

    fn restore(&self, env: &Env<'_>) -> Result<Outcome, Error> {
        Ok(restore_tree(env, &tree_dir(env), &env.home))
    }
}

/// Mirrors a captured tree into `home`, keeping any differing file it
/// replaces as `<name>.pre-restore.<stamp>`. Never deletes.
pub(super) fn restore_tree(env: &Env<'_>, root: &Path, home: &Path) -> Outcome {
    let mut res = Outcome::default();
    if !exists(root) {
        return res;
    }
    let stamp = stamp();
    walk(root, &mut |path, meta| {
        if meta.is_dir() {
            return Walk::Continue;
        }
        let Ok(rel) = path.strip_prefix(root) else {
            return Walk::Continue;
        };
        let rel_s = rel.to_string_lossy();
        if !env.want(&rel_s) {
            return Walk::Continue;
        }
        let target = home.join(rel);
        let symlink = meta.file_type().is_symlink();
        if same(path, &target, symlink) {
            res.present += 1;
            return Walk::Continue;
        }
        let existed = exists(&target);
        if env.dry_run {
            res.planned.push(format!(
                "{} ~/{rel_s}",
                if existed { "overwrite" } else { "create" }
            ));
            return Walk::Continue;
        }
        let mut backup = None;
        if existed {
            let b = PathBuf::from(format!("{}.pre-restore.{stamp}", target.display()));
            if let Err(e) = fs::rename(&target, &b) {
                res.failed.push(format!("{rel_s}: {e}"));
                return Walk::Continue;
            }
            backup = Some(b);
        }
        if let Err(e) = place(path, &target, meta, symlink) {
            res.failed.push(format!("{rel_s}: {e}"));
            return Walk::Continue;
        }
        res.wrote_file(&target, backup.as_deref(), false);
        res.applied += 1;
        Walk::Continue
    });
    res
}

fn same(src: &Path, target: &Path, symlink: bool) -> bool {
    if symlink {
        return matches!((fs::read_link(src), fs::read_link(target)), (Ok(a), Ok(b)) if a == b);
    }
    fs::symlink_metadata(target).is_ok_and(|m| m.is_file()) && same_file(src, target)
}

fn place(src: &Path, target: &Path, meta: &fs::Metadata, symlink: bool) -> std::io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent)?;
    }
    if symlink {
        return std::os::unix::fs::symlink(fs::read_link(src)?, target);
    }
    copy_file(src, target, meta.permissions().mode() & 0o777)
}

#[cfg(test)]
mod tests {
    use super::{Config, restore_tree};
    use crate::category::{Category, Env};
    use std::fs;

    fn write(p: &std::path::Path, body: &str) {
        fs::create_dir_all(p.parent().unwrap_or(p)).ok();
        fs::write(p, body).ok();
    }

    #[test]
    fn detects_configs_and_dotfiles_but_not_profiles_or_secrets() {
        let t = tempfile::tempdir().unwrap_or_else(|e| unreachable!("{e}"));
        let h = t.path();
        write(&h.join(".config/hypr/hyprland.conf"), "x");
        write(&h.join(".config/BraveSoftware/Default/Local State"), "x");
        write(&h.join(".config/gh-token"), "x");
        write(&h.join(".config/omarchy/plugins/p/a"), "x");
        write(&h.join(".zshrc"), "x");
        write(&h.join(".netrc"), "x");
        let log = |_: &str, _: &str| {};
        let env = Env::new(h, &h.join("tree"), &log);
        let ids: Vec<String> = Config
            .detect(&env)
            .unwrap_or_default()
            .into_iter()
            .map(|i| i.id)
            .collect();
        assert_eq!(ids, [".config/hypr", ".config/omarchy", ".zshrc"]);
    }

    #[test]
    fn restore_keeps_what_it_replaces_and_reports_dry_runs() {
        let t = tempfile::tempdir().unwrap_or_else(|e| unreachable!("{e}"));
        let (tree, home) = (t.path().join("tree"), t.path().join("home"));
        write(&tree.join(".zshrc"), "new");
        write(&tree.join(".config/a"), "same");
        write(&home.join(".zshrc"), "old");
        write(&home.join(".config/a"), "same");
        let log = |_: &str, _: &str| {};
        let mut env = Env::new(&home, &tree, &log);
        env.dry_run = true;
        let plan = restore_tree(&env, &tree, &home);
        assert_eq!(
            (plan.present, plan.planned.as_slice()),
            (1, ["overwrite ~/.zshrc".to_owned()].as_slice())
        );
        env.dry_run = false;
        let res = restore_tree(&env, &tree, &home);
        assert_eq!((res.applied, res.present), (1, 1));
        assert_eq!(
            fs::read_to_string(home.join(".zshrc")).ok().as_deref(),
            Some("new")
        );
        let backup = fs::read_dir(&home)
            .into_iter()
            .flatten()
            .flatten()
            .find(|e| {
                e.file_name()
                    .to_string_lossy()
                    .starts_with(".zshrc.pre-restore.")
            });
        assert_eq!(
            backup
                .and_then(|b| fs::read_to_string(b.path()).ok())
                .as_deref(),
            Some("old")
        );
        assert_eq!(res.changes.len(), 1);
    }
}
