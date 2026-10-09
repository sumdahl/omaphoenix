//! "Packages & updates": pending updates, known CVEs, pending reboot,
//! orphans, foreign packages, pacman signature policy, root hooks.

use std::path::Path;
use std::sync::LazyLock;

use regex::Regex;

use crate::audit::{Ctx, Report, Section, read_text};
use crate::contract::Target;

pub(super) const SECTION: Section = Section {
    name: "Packages & updates",
    run,
};

static SECURITY_PKGS: LazyLock<Option<Regex>> = LazyLock::new(|| {
    Regex::new(
        r"(?i)^(linux|openssl|openssh|glibc|sudo|systemd|curl|gnutls|nss|firefox|chromium|brave)",
    )
    .ok()
});
static WEAK_SIGLEVEL: LazyLock<Option<Regex>> =
    LazyLock::new(|| Regex::new(r"^\s*SigLevel\s*=.*(Never|Optional TrustAll)").ok());
static PIPE_TO_SHELL: LazyLock<Option<Regex>> = LazyLock::new(|| Regex::new(r"\|\s*(ba)?sh").ok());

fn lines(s: &str) -> Vec<&str> {
    s.lines().filter(|l| !l.is_empty()).collect()
}

fn run(ctx: &Ctx, r: &mut Report) {
    updates(ctx, r);

    let kernel = ctx.run(&["uname", "-r"]).stdout.trim().to_owned();
    if ctx.sys(&format!("/usr/lib/modules/{kernel}")).is_dir() {
        r.ok(
            "packages.pending_reboot",
            "Running kernel modules present (no pending reboot for kernel)",
        );
    } else {
        r.warn(
            "packages.pending_reboot",
            format!(
                "Running kernel {kernel} has no modules dir: kernel was upgraded, reboot needed"
            ),
        )
        .fix("systemctl reboot");
    }

    let orphans = ctx.run(&["pacman", "-Qdtq"]).stdout;
    if !orphans.is_empty() {
        r.info(
            "packages.orphans",
            format!(
                "{} orphaned packages (pacman -Qdtq)",
                orphans.lines().count()
            ),
        );
    }
    let foreign = ctx.run(&["pacman", "-Qmq"]).stdout;
    if !foreign.is_empty() {
        let joined = foreign.lines().collect::<Vec<_>>().join(" ");
        // `cut -c1-200` under LC_ALL=C cuts bytes.
        let cut =
            String::from_utf8_lossy(joined.as_bytes().get(..200).unwrap_or(joined.as_bytes()))
                .into_owned();
        r.info(
            "packages.foreign",
            format!(
                "{} foreign/AUR packages, not covered by repo signing: {cut}",
                foreign.lines().count()
            ),
        );
    }

    signatures(ctx, r);
    hooks(ctx, r);
    if ctx.deep {
        integrity(ctx, r);
    }
}

fn updates(ctx: &Ctx, r: &mut Report) {
    if ctx.have("checkupdates") {
        let out = ctx.run(&["checkupdates"]).stdout;
        let pending = lines(&out);
        if pending.is_empty() {
            r.ok("packages.pending_updates", "No pending repo updates");
        } else {
            let security: Vec<&str> = SECURITY_PKGS
                .as_ref()
                .map(|re| pending.iter().copied().filter(|l| re.is_match(l)).collect())
                .unwrap_or_default();
            r.warn(
                "packages.pending_updates",
                format!("{} pending repo updates", pending.len()),
            )
            .fix("omarchy-update")
            .add_detail(&security.join("\n"));
        }
    } else {
        r.skip(
            "packages.pending_updates",
            "checkupdates missing (pacman-contrib)",
        );
    }

    if ctx.have("arch-audit") {
        let vulns = ctx.run(&["arch-audit", "-u"]).stdout;
        if vulns.is_empty() {
            r.ok(
                "packages.known_cve",
                "arch-audit: no known-vulnerable packages with fixes available",
            );
        } else {
            let f = r.fail(
                "packages.known_cve",
                "arch-audit: vulnerable packages with fixes available",
            );
            f.add_detail(vulns.trim_end()).fix("omarchy-update");
            for name in vulns.lines().filter_map(|l| l.split_whitespace().next()) {
                f.target(Target::Package {
                    name: name.to_owned(),
                });
            }
        }
    } else {
        r.info(
            "packages.known_cve",
            "arch-audit not installed (no CVE matching of installed packages)",
        )
        .fix("sudo pacman -S --needed arch-audit");
    }
}

