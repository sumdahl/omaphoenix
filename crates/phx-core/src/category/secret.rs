//! `secret`: SSH keys and other credential files, opt-in only. Public
//! halves (ssh config, `known_hosts`, `*.pub`) are stored plain; everything
//! else goes into one age-encrypted vault.
//!
//! The vault is encrypted to an age X25519 recipient, so unattended timer
//! runs never need the passphrase. The matching identity is stored in the
//! repo encrypted with the user's passphrase (scrypt), asked for only on
//! restore. Plaintext never touches disk, and its buffers are wiped.

use std::fs;
use std::io::{Read, Write};
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::{Component, Path, PathBuf};
use std::str::FromStr;

use age::secrecy::{ExposeSecret, SecretString};
use phx_contract::Item;
use sha2::{Digest, Sha256};
use zeroize::Zeroizing;

use super::config::restore_tree;
use super::files::{Walk, copy_file, exists, is_secret, walk};
use super::{Category, Env, Error, Outcome, hex, stamp};

/// `secret`.
pub struct Secret;

/// Directory (inside the repo) holding the vault and its keys.
pub const SECRETS_DIR: &str = "secrets";

/// The vault holds key files, not data: anything bigger is refused.
const MAX_SECRET_FILE: u64 = 1 << 20;

fn secrets_path(env: &Env<'_>, name: &str) -> PathBuf {
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

fn age_err(e: impl std::fmt::Display) -> Error {
    Error::Other(e.to_string())
}

/// A reproducible tar: sorted names, real modes, no times or owners, so
/// identical secrets always hash the same.
fn tar_files(home: &Path, ids: &[&str]) -> Result<Zeroizing<Vec<u8>>, Error> {
    let mut b = tar::Builder::new(Vec::new());
    for id in ids {
        let path = home.join(id);
        let meta = fs::metadata(&path)?;
        if meta.len() > MAX_SECRET_FILE {
            return Err(Error::Other(format!(
                "{id} is over 1 MB; the vault holds keys, not data"
            )));
        }
        let body = Zeroizing::new(fs::read(&path)?);
        let mut h = tar::Header::new_ustar();
        h.set_path(id)?;
        h.set_mode(meta.permissions().mode() & 0o777);
        h.set_size(body.len() as u64);
        h.set_mtime(0);
        h.set_uid(0);
        h.set_gid(0);
        h.set_cksum();
        b.append(&h, body.as_slice())?;
    }
    Ok(Zeroizing::new(b.into_inner()?))
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
        // age output is randomised; re-encrypting unchanged secrets would
        // make every timer run a new snapshot.
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
            if env.dry_run {
                let names = fs::read_to_string(secrets_path(env, "vault.txt")).unwrap_or_default();
                res.planned
                    .extend(names.split_whitespace().map(|n| format!("decrypt ~/{n}")));
            } else if let Err(e) = open_vault(env, &vault, &mut res) {
                res.failed.push(e.to_string());
            }
        }
        if !env.dry_run && res.applied > 0 {
            fix_ssh_perms(&env.home.join(".ssh"));
        }
        Ok(res)
    }
}

fn write_private(path: &Path, body: &[u8]) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)?;
    }
    let mut f = fs::File::options()
        .create(true)
        .write(true)
        .truncate(true)
        .mode(0o600)
        .open(path)?;
    f.write_all(body)?;
    fs::set_permissions(path, fs::Permissions::from_mode(0o600))
}

