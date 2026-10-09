//! "Rootkit & malware indicators": modules hidden from lsmod, PIDs hidden
//! from ps, processes running deleted binaries, executables in temp dirs.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::os::unix::fs::PermissionsExt;

use crate::audit::{Ctx, Report, Section, walk};

pub(super) const SECTION: Section = Section {
    name: "Rootkit & malware indicators",
    run,
};

/// Most temp-dir executables listed, as Bash's `head -10`.
const MAX_TMP: usize = 10;

fn names_in(ctx: &Ctx, abs: &str) -> Vec<String> {
    fs::read_dir(ctx.sys(abs))
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect()
}

fn run(ctx: &Ctx, r: &mut Report) {
    hidden_modules(ctx, r);
    hidden_pids(ctx, r);
    deleted_binaries(ctx, r);
    temp_executables(ctx, r);

    let rkhunter = ctx.have("rkhunter");
    if rkhunter && ctx.is_root {
        r.info(
            "rootkit.rkhunter",
            "rkhunter installed: run 'sudo rkhunter --check --sk' for a deeper scan",
        );
    }
    if !rkhunter {
        r.info(
            "rootkit.scanners",
            "Optional deeper scanners not installed: rkhunter, lynis",
        )
        .fix("sudo pacman -S --needed rkhunter lynis");
    }
}

fn hidden_modules(ctx: &Ctx, r: &mut Report) {
    let listed: BTreeSet<String> = ctx
        .run(&["lsmod"])
        .stdout
        .lines()
        .skip(1)
        .filter_map(|l| l.split_whitespace().next().map(str::to_owned))
        .collect();
    let mut hidden: Vec<String> = names_in(ctx, "/sys/module")
        .into_iter()
        .filter(|m| !listed.contains(m) && ctx.sys(&format!("/sys/module/{m}/initstate")).is_file())
        .collect();
    hidden.sort();
    if hidden.is_empty() {
        r.ok(
            "rootkit.hidden_modules",
            "No loaded modules hidden from lsmod",
        );
    } else {
        r.fail(
            "rootkit.hidden_modules",
            "Modules in /sys/module but hidden from lsmod:",
        )
        .add_detail(&hidden.join("\n"))
        .hint("Possible kernel rootkit: boot a live USB and scan the disk offline");
    }
}

fn hidden_pids(ctx: &Ctx, r: &mut Report) {
    let ps: BTreeSet<String> = ctx
        .run(&["ps", "-e", "-o", "pid="])
        .stdout
        .lines()
        .map(|l| l.trim().to_owned())
        .collect();
    // Listed after ps, as Bash does; PIDs born in between show up and vanish on a re-run.
    let mut hidden: Vec<String> = names_in(ctx, "/proc")
        .into_iter()
        .filter(|n| !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()) && !ps.contains(n))
        .filter(|p| ctx.sys(&format!("/proc/{p}")).is_dir())
        .collect();
    hidden.sort();
    if hidden.is_empty() {
        r.ok("rootkit.hidden_pids", "No processes hidden from ps");
    } else {
        r.warn(
            "rootkit.hidden_pids",
            format!(
                "PIDs in /proc not shown by ps (may be short-lived): {}",
                hidden.join(" ")
            ),
        )
        .hint("Re-run; if the same PIDs persist, inspect /proc/<pid>/cmdline and exe");
    }
}

fn deleted_binaries(ctx: &Ctx, r: &mut Report) {
    let mut counts: BTreeMap<String, usize> = BTreeMap::new();
    for pid in names_in(ctx, "/proc")
        .iter()
        .filter(|n| n.bytes().all(|b| b.is_ascii_digit()))
    {
        let Ok(target) = fs::read_link(ctx.sys(&format!("/proc/{pid}/exe"))) else {
            continue;
        };
        let target = target.to_string_lossy();
        if let Some(path) = target
            .strip_suffix(" (deleted)")
            .filter(|p| !p.starts_with("/memfd:"))
        {
            *counts.entry(path.to_owned()).or_default() += 1;
        }
    }
    // `sort | uniq -c | sort -rn`: most processes first, ties by the whole line reversed.
    let mut rows: Vec<(usize, String)> = counts.into_iter().map(|(p, n)| (n, p)).collect();
    rows.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| b.1.cmp(&a.1)));
    let line = |(n, p): &(usize, String)| format!("{p} ({n} proc)");
    let system = |p: &str| p.starts_with("/usr/") || p.starts_with("/opt/");
    let odd: Vec<String> = rows.iter().filter(|(_, p)| !system(p)).map(line).collect();
    let sys: Vec<String> = rows.iter().filter(|(_, p)| system(p)).map(line).collect();
    if !odd.is_empty() {
        r.warn(
            "rootkit.deleted_binaries",
            "Processes running from deleted binaries outside /usr,/opt (malware often self-deletes):",
        )
        .add_detail(&odd.join("\n"))
        .hint("Find the PIDs with: ls -l /proc/*/exe 2>/dev/null | grep deleted");
    }
    if !sys.is_empty() {
        r.info(
            "rootkit.stale_binaries",
            "Processes still running pre-upgrade binaries (restart them):",
        )
        .add_detail(&sys.join("\n"));
    }
}

