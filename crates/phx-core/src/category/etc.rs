//! `etc`: system configuration the user changed: package config files
//! pacman reports as modified, and files no package owns in the drop-in
//! directories. Restore goes through sudo one file at a time.

use std::collections::BTreeSet;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use phx_contract::Item;
use serde::{Deserialize, Serialize};

use super::files::{
    MAX_CONFIG_FILE, Walk, copy_file, exists, has_secret_shape, is_junk, match_any, same_file, walk,
};
use super::{Category, Env, Error, Outcome, read_json, stamp, write_json};
use crate::sys;

/// `etc`.
pub struct Etc;

/// Drop-in directories scanned for files no package owns. Looking
/// everywhere in /etc would sweep up generated state.
const DROP_INS: &[&str] = &[
    "systemd/system",
    "systemd/user",
    "pacman.d/hooks",
    "modprobe.d",
    "modules-load.d",
    "udev/rules.d",
    "sysctl.d",
    "tmpfiles.d",
    "X11/xorg.conf.d",
    "security/limits.d",
    "environment.d",
    "NetworkManager/conf.d",
    "systemd/logind.conf.d",
    "systemd/sleep.conf.d",
    "systemd/resolved.conf.d",
    "systemd/system.conf.d",
    "systemd/journald.conf.d",
    "sudoers.d",
];

/// Machine identity, disks, boot and accounts: restoring them onto another
/// machine breaks booting or logins.
const NEVER: &[&str] = &[
    "fstab",
    "crypttab",
    "passwd",
    "group",
    "shadow",
    "gshadow",
    "subuid",
    "subgid",
    "shells",
    "resolv.conf",
    "hostname",
    "hosts",
    "machine-id",
    "adjtime",
    "localtime",
    "mkinitcpio.conf",
    "mkinitcpio.d/*",
    "default/limine",
    "limine*",
    "sudoers",
    "ssh/ssh_host_*",
    "ld.so.cache",
    ".pwd.lock",
    "pacman.d/mirrorlist",
    "pam.d/*",
    "mkinitcpio.conf.d/*",
];

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
struct EtcFile {
    /// Absolute path.
    path: String,
    /// Git keeps only the exec bit; the real mode lives here.
    mode: u32,
    /// `modified` or `added`.
    why: String,
}

fn denied(abs: &str) -> bool {
    let rel = abs.strip_prefix("/etc/").unwrap_or(abs);
    let base = Path::new(abs)
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    match_any(rel, NEVER) || is_junk(&base) || abs.ends_with(".pacnew") || abs.ends_with(".pacsave")
}

fn stored(env: &Env<'_>, p: &str) -> PathBuf {
    env.tree.join("files").join(p.trim_start_matches('/'))
}

