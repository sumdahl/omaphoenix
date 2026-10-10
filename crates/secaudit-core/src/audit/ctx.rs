use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use super::{CmdOut, CommandRunner, System, read_text};

/// Who and what is being audited.
#[derive(Clone, Debug)]
#[non_exhaustive]
pub struct Ctx {
    /// The audited user's home. Under sudo this is the invoking user's.
    pub home: PathBuf,
    /// The audited user's name.
    pub user: String,
    /// Running as root, so root-only checks run instead of being skipped.
    pub is_root: bool,
    /// `--deep`: slower, wider scans.
    pub deep: bool,
    /// `$PATH` of the audit, whose directories are checked.
    pub path_env: String,
    /// Where `/` is; tests point it at a fake system tree.
    pub root: PathBuf,
    /// How external commands run.
    pub cmd: Arc<dyn CommandRunner>,
}

impl Ctx {
    /// The real machine: `SUDO_USER`'s home under sudo, else `$USER`'s `$HOME`.
    #[must_use]
    pub fn from_env() -> Self {
        let sudo_user = env::var("SUDO_USER").ok();
        let user = sudo_user
            .clone()
            .or_else(|| env::var("USER").ok())
            .unwrap_or_default();
        let home = sudo_user
            .and_then(|u| passwd_home(&u))
            .or_else(|| env::var_os("HOME").map(PathBuf::from))
            .unwrap_or_default();
        Self {
            home,
            user,
            is_root: effective_uid() == Some(0),
            ..Self::for_home("")
        }
    }

    /// The real machine, auditing `name`'s home; `None` if no such user.
    #[must_use]
    pub fn for_user(name: &str) -> Option<Self> {
        let home = passwd_home(name)?;
        Some(Self {
            home,
            user: name.to_owned(),
            is_root: effective_uid() == Some(0),
            ..Self::for_home("")
        })
    }

    /// An audit of `home` as a normal user on the real system. For `--home` and tests.
    #[must_use]
    pub fn for_home(home: impl Into<PathBuf>) -> Self {
        Self {
            home: home.into(),
            user: env::var("USER").unwrap_or_default(),
            is_root: false,
            deep: false,
            path_env: env::var("PATH").unwrap_or_default(),
            root: PathBuf::from("/"),
            cmd: Arc::new(System),
        }
    }

    /// Whether `name` is an executable on [`Ctx::path_env`], as `command -v` checks.
    #[must_use]
    pub fn have(&self, name: &str) -> bool {
        use std::os::unix::fs::PermissionsExt;
        self.path_env.split(':').filter(|d| !d.is_empty()).any(|d| {
            fs::metadata(self.sys(d).join(name))
                .is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
        })
    }

    /// Runs a command; a failure to start counts as an unsuccessful, silent run.
    #[must_use]
    pub fn run(&self, argv: &[&str]) -> CmdOut {
        self.cmd.run(argv).unwrap_or_default()
    }

    /// Where absolute system path `abs` (e.g. `/etc/shadow`) is under [`Ctx::root`].
    #[must_use]
    pub fn sys(&self, abs: &str) -> PathBuf {
        self.root.join(abs.trim_start_matches('/'))
    }

    /// The system path as the report prints it, i.e. without [`Ctx::root`].
    #[must_use]
    pub fn unsys(&self, path: &Path) -> String {
        match path.strip_prefix(&self.root) {
            Ok(rest) => format!("/{}", rest.display()),
            Err(_) => path.display().to_string(),
        }
    }

    /// Lines of a system text file; empty if missing or unreadable.
    #[must_use]
    pub fn read_lines(&self, abs: &str) -> Vec<String> {
        read_text(&self.sys(abs))
            .lines()
            .map(str::to_owned)
            .collect()
    }

    /// Name for a uid or gid from `/etc/passwd` or `/etc/group`, or `UNKNOWN` as `stat` prints.
    #[must_use]
    pub fn id_name(&self, db: &str, id: u32) -> String {
        let id = id.to_string();
        self.read_lines(db)
            .iter()
            .find_map(|l| {
                let f: Vec<&str> = l.split(':').collect();
                (f.get(2) == Some(&id.as_str()))
                    .then(|| f.first().map(|n| (*n).to_owned()))
                    .flatten()
            })
            .unwrap_or_else(|| "UNKNOWN".to_owned())
    }

    /// `path` with the home prefix shown as `~`, as the report prints it.
    #[must_use]
    pub fn tilde(&self, path: &Path) -> String {
        match path.strip_prefix(&self.home) {
            Ok(rest) if !self.home.as_os_str().is_empty() => format!("~/{}", rest.display()),
            _ => path.display().to_string(),
        }
    }
}

/// Home directory of `user` from `/etc/passwd`.
fn passwd_home(user: &str) -> Option<PathBuf> {
    let passwd = fs::read_to_string("/etc/passwd").ok()?;
    passwd.lines().find_map(|line| {
        let f: Vec<&str> = line.split(':').collect();
        (f.first() == Some(&user))
            .then(|| f.get(5).map(PathBuf::from))
            .flatten()
    })
}

/// Effective uid from `/proc/self/status`, without `unsafe` libc calls.
fn effective_uid() -> Option<u32> {
    let status = fs::read_to_string("/proc/self/status").ok()?;
    let uids = status.lines().find_map(|l| l.strip_prefix("Uid:"))?;
    uids.split_whitespace().nth(1)?.parse().ok()
}

#[cfg(test)]
mod tests {
    use super::Ctx;
    use std::path::Path;

    #[test]
    fn sys_paths_map_under_root_and_back() {
        let mut ctx = Ctx::for_home("/home/u");
        ctx.root = "/tmp/fake".into();
        assert_eq!(ctx.sys("/etc/shadow"), Path::new("/tmp/fake/etc/shadow"));
        assert_eq!(ctx.unsys(Path::new("/tmp/fake/etc/shadow")), "/etc/shadow");
    }

    #[test]
    fn tilde_only_replaces_the_home_prefix() {
        let ctx = Ctx::for_home("/home/u");
        assert_eq!(ctx.tilde(Path::new("/home/u/.netrc")), "~/.netrc");
        assert_eq!(ctx.tilde(Path::new("/home/user2/x")), "/home/user2/x");
    }
}
