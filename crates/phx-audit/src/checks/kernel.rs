//! "Kernel hardening (sysctl)": hardening sysctls, taint, LSMs, ld.so.preload.

use std::path::Path;

use crate::audit::{Ctx, Report, Section, read_text};
use crate::contract::Severity;

pub(super) const SECTION: Section = Section {
    name: "Kernel hardening (sysctl)",
    run,
};

/// (key, accepted values, severity when not, note).
const SYSCTLS: &[(&str, &str, Severity, &str)] = &[
    ("kernel.randomize_va_space", "2", Severity::Fail, ""),
    ("kernel.kptr_restrict", "1 2", Severity::Warn, ""),
    ("kernel.dmesg_restrict", "1", Severity::Warn, ""),
    (
        "kernel.yama.ptrace_scope",
        "1 2 3",
        Severity::Warn,
        "(any process can ptrace your other processes)",
    ),
    (
        "kernel.unprivileged_bpf_disabled",
        "1 2",
        Severity::Warn,
        "",
    ),
    ("kernel.kexec_load_disabled", "1", Severity::Warn, ""),
    ("kernel.sysrq", "0 16 176", Severity::Warn, ""),
    ("fs.protected_symlinks", "1", Severity::Fail, ""),
    ("fs.protected_hardlinks", "1", Severity::Fail, ""),
    ("fs.protected_fifos", "1 2", Severity::Warn, ""),
    ("fs.protected_regular", "1 2", Severity::Warn, ""),
    ("fs.suid_dumpable", "0", Severity::Warn, ""),
    ("net.ipv4.tcp_syncookies", "1", Severity::Warn, ""),
    (
        "net.ipv4.conf.all.accept_redirects",
        "0",
        Severity::Warn,
        "",
    ),
    (
        "net.ipv6.conf.all.accept_redirects",
        "0",
        Severity::Warn,
        "",
    ),
    ("net.ipv4.conf.all.send_redirects", "0", Severity::Warn, ""),
    (
        "net.ipv4.conf.all.accept_source_route",
        "0",
        Severity::Warn,
        "",
    ),
    ("net.ipv4.conf.all.rp_filter", "1 2", Severity::Warn, ""),
    ("net.ipv4.conf.all.log_martians", "1", Severity::Warn, ""),
];

fn sysctl(ctx: &Ctx, key: &str) -> Option<String> {
    let out = ctx.run(&["sysctl", "-n", key]);
    out.success.then(|| out.stdout.trim_end().to_owned())
}

fn run(ctx: &Ctx, r: &mut Report) {
    for &(key, accepted, sev, note) in SYSCTLS {
        let id = format!("kernel.sysctl.{key}");
        let Some(v) = sysctl(ctx, key) else {
            r.skip(&id, format!("{key} not available"));
            continue;
        };
        if accepted.split(' ').any(|a| a == v) {
            r.ok(&id, format!("{key} = {v}"));
            continue;
        }
        let want = accepted.rsplit(' ').next().unwrap_or(accepted);
        let note = if note.is_empty() {
            String::new()
        } else {
            format!(" {note}")
        };
        let msg = format!("{key} = {v} (want {}){note}", accepted.replace(' ', " or "));
        let f = if sev == Severity::Fail {
            r.fail(&id, msg)
        } else {
            r.warn(&id, msg)
        };
        f.fix(format!(
            "echo '{key}={want}' | sudo tee -a /etc/sysctl.d/99-zz-hardening.conf && sudo sysctl -w '{key}={want}'"
        ));
    }
    if sysctl(ctx, "net.ipv4.ip_forward").as_deref() == Some("1") {
        r.info(
            "kernel.ip_forward",
            "net.ipv4.ip_forward = 1 (expected with Docker/libvirt; relies on firewall FORWARD rules)",
        );
    }

    let taint: u64 = read_text(&ctx.sys("/proc/sys/kernel/tainted"))
        .trim()
        .parse()
        .unwrap_or(0);
    if taint == 0 {
        r.ok("kernel.taint", "Kernel not tainted");
    } else {
        // Bash appends "flag " for each, keeping the trailing space.
        let flags: String = [
            (1, "proprietary-module "),
            (4096, "out-of-tree-module "),
            (8192, "unsigned-module "),
            (128, "kernel-oops "),
        ]
        .iter()
        .filter(|(bit, _)| taint & bit != 0)
        .map(|(_, name)| *name)
        .collect();
        let shown = if flags.is_empty() {
            "see docs.kernel.org/admin-guide/tainted-kernels".to_owned()
        } else {
            flags
        };
        r.info("kernel.taint", format!("Kernel tainted ({taint}): {shown}"));
    }

    let lsm = read_text(&ctx.sys("/sys/kernel/security/lsm"))
        .trim_end_matches('\n')
        .to_owned();
    if !lsm.is_empty() {
        r.info("kernel.lsm", format!("Active LSMs: {lsm}"));
    }
    if !lsm.contains("apparmor") && !lsm.contains("selinux") {
        r.info("kernel.mac", "No MAC (AppArmor/SELinux) enforcing");
    }

    let preload = ctx.sys("/etc/ld.so.preload");
    let text = read_text(&preload);
    if preload.is_file() && text.lines().any(|l| !l.is_empty()) {
        r.fail(
            "kernel.ld_so_preload",
            "/etc/ld.so.preload is set (common rootkit vector):",
        )
        .file(Path::new("/etc/ld.so.preload"))
        .add_detail(text.trim_end_matches('\n'))
        .hint("Check which package owns each library (pacman -Qo); investigate before removing");
    } else {
        r.ok("kernel.ld_so_preload", "/etc/ld.so.preload empty or absent");
    }
}

