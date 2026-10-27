//! "Filesystem permissions": sensitive modes, world-writable paths, stray SUID/SGID, PATH.

use std::fs;
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::path::{Path, PathBuf};

use crate::audit::{Ctx, Report, Section, shell_quote, walk};

pub(super) const SECTION: Section = Section {
    name: "Filesystem permissions",
    run,
};

/// Most paths listed per world-writable finding, as Bash's `head -20`.
const MAX_LISTED: usize = 20;

fn mode_of(path: &Path) -> Option<u32> {
    fs::symlink_metadata(path)
        .ok()
        .map(|m| m.permissions().mode() & 0o7777)
}

fn run(ctx: &Ctx, r: &mut Report) {
    for abs in ["/etc/shadow", "/etc/gshadow"] {
        let path = ctx.sys(abs);
        let Ok(meta) = fs::symlink_metadata(&path) else {
            continue;
        };
        let mode = meta.permissions().mode() & 0o7777;
        let shown = format!("{abs} {mode:o} {}", ctx.id_name("/etc/passwd", meta.uid()));
        // Bash: `^[0-6]00$|^0$`, i.e. owner may read/write, nobody else anything.
        if mode == 0 || (mode & 0o7077 == 0 && mode & 0o100 == 0) {
            r.ok("filesystem.shadow_perms", shown).file(Path::new(abs));
        } else {
            r.fail(
                "filesystem.shadow_perms",
                format!("{shown} (should not be group/world readable)"),
            )
            .file(Path::new(abs))
            .fix(format!("sudo chmod 600 {abs}"));
        }
    }

    // A missing sudoers prints an empty mode, as `stat` failing does in Bash.
    let sudoers = mode_of(&ctx.sys("/etc/sudoers"))
        .map(|m| format!("{m:o}"))
        .unwrap_or_default();
    if sudoers == "440" || sudoers == "400" {
        r.ok(
            "filesystem.sudoers_perms",
            format!("/etc/sudoers {sudoers}"),
        );
    } else {
        r.fail(
            "filesystem.sudoers_perms",
            format!("/etc/sudoers {sudoers}"),
        )
        .file(Path::new("/etc/sudoers"))
        .fix("sudo chmod 440 /etc/sudoers");
    }

    let home = mode_of(&ctx.home).unwrap_or(0);
    // Bash: `^7[0-5]0$`: owner rwx, group at most r-x, others nothing.
    if home & 0o7707 == 0o700 && home & 0o020 == 0 {
        r.ok("filesystem.home_perms", format!("Home dir {home:o}"));
    } else {
        r.warn(
            "filesystem.home_perms",
            format!("Home dir {home:o} (others can read it)"),
        )
        .file(&ctx.home)
        .fix(format!(
            "chmod 700 {}",
            shell_quote(&ctx.home.display().to_string())
        ));
    }

    world_writable(ctx, r);
    suid(ctx, r);
    path_dirs(ctx, r);
}

fn listed(ctx: &Ctx, paths: &[PathBuf]) -> (String, String) {
    let shown: Vec<String> = paths.iter().map(|p| ctx.unsys(p)).collect();
    let quoted: Vec<String> = shown.iter().map(|p| shell_quote(p)).collect();
    (shown.join("\n"), quoted.join(" "))
}

