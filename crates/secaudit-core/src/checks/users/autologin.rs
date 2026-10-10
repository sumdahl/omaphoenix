use crate::audit::{Ctx, Report, read_text};

pub(super) fn autologin(ctx: &Ctx, r: &mut Report) {
    // First `User=` / `autologin-user=` line in sddm config, as `grep -rh | head -1`.
    let mut sddm = vec![ctx.sys("/etc/sddm.conf")];
    crate::audit::walk(
        &ctx.sys("/etc/sddm.conf.d"),
        |_, _| true,
        |p, m| {
            if m.is_file() {
                sddm.push(p.to_path_buf());
            }
            true
        },
    );
    let is_user_line = |l: &&str| {
        let t = l.trim_start();
        ["User", "autologin-user"].iter().any(|k| {
            t.strip_prefix(k)
                .is_some_and(|rest| rest.trim_start().starts_with('='))
        })
    };
    let mut found = sddm
        .iter()
        .find_map(|f| read_text(f).lines().find(is_user_line).map(str::to_owned))
        .unwrap_or_default();

    let mut getty = false;
    let systemd = ctx.sys("/etc/systemd/system");
    if let Ok(entries) = std::fs::read_dir(&systemd) {
        for e in entries.flatten() {
            let name = e.file_name().to_string_lossy().into_owned();
            if name.starts_with("getty@") && name.ends_with(".service.d") {
                crate::audit::walk(
                    &e.path(),
                    |_, _| true,
                    |p, m| {
                        getty |= m.is_file() && read_text(p).contains("--autologin");
                        !getty
                    },
                );
            }
        }
    }
    if getty {
        found.push_str(" getty");
    }
    if !found.is_empty() {
        // Bash's `${autologin// /}` removes every space.
        let shown: String = found.chars().filter(|c| *c != ' ').collect();
        r.info(
            "users.autologin",
            format!("Autologin enabled ({shown}) - acceptable only because the LUKS passphrase gates boot"),
        );
    }
}
