//! A fake machine for section tests.

use std::collections::HashMap;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::sync::{Arc, Mutex};

use crate::audit::{CmdOut, CommandRunner, Ctx, Section, run};
use crate::contract::Severity;

/// Commands answer from a table; unknown commands fail silently.
#[derive(Debug, Default)]
pub(crate) struct FakeCmd(Mutex<HashMap<String, CmdOut>>);

impl CommandRunner for FakeCmd {
    fn run(&self, argv: &[&str]) -> Option<CmdOut> {
        self.0
            .lock()
            .ok()?
            .get(&argv.join(" "))
            .cloned()
            .or_else(|| Some(CmdOut::default()))
    }
}

pub(crate) struct FakeSystem {
    root: tempfile::TempDir,
    home: tempfile::TempDir,
    cmd: Arc<FakeCmd>,
}

impl FakeSystem {
    pub(crate) fn new() -> Self {
        let tmp = || tempfile::tempdir().unwrap_or_else(|e| unreachable!("tempdir: {e}"));
        let sys = Self {
            root: tmp(),
            home: tmp(),
            cmd: Arc::default(),
        };
        fs::set_permissions(sys.home.path(), fs::Permissions::from_mode(0o700)).ok();
        sys
    }

    pub(crate) fn file_mode(&self, abs: &str, body: &str, mode: u32) {
        let p = self.root.path().join(abs.trim_start_matches('/'));
        write(&p, body, mode);
    }

    pub(crate) fn file(&self, abs: &str, body: &str) {
        self.file_mode(abs, body, 0o644);
    }

    pub(crate) fn dir_mode(&self, abs: &str, mode: u32) {
        let p = self.root.path().join(abs.trim_start_matches('/'));
        fs::create_dir_all(&p).ok();
        fs::set_permissions(&p, fs::Permissions::from_mode(mode)).ok();
    }

    pub(crate) fn home_file(&self, rel: &str, body: &str) {
        self.home_file_mode(rel, body, 0o644);
    }

    pub(crate) fn home_file_mode(&self, rel: &str, body: &str, mode: u32) {
        write(&self.home.path().join(rel), body, mode);
    }

    pub(crate) fn home_dir(&self, rel: &str, mode: u32) {
        let p = self.home.path().join(rel);
        fs::create_dir_all(&p).ok();
        fs::set_permissions(&p, fs::Permissions::from_mode(mode)).ok();
    }

    /// An executable on the fake `/usr/bin`, so `have` finds it.
    pub(crate) fn bin(&self, name: &str) {
        self.file_mode(&format!("/usr/bin/{name}"), "", 0o755);
    }

    pub(crate) fn cmd(&self, argv: &str, success: bool, stdout: &str) {
        if let Ok(mut t) = self.cmd.0.lock() {
            t.insert(argv.to_owned(), CmdOut::new(success, stdout));
        }
    }

    pub(crate) fn ctx(&self, is_root: bool) -> Ctx {
        let mut ctx = Ctx::for_home(self.home.path());
        ctx.root = self.root.path().to_path_buf();
        ctx.is_root = is_root;
        ctx.user = "u".into();
        ctx.path_env = "/usr/bin".into();
        ctx.cmd = self.cmd.clone();
        ctx
    }
}

fn write(p: &Path, body: &str, mode: u32) {
    if let Some(dir) = p.parent() {
        fs::create_dir_all(dir).ok();
    }
    fs::write(p, body).ok();
    fs::set_permissions(p, fs::Permissions::from_mode(mode)).ok();
}

/// (severity, message) of every finding the section reports.
pub(crate) fn msgs(ctx: &Ctx, section: Section) -> Vec<(Severity, String)> {
    run(ctx, &[section])
        .findings()
        .iter()
        .map(|f| (f.sev, f.msg.clone()))
        .collect()
}
