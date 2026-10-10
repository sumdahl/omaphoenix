//! `secret`: opt-in credential files; public halves stored plain, the rest in an age vault.

mod keys;
#[cfg(test)]
mod tests;
mod vault;

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::str::FromStr;

use phx_contract::Item;
use sha2::{Digest, Sha256};

use super::config::restore_tree;
use super::files::{Walk, copy_file, exists, is_secret, walk};
use super::{Category, Env, Error, Outcome, hex};
use vault::{open_vault, plan_vault, tar_files, write_private};

pub use keys::{private_ssh_keys, secrets_ready, setup_secrets, unlock_identity};

/// `secret`.
pub struct Secret;

/// Directory (inside the repo) holding the vault and its keys.
pub const SECRETS_DIR: &str = "secrets";

/// The vault holds key files, not data: anything bigger is refused.
pub(super) const MAX_SECRET_FILE: u64 = 1 << 20;

pub(super) fn secrets_path(env: &Env<'_>, name: &str) -> PathBuf {
    env.tree.join(SECRETS_DIR).join(name)
}

fn plain_dir(env: &Env<'_>) -> PathBuf {
    env.tree.join("files/secret-plain")
}

/// A public half: safe to store unencrypted.
#[must_use]
pub fn is_public_half(id: &str) -> bool {
    let base = Path::new(id)
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    base.ends_with(".pub")
        || base == "config"
        || base.starts_with("known_hosts")
        || base == "authorized_keys"
}

pub(super) fn age_err(e: impl std::fmt::Display) -> Error {
    Error::Other(e.to_string())
}

impl Category for Secret {
    fn name(&self) -> &'static str {
        "secret"
    }

    fn detect(&self, env: &Env<'_>) -> Result<Vec<Item>, Error> {
        let mut items = Vec::new();
        walk(&env.home.join(".ssh"), &mut |p, meta| {
            if meta.is_file()
                && let Ok(rel) = p.strip_prefix(&env.home)
            {
                items.push(Item::new(rel.to_string_lossy(), ""));
            }
            Walk::Continue
        });
        let mut home: Vec<_> = fs::read_dir(&env.home)
            .into_iter()
            .flatten()
            .flatten()
            .collect();
        home.sort_by_key(fs::DirEntry::file_name);
        for e in home {
            let name = e.file_name().to_string_lossy().into_owned();
            if e.file_type().is_ok_and(|t| t.is_file()) && name.starts_with('.') && is_secret(&name)
            {
                items.push(Item::new(name, ""));
            }
        }
        Ok(items)
    }

    fn capture(&self, env: &Env<'_>, items: &[Item]) -> Result<(), Error> {
        let mut vault_ids = Vec::new();
        for it in items {
            if is_public_half(&it.id) {
                copy_file(&env.home.join(&it.id), &plain_dir(env).join(&it.id), 0o644)?;
            } else {
                vault_ids.push(it.id.as_str());
            }
        }
        let (vault, sum, list) = (
            secrets_path(env, "vault.age"),
            secrets_path(env, "vault.sha256"),
            secrets_path(env, "vault.txt"),
        );
        if vault_ids.is_empty() {
            for f in [&vault, &sum, &list] {
                let _ = fs::remove_file(f);
            }
            return Ok(());
        }
        vault_ids.sort_unstable();
        let plain = tar_files(&env.home, &vault_ids)?;
        let digest = hex(&Sha256::digest(plain.as_slice()));
        // age output is randomised; re-encrypting unchanged secrets would make a new snapshot.
        if fs::read_to_string(&sum).is_ok_and(|old| old.trim() == digest) && exists(&vault) {
            return Ok(());
        }
        let Ok(rcpt) = fs::read_to_string(secrets_path(env, "recipient.txt")) else {
            env.warn(&format!("{} secret file(s) selected but secrets are not set up; run: omaphoenix secrets init", vault_ids.len()));
            return Ok(());
        };
        let recipient = age::x25519::Recipient::from_str(rcpt.trim()).map_err(age_err)?;
        let encryptor =
            age::Encryptor::with_recipients(std::iter::once(&recipient as &dyn age::Recipient))
                .map_err(age_err)?;
        let mut out = Vec::new();
        let mut w = encryptor.wrap_output(&mut out)?;
        w.write_all(&plain)?;
        w.finish()?;
        write_private(&vault, &out)?;
        fs::write(&list, format!("{}\n", vault_ids.join("\n")))?;
        fs::write(&sum, format!("{digest}\n"))?;
        Ok(())
    }

    fn restore(&self, env: &Env<'_>) -> Result<Outcome, Error> {
        let mut res = restore_tree(env, &plain_dir(env), &env.home);
        let vault = secrets_path(env, "vault.age");
        if exists(&vault) {
            let names = fs::read_to_string(secrets_path(env, "vault.txt")).unwrap_or_default();
            let all: Vec<&str> = names.split_whitespace().collect();
            let wanted: Vec<&str> = all.iter().copied().filter(|n| env.want(n)).collect();
            // Only an encrypted item in scope may ask for the passphrase.
            if !wanted.is_empty() {
                if env.dry_run {
                    plan_vault(env, &all, &wanted, &mut res);
                } else if let Err(e) = open_vault(env, &vault, &mut res) {
                    res.failed.push(e.to_string());
                }
            }
        }
        if !env.dry_run && res.applied > 0 {
            keys::fix_ssh_perms(&env.home.join(".ssh"));
        }
        Ok(res)
    }
}
