//! "Boot & disk": LUKS, Secure Boot, bootloader config perms, swap.

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;

use crate::audit::{Ctx, Report, Section, shell_quote};

pub(super) const SECTION: Section = Section {
    name: "Boot & disk",
    run,
};

const LIMINE: &[&str] = &[
    "/boot/limine.conf",
    "/boot/EFI/limine/limine.conf",
    "/boot/limine/limine.conf",
];

fn run(ctx: &Ctx, r: &mut Report) {
    if ctx
        .run(&["lsblk", "-no", "FSTYPE"])
        .stdout
        .contains("crypto_LUKS")
    {
        r.ok("boot.luks", "LUKS full-disk encryption in use");
        if ctx.is_root && ctx.have("cryptsetup") {
            luks_headers(ctx, r);
        }
    } else {
        r.fail("boot.luks", "No LUKS encrypted volumes found")
            .hint("Disk encryption needs a reinstall; Omarchy's installer sets up LUKS by default");
    }

    if ctx.have("bootctl") {
        let status = ctx.run(&["bootctl", "status"]).stdout;
        // awk -F': ' '/Secure Boot/{print $2; exit}'
        let sb = status
            .lines()
            .find(|l| l.contains("Secure Boot"))
            .and_then(|l| l.split(": ").nth(1))
            .unwrap_or_default()
            .to_owned();
        if sb.contains("enabled") {
            r.ok("boot.secure_boot", format!("Secure Boot: {sb}"));
        } else if sb.is_empty() {
            r.skip("boot.secure_boot", "Secure Boot state unknown");
        } else {
            r.warn("boot.secure_boot", format!("Secure Boot: {sb} (evil-maid/bootkit protection is weaker)"))
                .hint("Enroll your own keys with sbctl and sign Limine: wiki.archlinux.org/title/Secure_Boot#sbctl");
        }
    }

    for abs in LIMINE {
        let path = ctx.sys(abs);
        let Ok(meta) = fs::symlink_metadata(&path) else {
            continue;
        };
        if !path.is_file() {
            continue;
        }
        let mode = meta.permissions().mode() & 0o7777;
        if mode & 0o002 != 0 {
            r.fail(
                "boot.limine_perms",
                format!("{abs} is world-writable ({mode:o})"),
            )
            .file(Path::new(abs))
            .fix(format!("sudo chmod 600 {}", shell_quote(abs)));
        } else {
            r.ok("boot.limine_perms", format!("{abs} perms {mode:o}"));
        }
    }

    for line in ctx
        .run(&["swapon", "--show=NAME,TYPE", "--noheadings"])
        .stdout
        .lines()
    {
        let mut words = line.split_whitespace();
        let Some(name) = words.next() else { continue };
        let kind = words.next().unwrap_or_default();
        if name.starts_with("/dev/zram") {
            r.ok("boot.swap", format!("Swap {name} is zram (RAM-backed)"));
        } else if kind == "file" {
            r.ok(
                "boot.swap",
                format!("Swap file {name} lives on the encrypted root"),
            );
        } else {
            r.info(
                "boot.swap",
                format!("Swap {name} ({kind}): confirm it is on an encrypted device"),
            );
        }
    }
}

fn luks_headers(ctx: &Ctx, r: &mut Report) {
    let devices: Vec<String> = ctx
        .run(&["lsblk", "-lnpo", "NAME,FSTYPE"])
        .stdout
        .lines()
        .filter_map(|l| {
            let mut w = l.split_whitespace();
            let name = w.next()?;
            (w.next() == Some("crypto_LUKS")).then(|| name.to_owned())
        })
        .collect();
    for dev in devices {
        let header = ctx.run(&["cryptsetup", "luksDump", &dev]).stdout;
        let version = header
            .lines()
            .find_map(|l| l.strip_prefix("Version:"))
            .map(|v| v.split_whitespace().next().unwrap_or_default().to_owned())
            .unwrap_or_default();
        if version == "2" {
            r.ok("boot.luks_version", format!("{dev} is LUKS2"));
        } else {
            r.warn(
                "boot.luks_version",
                format!("{dev} is LUKS{version} (LUKS2 recommended)"),
            )
            .hint(format!(
                "Back up the header, then from a live USB: cryptsetup convert --type luks2 {dev}"
            ));
        }
        let argon = header.lines().any(|l| {
            l.split_once("PBKDF:")
                .is_some_and(|(_, v)| v.trim_start().starts_with("argon2id"))
        });
        if argon {
            r.ok("boot.luks_pbkdf", format!("{dev} uses argon2id"));
        } else {
            r.warn(
                "boot.luks_pbkdf",
                format!("{dev} keyslot not using argon2id"),
            )
            .fix(format!(
                "sudo cryptsetup luksConvertKey --pbkdf argon2id {}",
                shell_quote(&dev)
            ));
        }
    }
}

#[cfg(test)]
mod tests {
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
                    "Secure Boot: disabled (setup) (evil-maid/bootkit protection is weaker)".into()
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
}