fn signatures(ctx: &Ctx, r: &mut Report) {
    let conf = read_text(&ctx.sys("/etc/pacman.conf"));
    let weak = WEAK_SIGLEVEL
        .as_ref()
        .is_some_and(|re| conf.lines().any(|l| re.is_match(l)));
    if weak {
        let shown: Vec<String> = conf
            .lines()
            .enumerate()
            .filter(|(_, l)| l.trim_start().starts_with("SigLevel"))
            .map(|(i, l)| format!("{}:{l}", i + 1))
            .collect();
        r.fail("packages.weak_siglevel", "pacman.conf has a weak SigLevel")
            .file(Path::new("/etc/pacman.conf"))
            .add_detail(&shown.join("\n"))
            .hint("Set 'SigLevel = Required DatabaseOptional' in /etc/pacman.conf");
    } else {
        r.ok(
            "packages.weak_siglevel",
            "pacman signature checking is enforced",
        );
    }
}

/// `Exec = …` of a hook, trimmed and with quotes dropped, as `cut -d= -f2- | xargs` gives.
fn hook_exec(text: &str) -> String {
    let line = text
        .lines()
        .find(|l| l.starts_with("Exec"))
        .unwrap_or_default();
    let value = line.split_once('=').map_or("", |(_, v)| v);
    value
        .split_whitespace()
        .map(|w| w.replace(['"', '\''], ""))
        .collect::<Vec<_>>()
        .join(" ")
}

fn hooks(ctx: &Ctx, r: &mut Report) {
    let dir = ctx.sys("/etc/pacman.d/hooks");
    let mut hooks: Vec<_> = std::fs::read_dir(&dir)
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|e| e == "hook"))
        .collect();
    hooks.sort();
    if hooks.is_empty() {
        return;
    }
    r.info(
        "packages.hooks",
        "Custom pacman hooks (run as root on transactions):",
    );
    for hook in &hooks {
        let name = hook
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        let exe = hook_exec(&read_text(hook));
        // Bash's `detail` attaches to the latest finding, which may be an
        // earlier hook's warning (docs/10, S6).
        if let Some(f) = r.last() {
            f.add_detail(&format!("{name} -> {exe}"));
        }
        let bin = exe.split(' ').next().unwrap_or_default();
        let script = ctx.sys(bin);
        if !bin.is_empty() && script.is_file() {
            let body = read_text(&script);
            let downloads = body.contains("curl") || body.contains("wget");
            if downloads && PIPE_TO_SHELL.as_ref().is_some_and(|re| re.is_match(&body)) {
                r.warn(
                    "packages.hook_pipes_to_shell",
                    format!("{name}: {bin} pipes a remote download into a shell as root"),
                )
                .file(Path::new(bin))
                .file(Path::new(&ctx.unsys(hook)))
                .hint("Download the script once, review it, and run the local copy from the hook");
            }
        }
    }
}

fn integrity(ctx: &Ctx, r: &mut Report) {
    if !ctx.is_root {
        r.skip(
            "packages.integrity",
            "Package file integrity (pacman -Qkk) (needs sudo)",
        );
        return;
    }
    r.info(
        "packages.integrity_scan",
        "Verifying package file checksums (slow)...",
    );
    let out = ctx.run(&["pacman", "-Qkk"]).stdout;
    let changed: Vec<&str> = out
        .lines()
        .filter(|l| !l.contains(" 0 altered files") && !l.contains("backup file"))
        .filter(|l| l.contains("warning") || l.contains("altered"))
        .filter(|l| {
            !l.contains("(Modification time mismatch)") && !l.contains("(Permissions mismatch)")
        })
        .take(40)
        .collect();
    if changed.is_empty() {
        r.ok(
            "packages.integrity",
            "No altered package files outside backup/config files",
        );
    } else {
        r.warn("packages.integrity", "Package files differ from their package checksums (review):")
            .add_detail(&changed.join("\n"))
            .hint("Reinstall a package to restore its files: sudo pacman -S <package> (find it with pacman -Qo <file>)");
    }
}

#[cfg(test)]
mod tests {
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
}
