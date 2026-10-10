use std::fs;

use phx_contract::Item;

use super::bundle::{bundle_name, capture_work};
use super::{DirEntry, DirManifest, GitDirs, par_map};
use crate::category::files::{MAX_ASSET_FILE, copy_tree};
use crate::category::{Env, Error, write_json};

impl GitDirs {
    pub(super) fn capture_items(&self, env: &Env<'_>, items: &[Item]) -> Result<(), Error> {
        let base = env.home.join(self.rel);
        let root = self.root(&base);
        let mut m = DirManifest::default();
        let mut uses_root = false;
        let ids: Vec<String> = items.iter().map(|it| it.id.clone()).collect();
        let classified = par_map(&ids, |id| Self::classify(&base, root.as_ref(), id));
        for (it, (mut kind, mut why, state)) in items.iter().zip(classified) {
            let mut entry = DirEntry {
                id: it.id.clone(),
                kind: kind.into(),
                git_ref: None,
            };
            if kind == "bundle" {
                let dir = base.join(&it.id);
                if let Some(st) = &state {
                    let rel = format!("files/{}-bundles/{}", self.name, bundle_name(&it.id));
                    let mut r = st.git_ref.clone();
                    match capture_work(&dir, &env.tree.join(&rel), st, &mut r) {
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
                "git" => entry.git_ref = state.map(|s| s.git_ref),
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
}
