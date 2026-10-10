use std::fs;
use std::io::{Read, Write};
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::{Component, Path, PathBuf};

use zeroize::Zeroizing;

use super::keys::unlock_identity;
use super::{MAX_SECRET_FILE, age_err, secrets_path};
use crate::category::files::exists;
use sha2::{Digest, Sha256};

use crate::category::{Env, Error, Outcome, hex, stamp};

/// A reproducible tar (sorted, real modes, no times or owners) so identical secrets hash the same.
pub(super) fn tar_files(home: &Path, ids: &[&str]) -> Result<Zeroizing<Vec<u8>>, Error> {
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

/// A dry run can't decrypt, but the files on disk hashing like the vault proves nothing would change.
pub(super) fn plan_vault(env: &Env<'_>, all: &[&str], wanted: &[&str], res: &mut Outcome) {
    let sum = fs::read_to_string(secrets_path(env, "vault.sha256")).unwrap_or_default();
    let unchanged =
        tar_files(&env.home, all).is_ok_and(|t| hex(&Sha256::digest(t.as_slice())) == sum.trim());
    if unchanged {
        res.present += wanted.len();
        return;
    }
    res.planned.extend(
        wanted
            .iter()
            .map(|n| format!("decrypt ~/{n} (written only if it differs)")),
    );
}

pub(super) fn write_private(path: &Path, body: &[u8]) -> std::io::Result<()> {
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

pub(super) fn open_vault(env: &Env<'_>, vault: &Path, res: &mut Outcome) -> Result<(), Error> {
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
