//! "Users & authentication": UID 0, passwords, groups, sudoers, faillock, autologin.

use crate::audit::{Ctx, Report, Section, read_text, shell_quote};

mod autologin;
mod sudoers;

use autologin::autologin;
use sudoers::sudoers;

pub(super) const SECTION: Section = Section {
    name: "Users & authentication",
    run,
};

/// `/etc/passwd`-style rows, split on `:`.
fn rows(ctx: &Ctx, abs: &str) -> Vec<Vec<String>> {
    ctx.read_lines(abs)
        .iter()
        .map(|l| l.split(':').map(str::to_owned).collect())
        .collect()
}

fn field(row: &[String], i: usize) -> &str {
    row.get(i).map_or("", String::as_str)
}

fn run(ctx: &Ctx, r: &mut Report) {
    let passwd = rows(ctx, "/etc/passwd");

    let uid0: Vec<&str> = passwd
        .iter()
        .filter(|f| field(f, 2) == "0")
        .map(|f| field(f, 0))
        .collect();
    if uid0 == ["root"] {
        r.ok("users.uid0", "Only root has UID 0");
    } else {
        r.fail(
            "users.uid0",
            format!("Multiple UID 0 accounts: {}", uid0.join(" ")),
        )
        .hint(
            "Any UID 0 account besides root is a backdoor; investigate, then remove with userdel",
        );
    }

    if ctx.is_root {
        shadow(ctx, r);
    } else {
        r.skip(
            "users.passwords",
            "Empty/locked password check (needs sudo)",
        );
    }

    let logins: Vec<&str> = passwd
        .iter()
        .filter(|f| {
            let uid: u32 = field(f, 2).parse().unwrap_or(0);
            let shell = field(f, 6);
            (1000..60000).contains(&uid) && !shell.ends_with("nologin") && !shell.ends_with("false")
        })
        .map(|f| field(f, 0))
        .collect();
    r.info(
        "users.interactive_accounts",
        format!("Interactive human accounts: {}", logins.join(" ")),
    );

    groups(ctx, r, &logins);

    if ctx.is_root {
        sudoers(ctx, r);
    } else {
        r.skip("users.sudoers", "sudoers review (needs sudo)");
    }

    let faillock = ctx.sys("/etc/security/faillock.conf");
    if faillock.is_file() {
        // Bash's `tr -dc 0-9` keeps every digit of every deny line.
        let digits: String = read_text(&faillock)
            .lines()
            .filter(|l| {
                l.trim_start()
                    .strip_prefix("deny")
                    .is_some_and(|rest| rest.trim_start().starts_with('='))
            })
            .flat_map(str::chars)
            .filter(char::is_ascii_digit)
            .collect();
        let shown = if digits.is_empty() {
            "3 (default)".to_owned()
        } else {
            digits
        };
        r.info("users.faillock", format!("faillock deny={shown}"));
    }

    autologin(ctx, r);
}

fn shadow(ctx: &Ctx, r: &mut Report) {
    let shadow = rows(ctx, "/etc/shadow");
    let empty: Vec<&str> = shadow
        .iter()
        .filter(|f| f.len() > 1 && field(f, 1).is_empty())
        .map(|f| field(f, 0))
        .collect();
    if empty.is_empty() {
        r.ok("users.empty_password", "No accounts with empty passwords");
    } else {
        let fix = empty
            .iter()
            .map(|u| format!("sudo passwd -l {};", shell_quote(u)))
            .collect::<Vec<_>>()
            .join(" ");
        r.fail(
            "users.empty_password",
            format!("Accounts with empty passwords: {}", empty.join("\n")),
        )
        .fix(fix);
    }
    let root_pw = shadow
        .iter()
        .find(|f| field(f, 0) == "root")
        .map_or("", |f| field(f, 1));
    if root_pw.starts_with('!') || root_pw.starts_with('*') {
        r.ok("users.root_password", "root password login locked");
    } else {
        r.info(
            "users.root_password",
            "root has a usable password (fine if intentional; sudo makes it unnecessary)",
        );
    }
    let weak: Vec<&str> = shadow
        .iter()
        .filter(|f| {
            ["$1$", "$2a$", "$5$"]
                .iter()
                .any(|p| field(f, 1).starts_with(p))
        })
        .map(|f| field(f, 0))
        .collect();
    if !weak.is_empty() {
        r.warn(
            "users.legacy_hash",
            format!(
                "Accounts with legacy password hashes (md5/sha256/bcrypt-2a): {}",
                weak.join("\n")
            ),
        )
        .hint("Change the password (passwd) to rehash it with yescrypt");
    }
}

fn groups(ctx: &Ctx, r: &mut Report, logins: &[&str]) {
    let group = rows(ctx, "/etc/group");
    for g in ["wheel", "docker", "libvirt", "wireshark", "input"] {
        let members = group
            .iter()
            .find(|f| field(f, 0) == g)
            .map_or("", |f| field(f, 3));
        if members.is_empty() {
            continue;
        }
        match g {
            "docker" => {
                r.warn("users.group_docker", format!("docker group (root-equivalent via the daemon socket): {members}"))
                    .hint(format!(
                        "Accepted risk for a dev box; for stricter isolation use rootless Docker or remove: sudo gpasswd -d {} docker",
                        ctx.user
                    ));
            }
            "libvirt" => {
                r.info(
                    "users.group_libvirt",
                    format!("libvirt group (can manage VMs, near root-equivalent): {members}"),
                );
            }
            "input" => {
                // System users like brltty legitimately need it.
                let human: Vec<&str> = members.split(',').filter(|m| logins.contains(m)).collect();
                if !human.is_empty() {
                    let fix = human
                        .iter()
                        .map(|u| format!("sudo gpasswd -d {} input;", shell_quote(u)))
                        .collect::<Vec<_>>()
                        .join(" ");
                    r.warn(
                        "users.group_input",
                        format!(
                            "input group (can read all keystrokes): {}",
                            human.join("\n")
                        ),
                    )
                    .fix(fix);
                }
            }
            _ => {
                r.info(&format!("users.group_{g}"), format!("{g} group: {members}"));
            }
        }
    }
}

#[cfg(test)]
mod tests;