#[cfg(test)]
mod tests {
    use crate::checks::testing::{FakeSystem, msgs};
    use crate::contract::Severity::{Fail, Info, Pass, Skip, Warn};

    #[test]
    fn sysctls_taint_and_preload() {
        let sys = FakeSystem::new();
        sys.cmd("sysctl -n kernel.randomize_va_space", true, "1\n");
        sys.cmd("sysctl -n kernel.kptr_restrict", true, "2\n");
        sys.cmd("sysctl -n kernel.yama.ptrace_scope", true, "0\n");
        sys.cmd("sysctl -n net.ipv4.ip_forward", true, "1\n");
        sys.file("/proc/sys/kernel/tainted", "4097\n");
        sys.file(
            "/sys/kernel/security/lsm",
            "capability,landlock,lockdown,yama,bpf\n",
        );
        sys.file("/etc/ld.so.preload", "/usr/lib/evil.so\n");
        let ctx = sys.ctx(false);
        let got = msgs(&ctx, super::SECTION);
        for want in [
            (Fail, "kernel.randomize_va_space = 1 (want 2)"),
            (Pass, "kernel.kptr_restrict = 2"),
            (Skip, "kernel.dmesg_restrict not available"),
            (
                Warn,
                "kernel.yama.ptrace_scope = 0 (want 1 or 2 or 3) (any process can ptrace your other processes)",
            ),
            (
                Info,
                "net.ipv4.ip_forward = 1 (expected with Docker/libvirt; relies on firewall FORWARD rules)",
            ),
            (
                Info,
                "Kernel tainted (4097): proprietary-module out-of-tree-module ",
            ),
            (Info, "Active LSMs: capability,landlock,lockdown,yama,bpf"),
            (Info, "No MAC (AppArmor/SELinux) enforcing"),
            (Fail, "/etc/ld.so.preload is set (common rootkit vector):"),
        ] {
            assert!(
                got.contains(&(want.0, want.1.into())),
                "missing {want:?} in {got:#?}"
            );
        }
        let report = crate::audit::run(&ctx, &[super::SECTION]);
        let ptrace = report
            .findings()
            .iter()
            .find(|f| f.msg.starts_with("kernel.yama"));
        assert_eq!(
            ptrace.map(|f| f.fix.as_str()),
            Some(
                "echo 'kernel.yama.ptrace_scope=3' | sudo tee -a /etc/sysctl.d/99-zz-hardening.conf && sudo sysctl -w 'kernel.yama.ptrace_scope=3'"
            )
        );
    }
}