fn temp_executables(ctx: &Ctx, r: &mut Report) {
    let mut found = Vec::new();
    for abs in ["/tmp", "/var/tmp", "/dev/shm"] {
        walk(
            &ctx.sys(abs),
            |_, _| true,
            |p, m| {
                if m.is_file() && m.permissions().mode() & 0o100 != 0 {
                    found.push(ctx.unsys(p));
                }
                found.len() < MAX_TMP
            },
        );
        if found.len() >= MAX_TMP {
            break;
        }
    }
    if found.is_empty() {
        r.ok(
            "rootkit.temp_executables",
            "No executables in /tmp, /var/tmp, /dev/shm",
        );
    } else {
        r.warn("rootkit.temp_executables", "Executables in temp dirs:")
            .add_detail(&found.join("\n"))
            .hint("Check what created them; delete if unknown");
    }
}

#[cfg(test)]
mod tests {
    use std::os::unix::fs::symlink;

    use crate::checks::testing::{FakeSystem, msgs};
    use crate::contract::Severity::{Fail, Info, Pass, Warn};

    #[test]
    fn clean_machine() {
        let sys = FakeSystem::new();
        sys.bin("rkhunter");
        sys.file("/sys/module/ext4/initstate", "live");
        sys.file("/sys/module/kernel/parameters/x", "");
        sys.cmd("lsmod", true, "Module Size Used by\next4 1 0\n");
        sys.dir_mode("/proc/1", 0o555);
        sys.cmd("ps -e -o pid=", true, "    1\n");
        assert_eq!(
            msgs(&sys.ctx(false), super::SECTION),
            vec![
                (Pass, "No loaded modules hidden from lsmod".into()),
                (Pass, "No processes hidden from ps".into()),
                (Pass, "No executables in /tmp, /var/tmp, /dev/shm".into()),
            ]
        );
    }

    #[test]
    fn indicators() {
        let sys = FakeSystem::new();
        sys.file("/sys/module/diamorphine/initstate", "live");
        sys.cmd("lsmod", true, "Module Size Used by\n");
        sys.dir_mode("/proc/1", 0o755);
        sys.dir_mode("/proc/666", 0o755);
        sys.dir_mode("/proc/7", 0o755);
        sys.dir_mode("/proc/8", 0o755);
        sys.cmd("ps -e -o pid=", true, "1\n7\n8\n");
        let ctx = sys.ctx(false);
        symlink("/tmp/.x (deleted)", ctx.sys("/proc/666/exe")).ok();
        symlink("/usr/bin/foo (deleted)", ctx.sys("/proc/7/exe")).ok();
        symlink("/usr/bin/foo (deleted)", ctx.sys("/proc/8/exe")).ok();
        sys.file_mode("/dev/shm/payload", "", 0o755);
        let got = msgs(&ctx, super::SECTION);
        assert_eq!(
            got,
            vec![
                (Fail, "Modules in /sys/module but hidden from lsmod:".into()),
                (Warn, "PIDs in /proc not shown by ps (may be short-lived): 666".into()),
                (Warn, "Processes running from deleted binaries outside /usr,/opt (malware often self-deletes):".into()),
                (Info, "Processes still running pre-upgrade binaries (restart them):".into()),
                (Warn, "Executables in temp dirs:".into()),
                (Info, "Optional deeper scanners not installed: rkhunter, lynis".into()),
            ]
        );
        let report = crate::audit::run(&ctx, &[super::SECTION]);
        let stale = report
            .findings()
            .iter()
            .find(|f| f.id.as_deref() == Some("rootkit.stale_binaries"));
        assert_eq!(
            stale.map(|f| f.detail.as_str()),
            Some("/usr/bin/foo (2 proc)")
        );
    }
}