fn open_vault(env: &Env<'_>, vault: &Path, res: &mut Outcome) -> Result<(), Error> {
    let ask = env.passphrase.ok_or_else(|| {
        Error::Other("encrypted secrets need a passphrase; run restore from a terminal or set OMAPHOENIX_PASSPHRASE".into())
    })?;
    let pass = ask().map_err(Error::Other)?;
    let identity = unlock_identity(&secrets_path(env, "identity.age"), &pass)?;
    let decryptor =
        age::Decryptor::new(std::io::BufReader::new(fs::File::open(vault)?)).map_err(age_err)?;
    let reader = decryptor
        .decrypt(std::iter::once(&identity as &dyn age::Identity))
        .map_err(|e| Error::Other(format!("decrypt vault: {e}")))?;
    let stamp = stamp();
    let mut archive = tar::Archive::new(reader);
    for entry in archive.entries()? {
        let mut entry = entry?;
        let name = entry.path()?.into_owned();
        if name
            .components()
            .any(|c| !matches!(c, Component::Normal(_)))
        {
            return Err(Error::Other(format!(
                "refusing vault entry {:?}",
                name.display().to_string()
            )));
        }
        let mode = entry.header().mode().unwrap_or(0o600) & 0o777;
        let mut body = Zeroizing::new(Vec::new());
        (&mut entry)
            .take(MAX_SECRET_FILE + 1)
            .read_to_end(&mut body)?;
        let name_s = name.to_string_lossy().into_owned();
        if !env.want(&name_s) {
            continue;
        }
        let target = env.home.join(&name);
        if fs::read(&target).is_ok_and(|cur| Zeroizing::new(cur).as_slice() == body.as_slice()) {
            res.present += 1;
            continue;
        }
        let mut backup = None;
        if exists(&target) {
            let b = PathBuf::from(format!("{}.pre-restore.{stamp}", target.display()));
            if let Err(e) = fs::rename(&target, &b) {
                res.failed.push(format!("{name_s}: {e}"));
                continue;
            }
            backup = Some(b);
        }
        if let Some(dir) = target.parent() {
            use std::os::unix::fs::DirBuilderExt;
            fs::DirBuilder::new()
                .recursive(true)
                .mode(0o700)
                .create(dir)?;
        }
        let written = fs::File::options()
            .create(true)
            .write(true)
            .truncate(true)
            .mode(mode)
            .open(&target)
            .and_then(|mut f| f.write_all(&body));
        match written {
            Ok(()) => {
                res.wrote_file(&target, backup.as_deref(), false);
                res.applied += 1;
            }
            Err(e) => res.failed.push(format!("{name_s}: {e}")),
        }
    }
    Ok(())
}

/// Decrypts the vault's X25519 identity with the passphrase.
///
/// # Errors
/// On a wrong passphrase or a damaged key file.
pub fn unlock_identity(file: &Path, passphrase: &str) -> Result<age::x25519::Identity, Error> {
    let scrypt = age::scrypt::Identity::new(SecretString::from(passphrase.to_owned()));
    let decryptor =
        age::Decryptor::new(std::io::BufReader::new(fs::File::open(file)?)).map_err(age_err)?;
    let mut reader = decryptor
        .decrypt(std::iter::once(&scrypt as &dyn age::Identity))
        .map_err(|_| Error::Other("wrong passphrase (or the vault key is damaged)".into()))?;
    let mut key = Zeroizing::new(String::new());
    reader.read_to_string(&mut key)?;
    age::x25519::Identity::from_str(key.trim()).map_err(age_err)
}

/// Creates the vault key pair in `dir`: the public recipient in clear, the
/// private identity encrypted with `passphrase`.
///
/// # Errors
/// When already set up, or on I/O failure.
pub fn setup_secrets(dir: &Path, passphrase: &str) -> Result<(), Error> {
    if exists(&dir.join("identity.age")) {
        return Err(Error::Other("secrets are already set up".into()));
    }
    let id = age::x25519::Identity::generate();
    let encryptor = age::Encryptor::with_user_passphrase(SecretString::from(passphrase.to_owned()));
    let mut out = Vec::new();
    let mut w = encryptor.wrap_output(&mut out)?;
    w.write_all(format!("{}\n", id.to_string().expose_secret()).as_bytes())?;
    w.finish()?;
    {
        use std::os::unix::fs::DirBuilderExt;
        fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(dir)?;
    }
    write_private(&dir.join("identity.age"), &out)?;
    fs::write(dir.join("recipient.txt"), format!("{}\n", id.to_public()))?;
    Ok(())
}

/// Private keys in `~/.ssh` (files that are not public halves).
#[must_use]
pub fn private_ssh_keys(home: &Path) -> usize {
    fs::read_dir(home.join(".ssh"))
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| {
            e.file_type().is_ok_and(|t| t.is_file())
                && !is_public_half(&e.file_name().to_string_lossy())
        })
        .count()
}

