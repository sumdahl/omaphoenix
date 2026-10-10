use crate::checks::testing::{FakeSystem, msgs};
use crate::contract::Severity::{Fail, Info, Pass, Skip, Warn};

const PASSWD: &str = "root:x:0:0::/root:/bin/bash\nbin:x:1:1::/:/usr/bin/nologin\nu:x:1000:1000::/home/u:/bin/zsh\nsvc:x:1001:1001::/:/usr/bin/false\n";

#[test]
fn as_user_skips_root_checks() {
    let sys = FakeSystem::new();
    sys.file("/etc/passwd", PASSWD);
    sys.file(
        "/etc/group",
        "wheel:x:998:u\ndocker:x:970:u\ninput:x:97:brltty,u\nwireshark:x:150:\n",
    );
    let got = msgs(&sys.ctx(false), super::SECTION);
    assert_eq!(
        got,
        vec![
            (Pass, "Only root has UID 0".into()),
            (Skip, "Empty/locked password check (needs sudo)".into()),
            (Info, "Interactive human accounts: u".into()),
            (Info, "wheel group: u".into()),
            (
                Warn,
                "docker group (root-equivalent via the daemon socket): u".into()
            ),
            (Warn, "input group (can read all keystrokes): u".into()),
            (Skip, "sudoers review (needs sudo)".into()),
        ]
    );
}

#[test]
fn as_root_reads_shadow_and_sudoers() {
    let sys = FakeSystem::new();
    sys.file("/etc/passwd", &format!("{PASSWD}toor:x:0:0::/:/bin/sh\n"));
    sys.file(
        "/etc/shadow",
        "root:!*:1::::::\nu:$1$salt$h:1::::::\nguest::1::::::\n",
    );
    sys.file(
        "/etc/sudoers",
        "# u ALL=(ALL) NOPASSWD: ALL\nroot ALL=(ALL) ALL\n",
    );
    sys.file(
        "/etc/sudoers.d/10-u",
        "u ALL=(ALL) NOPASSWD: /usr/bin/pacman\nDefaults:u !authenticate\n",
    );
    sys.file("/etc/security/faillock.conf", "# deny = 9\ndeny = 5\n");
    sys.file("/etc/sddm.conf.d/autologin.conf", "[Autologin]\nUser = u\n");
    sys.file(
        "/etc/systemd/system/getty@tty1.service.d/a.conf",
        "ExecStart=-/sbin/agetty --autologin u\n",
    );
    sys.cmd("visudo -c", false, "");
    let ctx = sys.ctx(true);
    let got = msgs(&ctx, super::SECTION);
    for want in [
        (Fail, "Multiple UID 0 accounts: root toor"),
        (Fail, "Accounts with empty passwords: guest"),
        (Pass, "root password login locked"),
        (
            Warn,
            "Accounts with legacy password hashes (md5/sha256/bcrypt-2a): u",
        ),
        (Warn, "NOPASSWD sudo rules:"),
        (Fail, "sudo authentication disabled (!authenticate)"),
        (Fail, "sudoers has syntax errors"),
        (Info, "faillock deny=5"),
        (
            Info,
            "Autologin enabled (User=ugetty) - acceptable only because the LUKS passphrase gates boot",
        ),
    ] {
        assert!(
            got.contains(&(want.0, want.1.into())),
            "missing {want:?} in {got:#?}"
        );
    }
    let report = crate::audit::run(&ctx, &[super::SECTION]);
    let nopw = report
        .findings()
        .iter()
        .find(|f| f.id.as_deref() == Some("users.sudo_nopasswd"));
    let nopw = nopw.unwrap_or_else(|| unreachable!("nopasswd finding"));
    assert_eq!(
        nopw.detail, "u ALL=(ALL) NOPASSWD: /usr/bin/pacman",
        "comments ignored"
    );
    assert_eq!(
        nopw.targets,
        vec![crate::Target::File {
            path: "/etc/sudoers.d/10-u".into()
        }]
    );
}
