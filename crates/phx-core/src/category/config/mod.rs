//! `config`: every `~/.config` entry and top-level dotfile, minus Omarchy defaults, profiles and secrets.

mod mirror;

use std::fs;
use std::path::{Path, PathBuf};

use phx_contract::Item;

use super::files::{
    MAX_CONFIG_FILE, copy_tree, dir_size, exists, is_binary, is_junk, is_secret, same_file,
};
use super::{Category, Env, Error, Outcome};

pub(super) use mirror::restore_tree;

/// `config`.
pub struct Config;

/// Entries bigger than this are offered, not copied silently.
const MAX_ENTRY_SIZE: u64 = 50 << 20;
const OMARCHY_DEFAULT: &str = "/usr/share/omarchy/config";

/// Handled by their own categories.
const OWNED_ELSEWHERE: &[&str] = &[".config/omarchy/plugins", ".config/omarchy/themes"];

// Browser and Electron profiles hold caches, cookies and session tokens, not configuration.
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

// Size of `~/<id>` without paths owned by other categories.
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

#[cfg(test)]
mod tests {
    use super::Config;
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
}