/// Whether `secrets init` has run for the repo at `dir`.
#[must_use]
pub fn secrets_ready(dir: &Path) -> bool {
    exists(&dir.join(SECRETS_DIR).join("recipient.txt"))
}

/// What ssh insists on: a private directory, private keys owner-only.
fn fix_ssh_perms(dir: &Path) {
    if !exists(dir) {
        return;
    }
    let _ = fs::set_permissions(dir, fs::Permissions::from_mode(0o700));
    walk(dir, &mut |p, meta| {
        if meta.file_type().is_symlink() {
            return Walk::Continue;
        }
        if meta.is_dir() {
            return if p == dir { Walk::Continue } else { Walk::Skip }; // agent sockets etc. aren't ours
        }
        let mode = if is_public_half(&p.to_string_lossy()) {
            0o644
        } else {
            0o600
        };
        let _ = fs::set_permissions(p, fs::Permissions::from_mode(mode));
        Walk::Continue
    });
}

#[cfg(test)]
mod tests {
    use super::{Secret, setup_secrets};
    use crate::category::{Category, Env};
    use phx_contract::Item;
    use std::fs;
    use std::os::unix::fs::PermissionsExt;

    #[test]
    fn vault_round_trip_never_stores_plaintext() {
        let t = tempfile::tempdir().unwrap_or_else(|e| unreachable!("{e}"));
        let (home, tree) = (t.path().join("home"), t.path().join("tree"));
        fs::create_dir_all(home.join(".ssh")).ok();
        fs::write(
            home.join(".ssh/id_ed25519"),
            "-----BEGIN OPENSSH PRIVATE KEY-----\nfake\n",
        )
        .ok();
        fs::write(home.join(".ssh/id_ed25519.pub"), "ssh-ed25519 AAAA").ok();
        setup_secrets(&tree.join("secrets"), "correct horse battery")
            .unwrap_or_else(|e| unreachable!("{e}"));
        let log = |_: &str, _: &str| {};
        let env = Env::new(&home, &tree, &log);
        let items = vec![
            Item::new(".ssh/id_ed25519", ""),
            Item::new(".ssh/id_ed25519.pub", ""),
        ];
        Secret
            .capture(&env, &items)
            .unwrap_or_else(|e| unreachable!("{e}"));
        let vault = fs::read(tree.join("secrets/vault.age")).unwrap_or_default();
        assert!(
            !vault.windows(11).any(|w| w == b"PRIVATE KEY"),
            "vault is encrypted"
        );
        let before = fs::read(tree.join("secrets/vault.age")).unwrap_or_default();
        Secret.capture(&env, &items).ok();
        assert_eq!(
            fs::read(tree.join("secrets/vault.age")).unwrap_or_default(),
            before,
            "unchanged secrets aren't re-encrypted"
        );

        let fresh = t.path().join("fresh");
        let wrong = || Ok(zeroize::Zeroizing::new("wrong passphrase!".to_owned()));
        let mut env2 = Env::new(&fresh, &tree, &log);
        env2.passphrase = Some(&wrong);
        let res = Secret.restore(&env2).unwrap_or_default();
        assert!(
            res.failed.iter().any(|f| f.contains("wrong passphrase")),
            "{:?}",
            res.failed
        );

        let right = || Ok(zeroize::Zeroizing::new("correct horse battery".to_owned()));
        env2.passphrase = Some(&right);
        let res = Secret.restore(&env2).unwrap_or_default();
        assert_eq!(res.failed, Vec::<String>::new());
        assert_eq!(
            fs::read_to_string(fresh.join(".ssh/id_ed25519"))
                .ok()
                .as_deref(),
            Some("-----BEGIN OPENSSH PRIVATE KEY-----\nfake\n")
        );
        let mode =
            |p: &str| fs::metadata(fresh.join(p)).map_or(0, |m| m.permissions().mode() & 0o777);
        assert_eq!(
            (
                mode(".ssh"),
                mode(".ssh/id_ed25519"),
                mode(".ssh/id_ed25519.pub")
            ),
            (0o700, 0o600, 0o644)
        );
    }
}
