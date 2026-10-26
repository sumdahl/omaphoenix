use std::fs;
use std::os::unix::fs::PermissionsExt;

use crate::checks::testing::{FakeSystem, msgs};
use crate::contract::Severity::{Fail, Info, Pass, Warn};

#[test]
fn clean_system() {
    let sys = FakeSystem::new();
    sys.file_mode("/etc/shadow", "", 0o600);
    sys.file_mode("/etc/sudoers", "", 0o440);
    sys.file_mode("/usr/bin/sudo", "", 0o4755);
    sys.cmd("pacman -Qoq /usr/bin/sudo", true, "sudo\n");
    let mut ctx = sys.ctx(false);
    ctx.path_env = "/usr/bin".into();
    let got = msgs(&ctx, super::SECTION);
    assert_eq!(got[0].0, Pass, "{got:?}");
    assert!(got[0].1.starts_with("/etc/shadow 600 "), "{got:?}");
    for want in [
        (Pass, "/etc/sudoers 440"),
        (Pass, "Home dir 700"),
        (Pass, "No world-writable files in /etc /usr /boot /opt"),
        (Pass, "All world-writable dirs have the sticky bit"),
        (
            Info,
            "Scanning /usr /opt /etc /home /var /srv for SUID/SGID binaries not owned by any package...",
        ),
        (Pass, "All SUID/SGID binaries belong to packages"),
    ] {
        assert!(
            got.contains(&(want.0, want.1.into())),
            "missing {want:?} in {got:#?}"
        );
    }
    assert_eq!(got.len(), 7, "{got:#?}");
}

#[test]
fn finds_every_problem() {
    let sys = FakeSystem::new();
    sys.file_mode("/etc/shadow", "", 0o644);
    sys.file_mode("/etc/sudoers", "", 0o644);
    sys.file_mode("/usr/share/x.conf", "", 0o666);
    sys.file_mode("/opt/tool/helper", "", 0o4755);
    sys.dir_mode("/srv/drop", 0o777);
    sys.dir_mode("/tmp", 0o1777);
    sys.dir_mode("/usr/local/bin", 0o777);
    let mut ctx = sys.ctx(false);
    fs::set_permissions(&ctx.home, fs::Permissions::from_mode(0o755)).ok();
    ctx.path_env = "/usr/local/bin:.".into();
    let got = msgs(&ctx, super::SECTION);
    assert!(
        got.iter()
            .any(|(s, m)| *s == Fail && m.ends_with("(should not be group/world readable)")),
        "{got:#?}"
    );
    for want in [
        (Fail, "/etc/sudoers 644"),
        (Warn, "Home dir 755 (others can read it)"),
        (Fail, "World-writable system files:"),
        (Warn, "World-writable dirs without sticky bit:"),
        (Fail, "SUID/SGID binaries not owned by any package:"),
        (
            Fail,
            "PATH dir /usr/local/bin is group/world-writable (777)",
        ),
        (Fail, "PATH contains the current directory"),
    ] {
        assert!(
            got.contains(&(want.0, want.1.into())),
            "missing {want:?} in {got:#?}"
        );
    }
    let report = crate::audit::run(&ctx, &[super::SECTION]);
    let dirs = report
        .findings()
        .iter()
        .find(|f| f.id.as_deref() == Some("filesystem.world_writable_dirs"));
    let detail: Vec<&str> = dirs.map(|f| f.detail.lines().collect()).unwrap_or_default();
    assert!(
        detail.contains(&"/srv/drop") && !detail.contains(&"/tmp"),
        "{detail:?}"
    );
}