fn world_writable(ctx: &Ctx, r: &mut Report) {
    let mut files = Vec::new();
    for abs in ["/etc", "/usr", "/boot", "/opt"] {
        walk(
            &ctx.sys(abs),
            |_, _| true,
            |p, m| {
                if m.is_file() && m.mode() & 0o002 != 0 {
                    files.push(p.to_path_buf());
                }
                files.len() < MAX_LISTED
            },
        );
        if files.len() >= MAX_LISTED {
            break;
        }
    }
    if files.is_empty() {
        r.ok(
            "filesystem.world_writable_files",
            "No world-writable files in /etc /usr /boot /opt",
        );
    } else {
        let (detail, quoted) = listed(ctx, &files);
        let f = r.fail(
            "filesystem.world_writable_files",
            "World-writable system files:",
        );
        f.detail(detail).fix(format!("sudo chmod o-w {quoted}"));
        for p in &files {
            f.file(Path::new(&ctx.unsys(p)));
        }
    }

    let skip: Vec<PathBuf> = ["/proc", "/sys", "/dev"]
        .iter()
        .map(|d| ctx.sys(d))
        .collect();
    let mut dirs = Vec::new();
    walk(
        &ctx.root,
        |d, _| !skip.iter().any(|s| s == d),
        |p, m| {
            let mode = m.mode();
            if m.is_dir()
                && mode & 0o002 != 0
                && mode & 0o1000 == 0
                && !skip.iter().any(|s| p.starts_with(s) && p != s)
            {
                dirs.push(p.to_path_buf());
            }
            dirs.len() < MAX_LISTED
        },
    );
    if dirs.is_empty() {
        r.ok(
            "filesystem.world_writable_dirs",
            "All world-writable dirs have the sticky bit",
        );
    } else {
        let (detail, quoted) = listed(ctx, &dirs);
        let f = r.warn(
            "filesystem.world_writable_dirs",
            "World-writable dirs without sticky bit:",
        );
        f.detail(detail).fix(format!("sudo chmod +t {quoted}"));
        for p in &dirs {
            f.file(Path::new(&ctx.unsys(p)));
        }
    }
}

fn suid(ctx: &Ctx, r: &mut Report) {
    let roots: &[&str] = if ctx.deep {
        &["/"]
    } else {
        &["/usr", "/opt", "/etc", "/home", "/var", "/srv"]
    };
    r.info(
        "filesystem.suid_scan",
        format!(
            "Scanning {} for SUID/SGID binaries not owned by any package...",
            roots.join(" ")
        ),
    );
    let mut rogue = Vec::new();
    for abs in roots {
        walk(
            &ctx.sys(abs),
            |_, _| true,
            |p, m| {
                if m.is_file() && m.mode() & 0o6000 != 0 {
                    let shown = ctx.unsys(p);
                    if !ctx.run(&["pacman", "-Qoq", &shown]).success {
                        rogue.push(shown);
                    }
                }
                true
            },
        );
    }
    if rogue.is_empty() {
        r.ok(
            "filesystem.suid_unowned",
            "All SUID/SGID binaries belong to packages",
        );
    } else {
        let f = r.fail(
            "filesystem.suid_unowned",
            "SUID/SGID binaries not owned by any package:",
        );
        f.detail(rogue.join("\n")).hint(
            "Find where each came from; if not needed, drop the bit with sudo chmod u-s,g-s <file>",
        );
        for p in &rogue {
            f.file(Path::new(p));
        }
    }
}

fn path_dirs(ctx: &Ctx, r: &mut Report) {
    // Bash splits PATH on `:` and then on spaces (`${PATH//:/ }`, unquoted).
    for d in ctx.path_env.split(':').flat_map(str::split_whitespace) {
        let dir = ctx.sys(d);
        let Ok(meta) = fs::metadata(&dir) else {
            continue;
        };
        if !meta.is_dir() {
            continue;
        }
        let mode = meta.permissions().mode() & 0o7777;
        let group_writable = mode & 0o020 != 0 && ctx.id_name("/etc/group", meta.gid()) != ctx.user;
        if mode & 0o002 != 0 || group_writable {
            let real = fs::canonicalize(&dir).map_or_else(|_| d.to_owned(), |p| ctx.unsys(&p));
            r.fail(
                "filesystem.path_writable",
                format!("PATH dir {d} is group/world-writable ({mode:o})"),
            )
            .file(Path::new(&real))
            .fix(format!("chmod go-w {}", shell_quote(&real)));
        }
    }
    let wrapped = format!(":{}:", ctx.path_env);
    if wrapped.contains("::") || wrapped.contains(":.:") {
        r.fail("filesystem.path_cwd", "PATH contains the current directory")
            .hint("Remove '.' or empty entries from PATH in your shell rc");
    }
}

#[cfg(test)]
mod tests;
