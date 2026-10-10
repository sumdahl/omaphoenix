//! `local`: launchers, icons, fonts and small app state under `~/.local`.

use std::fs;
use std::path::{Path, PathBuf};

use phx_contract::Item;

use super::config::restore_tree;
use super::files::{
    MAX_ASSET_FILE, MAX_CONFIG_FILE, copy_tree, dir_size, has_files, is_binary, is_junk, is_secret,
    match_any, same_file,
};
use super::{Category, Env, Error, Outcome};

/// `local`.
pub struct Local;

/// Always-candidate paths, relative to `~/.local`.
const FIXED: &[&str] = &[
    "share/applications",
    "share/icons",
    "share/fonts",
    "state/omarchy/toggles",
    "state/omarchy/settings",
    "state/omarchy/workspace-layouts",
];

/// App state dirs larger than this are history, caches or media, not settings.
const MAX_STATE_DIR: u64 = 1 << 20;

/// State that must come from the new install itself; `omaphoenix` is this tool's own cache and undo records.
const STATE_SKIP: &[&str] = &["omarchy", "mise", "nvim", "pnpm", "gh", "omaphoenix"];

const LAUNCHER_DEFAULTS: &[&str] = &["/usr/share/omarchy/applications", "/usr/share/applications"];

fn tree_dir(env: &Env<'_>) -> PathBuf {
    env.tree.join("files/local")
}

fn local_path(env: &Env<'_>, id: &str) -> PathBuf {
    env.home.join(".local").join(id)
}

fn is_default_launcher(path: &Path) -> bool {
    path.extension().is_some_and(|e| e == "desktop")
        && path.file_name().is_some_and(|n| {
            LAUNCHER_DEFAULTS
                .iter()
                .any(|d| same_file(path, &Path::new(d).join(n)))
        })
}

impl Category for Local {
    fn name(&self) -> &'static str {
        "local"
    }

    fn detect(&self, env: &Env<'_>) -> Result<Vec<Item>, Error> {
        let mut items: Vec<Item> = FIXED
            .iter()
            .filter(|id| has_files(&local_path(env, id)))
            .map(|id| Item::new(*id, ""))
            .collect();
        let mut states: Vec<_> = fs::read_dir(local_path(env, "state"))
            .into_iter()
            .flatten()
            .flatten()
            .collect();
        states.sort_by_key(fs::DirEntry::file_name);
        for e in states {
            let name = e.file_name().to_string_lossy().into_owned();
            if !e.file_type().is_ok_and(|t| t.is_dir())
                || is_junk(&name)
                || is_secret(&name)
                || match_any(&name, STATE_SKIP)
            {
                continue;
            }
            let (size, over) = dir_size(&local_path(env, &format!("state/{name}")), MAX_STATE_DIR);
            if !over && size > 0 {
                items.push(Item::new(format!("state/{name}"), ""));
            }
        }
        Ok(items)
    }

    fn capture(&self, env: &Env<'_>, items: &[Item]) -> Result<(), Error> {
        for it in items {
            let src = local_path(env, &it.id);
            let skip = |rel: &Path, name: &str, meta: &fs::Metadata| {
                if is_secret(name) || name == "mimeinfo.cache" {
                    return true;
                }
                let p = src.join(rel);
                meta.is_file() && (is_default_launcher(&p) || is_binary(&p))
            };
            let limit = if it.id == "share/fonts" || it.id == "share/icons" {
                MAX_ASSET_FILE
            } else {
                MAX_CONFIG_FILE
            };
            copy_tree(env, &src, &tree_dir(env).join(&it.id), limit, &skip)?;
        }
        Ok(())
    }

    fn restore(&self, env: &Env<'_>) -> Result<Outcome, Error> {
        let mut res = restore_tree(env, &tree_dir(env), &env.home.join(".local"));
        // restore_tree labels paths relative to the home it was given.
        for p in &mut res.planned {
            *p = p.replacen(" ~/", " ~/.local/", 1);
        }
        Ok(res)
    }
}

#[cfg(test)]
mod tests {
    use super::Local;
    use crate::category::{Category, Env};

    #[test]
    fn never_offers_omaphoenix_own_state() {
        let t = tempfile::tempdir().unwrap_or_else(|e| unreachable!("{e}"));
        for (dir, file) in [("omaphoenix", "gate-daily.json"), ("myapp", "state.json")] {
            let d = t.path().join(".local/state").join(dir);
            std::fs::create_dir_all(&d).unwrap_or_else(|e| unreachable!("{e}"));
            std::fs::write(d.join(file), "{}").unwrap_or_else(|e| unreachable!("{e}"));
        }
        let log = |_: &str, _: &str| {};
        let env = Env::new(t.path(), &t.path().join("tree"), &log);
        let ids: Vec<String> = Local
            .detect(&env)
            .unwrap_or_default()
            .into_iter()
            .map(|i| i.id)
            .collect();
        assert!(ids.contains(&"state/myapp".to_owned()), "{ids:?}");
        assert!(!ids.contains(&"state/omaphoenix".to_owned()), "{ids:?}");
    }
}
