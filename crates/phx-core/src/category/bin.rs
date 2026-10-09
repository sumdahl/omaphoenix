//! `bin`: hand-written scripts from the user's executable directories.
//! Compiled binaries and symlinks belong to whatever installed them, so they
//! are listed, not copied: restore reports them as "reinstall manually".

use std::fmt::Write as _;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use phx_contract::Item;
use serde::{Deserialize, Serialize};

use super::files::{copy_file, exists, has_secret_shape, is_binary, is_junk, same_file};
use super::{Category, Env, Error, Outcome, read_json, stamp, write_json};
use crate::sys;

/// `bin`.
pub struct Bin;

/// User-writable script dirs relative to `$HOME`, plus `/usr/local/bin`
/// (restored through sudo).
const DIRS: &[&str] = &[".local/bin", "bin", "/usr/local/bin"];

const MAX_SCRIPT: u64 = 1 << 20;

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq, Eq)]
struct Entry {
    /// `~`-relative, or absolute for system dirs.
    path: String,
    /// `script`, `binary` or `symlink`.
    kind: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    target: String,
    #[serde(default, skip_serializing_if = "is_zero")]
    size: u64,
}

#[allow(
    clippy::trivially_copy_pass_by_ref,
    reason = "serde skip_serializing_if takes a reference"
)]
fn is_zero(n: &u64) -> bool {
    *n == 0
}

fn abs(env: &Env<'_>, p: &str) -> PathBuf {
    if p.starts_with('/') {
        PathBuf::from(p)
    } else {
        env.home.join(p)
    }
}

fn stored(env: &Env<'_>, p: &str) -> PathBuf {
    env.tree.join("files/bin").join(p.trim_start_matches('/'))
}

fn classify(path: &Path) -> Option<Entry> {
    let meta = fs::symlink_metadata(path).ok()?;
    if meta.is_dir() {
        return None;
    }
    if meta.file_type().is_symlink() {
        let target = fs::read_link(path)
            .map(|t| t.to_string_lossy().into_owned())
            .unwrap_or_default();
        return Some(Entry {
            kind: "symlink".into(),
            target,
            ..Entry::default()
        });
    }
    if !meta.is_file() || meta.permissions().mode() & 0o111 == 0 {
        return None;
    }
    let kind = if is_binary(path) || meta.len() > MAX_SCRIPT {
        "binary"
    } else {
        "script"
    };
    Some(Entry {
        kind: kind.into(),
        size: meta.len(),
        ..Entry::default()
    })
}

impl Category for Bin {
    fn name(&self) -> &'static str {
        "bin"
    }

    fn detect(&self, env: &Env<'_>) -> Result<Vec<Item>, Error> {
        let mut candidates: Vec<(String, PathBuf, String)> = Vec::new();
        for d in DIRS {
            let mut entries: Vec<_> = fs::read_dir(abs(env, d))
                .into_iter()
                .flatten()
                .flatten()
                .collect();
            entries.sort_by_key(fs::DirEntry::file_name);
            for e in entries {
                let name = e.file_name().to_string_lossy().into_owned();
                if is_junk(&name) || name.contains(".old") {
                    continue;
                }
                let p = format!("{d}/{name}");
                let a = abs(env, &p);
                if let Some(entry) = classify(&a) {
                    candidates.push((p, a, entry.kind));
                }
            }
        }
        let owned = if sys::has("pacman") {
            sys::pacman_owned(
                &candidates
                    .iter()
                    .map(|(_, a, _)| a.to_string_lossy().into_owned())
                    .collect::<Vec<_>>(),
            )
        } else {
            std::collections::BTreeSet::new()
        };
        Ok(candidates
            .into_iter()
            .filter(|(_, a, _)| !owned.contains(a.to_string_lossy().as_ref()))
            .map(|(p, _, kind)| Item::new(p, kind))
            .collect())
    }

    fn capture(&self, env: &Env<'_>, items: &[Item]) -> Result<(), Error> {
        let mut entries = Vec::new();
        for it in items {
            let a = abs(env, &it.id);
            if env.audit_excludes(&a) {
                continue;
            }
            let Some(mut e) = classify(&a) else { continue };
            e.path.clone_from(&it.id);
            if e.kind == "script" {
                if has_secret_shape(&a) {
                    env.warn(&format!(
                        "bin {} skipped: contains something shaped like a secret",
                        it.id
                    ));
                    continue;
                }
                copy_file(&a, &stored(env, &it.id), 0o755)?;
            }
            entries.push(e);
        }
        if entries.is_empty() {
            return Ok(());
        }
        write_json(&env.tree.join("bin.json"), &entries)
    }

    fn restore(&self, env: &Env<'_>) -> Result<Outcome, Error> {
        let mut res = Outcome::default();
        let Some(entries): Option<Vec<Entry>> = read_json(&env.tree.join("bin.json"))? else {
            return Ok(res);
        };
        for e in entries {
            let base = Path::new(&e.path)
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
            if !env.want(&e.path) && !env.want(&base) {
                continue;
            }
            let target = abs(env, &e.path);
            if e.kind != "script" {
                if exists(&target) {
                    res.present += 1;
                    continue;
                }
                // Not restorable from a snapshot by design; say so plainly.
                let mut note = format!("reinstall manually: {} ({}", e.path, e.kind);
                if !e.target.is_empty() {
                    note.push_str(" → ");
                    note.push_str(&e.target);
                }
                if e.size > 0 {
                    let _ = write!(note, ", {} MB", e.size >> 20);
                }
                env.warn(&format!("{note})"));
                continue;
            }
            let src = stored(env, &e.path);
            if same_file(&src, &target) {
                res.present += 1;
                continue;
            }
            if env.dry_run {
                res.planned.push(format!("script {}", e.path));
                continue;
            }
            // A different script already there is the user's: keep it.
            let root = e.path.starts_with('/');
            let (src_s, target_s) = (src.to_string_lossy(), target.to_string_lossy());
            let mut backup = None;
            if exists(&target) {
                let b = format!("{target_s}.pre-restore.{}", stamp());
                let moved = if root {
                    sys::interactive(&["sudo", "mv", "--", &target_s, &b]).is_ok()
                } else {
                    fs::rename(&target, &b).is_ok()
                };
                if !moved {
                    res.failed
                        .push(format!("{}: could not keep the existing file", e.path));
                    continue;
                }
                backup = Some(PathBuf::from(b));
            }
            let placed = if root {
                sys::interactive(&[
                    "sudo", "install", "-D", "-m", "0755", "--", &src_s, &target_s,
                ])
                .map_err(|e| e.to_string())
            } else {
                copy_file(&src, &target, 0o755).map_err(|e| e.to_string())
            };
            match placed {
                Ok(()) => {
                    res.wrote_file(&target, backup.as_deref(), root);
                    res.applied += 1;
                }
                Err(err) => res.failed.push(format!("{}: {err}", e.path)),
            }
        }
        Ok(res)
    }
}
