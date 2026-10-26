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
        // bootctl prints e.g. "disabled (unknown)" or "enabled (user)": state, then firmware mode
        // sed -n 's/.*(\(.*\)).*/\1/p': the text inside the last parentheses' outermost pair.
        let mode = match (sb.find('('), sb.rfind(')')) {
            (Some(open), Some(close)) if open < close => {
                sb.get(open + 1..close).unwrap_or_default()
            }
            _ => "",
        };
        if sb.starts_with("enabled") {
            let shown = if mode.is_empty() {
                String::new()
            } else {
                format!(" ({mode} mode)")
            };
            r.ok("boot.secure_boot", format!("Secure Boot is enabled{shown}"));
        } else if sb.is_empty() {
            r.skip("boot.secure_boot", "Secure Boot state unknown");
        } else {
            let msg = if mode == "setup" {
                "Secure Boot is off; firmware is in Setup Mode, ready for your own keys"
            } else {
                "Secure Boot is off, so a tampered bootloader or kernel would still boot"
            };
            r.warn("boot.secure_boot", msg)
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
mod tests;
