use std::collections::BTreeSet;
use std::path::PathBuf;

use crate::audit::{Ctx, Report, read_text};
use crate::contract::Target;

/// `/etc/sudoers` and every file under `/etc/sudoers.d`, as `grep -r` reads them.
fn sudoers_files(ctx: &Ctx) -> Vec<PathBuf> {
    let mut files = vec![ctx.sys("/etc/sudoers")];
    crate::audit::walk(
        &ctx.sys("/etc/sudoers.d"),
        |_, _| true,
        |p, m| {
            if m.is_file() {
                files.push(p.to_path_buf());
            }
            true
        },
    );
    files
}

pub(super) fn sudoers(ctx: &Ctx, r: &mut Report) {
    let mut nopasswd = Vec::new();
    let mut nopasswd_files = BTreeSet::new();
    let mut no_auth = false;
    for file in sudoers_files(ctx) {
        for line in read_text(&file).lines() {
            if line.trim_start().starts_with('#') {
                continue;
            }
            if line.contains("NOPASSWD") {
                nopasswd.push(line.to_owned());
                nopasswd_files.insert(ctx.unsys(&file));
            }
            let t = line.trim_start();
            if t.starts_with("Defaults") && t.contains("!authenticate") {
                no_auth = true;
            }
        }
    }
    if nopasswd.is_empty() {
        r.ok("users.sudo_nopasswd", "No NOPASSWD sudo rules");
    } else {
        let f = r.warn("users.sudo_nopasswd", "NOPASSWD sudo rules:");
        f.detail(nopasswd.join("\n"))
            .hint("Limit NOPASSWD to specific commands, or remove it with sudo visudo");
        for path in nopasswd_files {
            f.target(Target::File { path });
        }
    }
    if no_auth {
        r.fail(
            "users.sudo_no_authenticate",
            "sudo authentication disabled (!authenticate)",
        )
        .hint("Remove the !authenticate Defaults line with sudo visudo");
    }
    if ctx.run(&["visudo", "-c"]).success {
        r.ok("users.sudoers_syntax", "sudoers syntax valid");
    } else {
        r.fail("users.sudoers_syntax", "sudoers has syntax errors")
            .fix("sudo visudo -c");
    }
}