impl Category for Etc {
    fn name(&self) -> &'static str {
        "etc"
    }

    fn detect(&self, env: &Env<'_>) -> Result<Vec<Item>, Error> {
        if !sys::has("pacman") {
            return Ok(Vec::new());
        }
        let mut seen = BTreeSet::new();
        let mut items = Vec::new();
        let mut add = |p: &str, why: &str| {
            if !denied(p) && seen.insert(p.to_owned()) {
                items.push(Item::new(p, why));
            }
        };
        // ponytail: pacman -Qii prints every package's backup list (~1 MB);
        // streamed line by line it stays small. Fine until it isn't.
        for line in sys::output(None, &["pacman", "-Qii"])?.lines() {
            let line = line.trim();
            let line = line.strip_prefix("Backup Files    : ").unwrap_or(line);
            if let Some(p) = line
                .strip_suffix(" [modified]")
                .filter(|p| p.starts_with("/etc/"))
            {
                add(p, "modified");
            }
        }
        let mut candidates = Vec::new();
        for d in DROP_INS {
            let root = PathBuf::from("/etc").join(d);
            walk(&root, &mut |p, meta| {
                let ps = p.to_string_lossy();
                if ps.ends_with(".pacnew") {
                    env.warn(&format!(
                        "{ps} is waiting to be merged (pacdiff); it is not captured"
                    ));
                }
                if meta.is_dir() && fs::read_dir(p).is_err() {
                    env.info(&format!("etc {ps} skipped: needs root to read"));
                    return Walk::Skip;
                }
                if meta.is_file() {
                    candidates.push(ps.into_owned());
                }
                Walk::Continue
            });
        }
        let owned = sys::pacman_owned(&candidates);
        for p in candidates.iter().filter(|p| !owned.contains(*p)) {
            add(p, "added");
        }
        Ok(items)
    }

    fn capture(&self, env: &Env<'_>, items: &[Item]) -> Result<(), Error> {
        let mut files = Vec::new();
        for it in items {
            let Ok(meta) = fs::metadata(&it.id) else {
                continue;
            };
            if meta.len() > MAX_CONFIG_FILE {
                env.warn(&format!(
                    "etc {} skipped: over {} MB",
                    it.id,
                    MAX_CONFIG_FILE >> 20
                ));
                continue;
            }
            if has_secret_shape(Path::new(&it.id)) {
                env.warn(&format!(
                    "etc {} skipped: contains something shaped like a secret",
                    it.id
                ));
                continue;
            }
            match copy_file(Path::new(&it.id), &stored(env, &it.id), 0o644) {
                Ok(()) => {}
                Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied => {
                    env.info(&format!("etc {} skipped: needs root to read", it.id));
                    continue;
                }
                Err(e) => return Err(e.into()),
            }
            files.push(EtcFile {
                path: it.id.clone(),
                mode: meta.permissions().mode() & 0o777,
                why: it.note.clone(),
            });
        }
        if files.is_empty() {
            return Ok(());
        }
        write_json(&env.tree.join("etc.json"), &files)
    }

    fn restore(&self, env: &Env<'_>) -> Result<Outcome, Error> {
        let mut res = Outcome::default();
        let Some(files): Option<Vec<EtcFile>> = read_json(&env.tree.join("etc.json"))? else {
            return Ok(res);
        };
        let stamp = stamp();
        for f in files {
            if !env.want(&f.path) {
                continue;
            }
            let src = stored(env, &f.path);
            let target = Path::new(&f.path);
            if same_file(&src, target) {
                res.present += 1;
                continue;
            }
            if env.dry_run {
                let verb = if exists(target) {
                    "overwrite"
                } else {
                    "create"
                };
                res.planned
                    .push(format!("{verb} {} ({}, sudo)", f.path, f.why));
                continue;
            }
            let mut backup = None;
            if exists(target) {
                let b = format!("{}.pre-restore.{stamp}", f.path);
                if sys::interactive(&["sudo", "cp", "-a", "--", &f.path, &b]).is_err() {
                    res.failed.push(format!("{}: backup failed", f.path));
                    continue;
                }
                backup = Some(PathBuf::from(b));
            }
            let mode = format!("{:o}", f.mode);
            let src_s = src.to_string_lossy();
            if sys::interactive(&[
                "sudo", "install", "-D", "-o", "root", "-g", "root", "-m", &mode, "--", &src_s,
                &f.path,
            ])
            .is_err()
            {
                res.failed.push(f.path.clone());
                continue;
            }
            res.wrote_file(target, backup.as_deref(), true);
            res.applied += 1;
        }
        Ok(res)
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn never_restores_identity_or_boot_files() {
        for p in [
            "/etc/fstab",
            "/etc/pam.d/system-auth",
            "/etc/ssh/ssh_host_ed25519_key",
            "/etc/x.pacnew",
            "/etc/limine.conf",
        ] {
            assert!(super::denied(p), "{p}");
        }
        for p in ["/etc/sysctl.d/99-x.conf", "/etc/pacman.d/hooks/spotx.hook"] {
            assert!(!super::denied(p), "{p}");
        }
    }
}
