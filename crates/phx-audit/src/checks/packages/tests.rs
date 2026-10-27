use crate::checks::testing::{FakeSystem, msgs};
use crate::contract::Severity::{Fail, Info, Pass, Skip, Warn};

#[test]
fn patterns_compile() {
    assert!(
        super::SECURITY_PKGS.is_some()
            && super::WEAK_SIGLEVEL.is_some()
            && super::PIPE_TO_SHELL.is_some()
    );
}

#[test]
fn up_to_date_machine() {
    let sys = FakeSystem::new();
    sys.bin("checkupdates");
    sys.cmd("uname -r", true, "6.1.0\n");
    sys.dir_mode("/usr/lib/modules/6.1.0", 0o755);
    sys.file(
        "/etc/pacman.conf",
        "[options]\nSigLevel = Required DatabaseOptional\n",
    );
    let got = msgs(&sys.ctx(false), super::SECTION);
    assert_eq!(
        got,
        vec![
            (Pass, "No pending repo updates".into()),
            (
                Info,
                "arch-audit not installed (no CVE matching of installed packages)".into()
            ),
            (
                Pass,
                "Running kernel modules present (no pending reboot for kernel)".into()
            ),
            (Pass, "pacman signature checking is enforced".into()),
        ]
    );
}

#[test]
fn neglected_machine() {
    let sys = FakeSystem::new();
    sys.bin("arch-audit");
    sys.cmd(
        "arch-audit -u",
        true,
        "openssl is affected by CVE-1. High risk!\n",
    );
    sys.cmd("uname -r", true, "6.2.0\n");
    sys.cmd("pacman -Qdtq", true, "a\nb\n");
    sys.cmd("pacman -Qmq", true, "yay\nspotx\n");
    sys.file("/etc/pacman.conf", "SigLevel = Never\n");
    sys.file(
        "/etc/pacman.d/hooks/a.hook",
        "[Action]\nExec = /usr/local/bin/upd --x\n",
    );
    sys.file(
        "/etc/pacman.d/hooks/b.hook",
        "[Action]\nExec = /usr/bin/true\n",
    );
    sys.file("/usr/local/bin/upd", "curl -s https://x | bash\n");
    let ctx = sys.ctx(false);
    let got = msgs(&ctx, super::SECTION);
    for want in [
        (Skip, "checkupdates missing (pacman-contrib)"),
        (Fail, "arch-audit: vulnerable packages with fixes available"),
        (
            Warn,
            "Running kernel 6.2.0 has no modules dir: kernel was upgraded, reboot needed",
        ),
        (Info, "2 orphaned packages (pacman -Qdtq)"),
        (
            Info,
            "2 foreign/AUR packages, not covered by repo signing: yay spotx",
        ),
        (Fail, "pacman.conf has a weak SigLevel"),
        (Info, "Custom pacman hooks (run as root on transactions):"),
        (
            Warn,
            "a.hook: /usr/local/bin/upd pipes a remote download into a shell as root",
        ),
    ] {
        assert!(
            got.contains(&(want.0, want.1.into())),
            "missing {want:?} in {got:#?}"
        );
    }
    let report = crate::audit::run(&ctx, &[super::SECTION]);
    let warn = report
        .findings()
        .iter()
        .find(|f| f.id.as_deref() == Some("packages.hook_pipes_to_shell"));
    assert_eq!(
        warn.map(|f| f.detail.as_str()),
        Some("b.hook -> /usr/bin/true"),
        "Bash S6 quirk kept"
    );
    let cve = report
        .findings()
        .iter()
        .find(|f| f.id.as_deref() == Some("packages.known_cve"));
    assert_eq!(cve.map(|f| f.targets.len()), Some(1));
}
