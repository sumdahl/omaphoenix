use std::fs;
use std::io::{Read, Write};
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::str::FromStr;

use age::secrecy::{ExposeSecret, SecretString};
use zeroize::Zeroizing;

use super::vault::write_private;
use super::{SECRETS_DIR, age_err, is_public_half};
use crate::category::Error;
use crate::category::files::{Walk, exists, walk};

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

/// Creates the vault key pair in `dir`: the recipient in clear, the identity under `passphrase`.
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

// What ssh insists on: a private directory, private keys owner-only.
pub(super) fn fix_ssh_perms(dir: &Path) {
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
