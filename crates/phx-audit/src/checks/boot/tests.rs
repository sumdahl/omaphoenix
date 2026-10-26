use crate::checks::testing::{FakeSystem, msgs};
use crate::contract::Severity::{Fail, Info, Pass, Skip, Warn};

#[test]
fn encrypted_laptop_as_root() {
    let sys = FakeSystem::new();
    sys.bin("cryptsetup");
    sys.bin("bootctl");
    sys.cmd("lsblk -no FSTYPE", true, "vfat\ncrypto_LUKS\nbtrfs\n");
    sys.cmd(
        "lsblk -lnpo NAME,FSTYPE",
        true,
        "/dev/nvme0n1p1 vfat\n/dev/nvme0n1p2 crypto_LUKS\n",
    );
    sys.cmd(
        "cryptsetup luksDump /dev/nvme0n1p2",
        true,
        "Version:       \t2\nKeyslots:\n  0: luks2\n\tPBKDF:      argon2id\n",
    );
    sys.cmd(
        "bootctl status",
        true,
        "System:\n     Secure Boot: disabled (setup)\n",
    );
    sys.cmd(
        "swapon --show=NAME,TYPE --noheadings",
        true,
        "/dev/zram0 partition\n/swap/swapfile file\n/dev/sda2 partition\n",
    );
    sys.file_mode("/boot/limine.conf", "", 0o644);
    let got = msgs(&sys.ctx(true), super::SECTION);
    assert_eq!(
        got,
        vec![
            (Pass, "LUKS full-disk encryption in use".into()),
            (Pass, "/dev/nvme0n1p2 is LUKS2".into()),
            (Pass, "/dev/nvme0n1p2 uses argon2id".into()),
            (
                Warn,
                "Secure Boot is off; firmware is in Setup Mode, ready for your own keys".into()
            ),
            (Pass, "/boot/limine.conf perms 644".into()),
            (Pass, "Swap /dev/zram0 is zram (RAM-backed)".into()),
            (
                Pass,
                "Swap file /swap/swapfile lives on the encrypted root".into()
            ),
            (
                Info,
                "Swap /dev/sda2 (partition): confirm it is on an encrypted device".into()
            ),
        ]
    );
}

#[test]
fn unencrypted_and_unknown() {
    let sys = FakeSystem::new();
    sys.bin("bootctl");
    sys.file_mode("/boot/limine.conf", "", 0o666);
    let got = msgs(&sys.ctx(false), super::SECTION);
    assert_eq!(
        got,
        vec![
            (Fail, "No LUKS encrypted volumes found".into()),
            (Skip, "Secure Boot state unknown".into()),
            (Fail, "/boot/limine.conf is world-writable (666)".into()),
        ]
    );
}

#[test]
fn secure_boot_states_as_manish_words_them() {
    for (status, want) in [
        (
            "enabled (user)",
            (Pass, "Secure Boot is enabled (user mode)"),
        ),
        ("enabled", (Pass, "Secure Boot is enabled")),
        (
            "disabled (unknown)",
            (
                Warn,
                "Secure Boot is off, so a tampered bootloader or kernel would still boot",
            ),
        ),
        (
            "disabled (setup)",
            (
                Warn,
                "Secure Boot is off; firmware is in Setup Mode, ready for your own keys",
            ),
        ),
    ] {
        let sys = FakeSystem::new();
        sys.bin("bootctl");
        sys.cmd(
            "bootctl status",
            true,
            &format!("System:\n     Secure Boot: {status}\n"),
        );
        let got = msgs(&sys.ctx(true), super::SECTION);
        assert!(got.contains(&(want.0, want.1.into())), "{status}: {got:?}");
    }
}
