//! "SSH": sshd exposure and hardening, `~/.ssh` and private key hygiene.

use std::fs;
use std::io::Read;
use std::os::unix::fs::PermissionsExt;

use crate::audit::{Ctx, Report, Section, read_text, shell_quote};
use crate::contract::Target;

pub(super) const SECTION: Section = Section { name: "SSH", run };

/// (sshd -T key, accepted values).
const SSHD: &[(&str, &str)] = &[
    ("permitrootlogin", "no prohibit-password"),
    ("passwordauthentication", "no"),
    ("kbdinteractiveauthentication", "no"),
    ("permitemptypasswords", "no"),
    ("x11forwarding", "no"),
];

fn run(ctx: &Ctx, r: &mut Report) {
    let enabled = ctx.run(&["systemctl", "is-enabled", "-q", "sshd"]).success
        || ctx.run(&["systemctl", "is-active", "-q", "sshd"]).success;
    if enabled {
        r.warn("ssh.sshd_enabled", "sshd is enabled/active")
            .target(Target::Unit {
                name: "system/sshd.service".into(),
            })
            .fix("sudo systemctl disable --now sshd");
        if ctx.have("sshd") && ctx.is_root {
            let cfg = ctx.run(&["sshd", "-T"]).stdout;
            for (key, accepted) in SSHD {
                let v = cfg
                    .lines()
                    .find_map(|l| {
                        l.split_once(' ')
                            .filter(|(k, _)| k == key)
                            .map(|(_, v)| v.to_owned())
                    })
                    .unwrap_or_default();
                let id = format!("ssh.sshd_{key}");
                if accepted.split(' ').any(|a| a == v) {
                    r.ok(&id, format!("sshd {key} {v}"));
                } else {
                    let first = accepted.split(' ').next().unwrap_or(accepted);
                    r.fail(&id, format!("sshd {key} {v} (want {accepted})")).hint(format!(
                        "Add '{key} {first}' to /etc/ssh/sshd_config.d/10-hardening.conf, then sudo systemctl reload sshd"
                    ));
                }
            }
        } else {
            r.skip("ssh.sshd_config", "sshd effective config (needs sudo)");
        }
    } else {
        r.ok("ssh.sshd_enabled", "sshd not enabled");
    }

    keys(ctx, r);
}

fn keys(ctx: &Ctx, r: &mut Report) {
    let dir = ctx.home.join(".ssh");
    if !dir.is_dir() {
        return;
    }
    let mode = fs::symlink_metadata(&dir).map_or(0, |m| m.permissions().mode() & 0o7777);
    if mode == 0o700 {
        r.ok("ssh.dir_perms", "~/.ssh perms 700");
    } else {
        r.fail("ssh.dir_perms", format!("~/.ssh perms {mode:o} (want 700)"))
            .file(&dir)
            .fix("chmod 700 ~/.ssh");
    }

    let mut entries: Vec<_> = fs::read_dir(&dir)
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            !p.file_name()
                .is_some_and(|n| n.to_string_lossy().starts_with('.'))
        })
        .collect();
    entries.sort();
    for key in entries.iter().filter(|p| p.is_file()) {
        let mut head = [0u8; 40];
        let n = fs::File::open(key)
            .and_then(|mut f| f.read(&mut head))
            .unwrap_or(0);
        if !String::from_utf8_lossy(head.get(..n).unwrap_or_default()).contains("PRIVATE KEY") {
            continue;
        }
        let shown = ctx.tilde(key);
        let quoted = shell_quote(&key.display().to_string());
        let mode = fs::symlink_metadata(key).map_or(0, |m| m.permissions().mode() & 0o7777);
        if mode != 0o600 && mode != 0o400 {
            r.fail(
                "ssh.key_perms",
                format!("{shown} perms {mode:o} (want 600)"),
            )
            .file(key)
            .fix(format!("chmod 600 {quoted}"));
        }
        let path = key.display().to_string();
        if ctx.have("ssh-keygen")
            && ctx
                .run(&["ssh-keygen", "-y", "-P", "", "-f", &path])
                .success
        {
            r.warn(
                "ssh.key_no_passphrase",
                format!("{shown} has no passphrase"),
            )
            .file(key)
            .fix(format!("ssh-keygen -p -f {quoted}"));
        }
        let body = read_text(key);
        if body.contains("BEGIN RSA PRIVATE KEY") || body.contains("BEGIN DSA") {
            r.warn(
                "ssh.key_legacy_format",
                format!("{shown} uses legacy RSA/DSA PEM format"),
            )
            .file(key)
            .fix(format!("ssh-keygen -p -o -f {quoted}"));
        }
    }

    let authorized = dir.join("authorized_keys");
    if authorized.is_file() {
        let n = read_text(&authorized)
            .lines()
            .filter(|l| {
                let t = l.trim_start();
                !t.is_empty() && !t.starts_with('#')
            })
            .count();
        r.info(
            "ssh.authorized_keys",
            format!("authorized_keys has {n} key(s)"),
        )
        .file(&authorized);
    }
}

#[cfg(test)]
mod tests {
    use crate::checks::testing::{FakeSystem, msgs};
    use crate::contract::Severity::{Fail, Info, Pass, Warn};

    #[test]
    fn sshd_off_and_clean_keys() {
        let sys = FakeSystem::new();
        sys.home_dir(".ssh", 0o700);
        sys.home_file_mode(
            ".ssh/id_ed25519",
            "-----BEGIN OPENSSH PRIVATE KEY-----\n",
            0o600,
        );
        sys.home_file(".ssh/id_ed25519.pub", "ssh-ed25519 AAAA");
        sys.home_file(".ssh/authorized_keys", "# comment\n\nssh-ed25519 AAAA a\n");
        assert_eq!(
            msgs(&sys.ctx(false), super::SECTION),
            vec![
                (Pass, "sshd not enabled".into()),
                (Pass, "~/.ssh perms 700".into()),
                (Info, "authorized_keys has 1 key(s)".into()),
            ]
        );
    }

    #[test]
    fn exposed_sshd_and_weak_keys() {
        let sys = FakeSystem::new();
        sys.bin("sshd");
        sys.bin("ssh-keygen");
        sys.cmd("systemctl is-active -q sshd", true, "");
        sys.cmd(
            "sshd -T",
            true,
            "permitrootlogin yes\npasswordauthentication no\nx11forwarding no\n",
        );
        sys.home_dir(".ssh", 0o755);
        sys.home_file_mode(".ssh/id_rsa", "-----BEGIN RSA PRIVATE KEY-----\n", 0o644);
        let ctx = sys.ctx(true);
        let key = ctx.home.join(".ssh/id_rsa").display().to_string();
        sys.cmd(
            &format!("ssh-keygen -y -P  -f {key}"),
            true,
            "ssh-rsa AAAA\n",
        );
        let got = msgs(&ctx, super::SECTION);
        for want in [
            (Warn, "sshd is enabled/active"),
            (Fail, "sshd permitrootlogin yes (want no prohibit-password)"),
            (Pass, "sshd passwordauthentication no"),
            (Fail, "sshd kbdinteractiveauthentication  (want no)"),
            (Fail, "~/.ssh perms 755 (want 700)"),
            (Fail, "~/.ssh/id_rsa perms 644 (want 600)"),
            (Warn, "~/.ssh/id_rsa has no passphrase"),
            (Warn, "~/.ssh/id_rsa uses legacy RSA/DSA PEM format"),
        ] {
            assert!(
                got.contains(&(want.0, want.1.into())),
                "missing {want:?} in {got:#?}"
            );
        }
    }
}
