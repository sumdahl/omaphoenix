use std::fs;
use std::path::{Path, PathBuf};

use crate::category::files::{Walk, copy_file, exists, same_file, walk};
use crate::category::{Env, Outcome, stamp};

/// Mirrors a captured tree into `home`, keeping replaced files as `.pre-restore.<stamp>`.
pub(crate) fn restore_tree(env: &Env<'_>, root: &Path, home: &Path) -> Outcome {
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
        if !symlink && super::shell_init::is_shell_init(&rel_s) {
            match super::shell_init::guard_missing_sources(&target, home) {
                Ok(g) if !g.is_empty() => env.warn(&format!(
                    "~/{rel_s} loads {}, not on this machine yet: those lines now run only once the file exists",
                    g.join(", ")
                )),
                Ok(_) => {}
                Err(e) => res.failed.push(format!("{rel_s}: {e}")),
            }
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
    use super::restore_tree;
    use crate::category::Env;
    use std::fs;

    fn write(p: &std::path::Path, body: &str) {
        fs::create_dir_all(p.parent().unwrap_or(p)).ok();
        fs::write(p, body).ok();
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
