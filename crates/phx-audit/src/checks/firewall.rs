//! "Firewall": ufw (or an nftables ruleset), and ufw-docker.

use std::sync::LazyLock;

use regex::Regex;

use crate::audit::{Ctx, Report, Section};
use crate::contract::Target;

pub(super) const SECTION: Section = Section {
    name: "Firewall",
    run,
};

static ALLOW_ANYWHERE: LazyLock<Option<Regex>> =
    LazyLock::new(|| Regex::new(r"ALLOW IN\s+Anywhere").ok());

fn run(ctx: &Ctx, r: &mut Report) {
    if ctx.have("ufw") {
        ufw(ctx, r);
    } else {
        let nft = ctx.have("nft")
            && ctx.is_root
            && !ctx.run(&["nft", "list", "ruleset"]).stdout.is_empty();
        if nft {
            r.ok("firewall.active", "nftables ruleset present");
        } else {
            r.fail("firewall.active", "No firewall detected")
                .target(Target::None)
                .fix("sudo pacman -S --needed ufw && sudo systemctl enable --now ufw && sudo ufw enable");
        }
    }

    if ctx.have("ufw-docker") && ctx.is_root {
        if ctx
            .run(&["iptables", "-S", "DOCKER-USER"])
            .stdout
            .contains("ufw-user-forward")
        {
            r.ok(
                "firewall.ufw_docker",
                "ufw-docker integration present (published container ports filtered by ufw)",
            );
        } else {
            r.info(
                "firewall.ufw_docker",
                "ufw-docker rules not loaded (fine if Docker is stopped)",
            )
            .fix("sudo ufw-docker install && sudo systemctl restart ufw");
        }
    }
}

fn ufw(ctx: &Ctx, r: &mut Report) {
    if !ctx.run(&["systemctl", "is-active", "-q", "ufw"]).success {
        r.fail("firewall.active", "ufw installed but not active")
            .target(Target::None)
            .fix("sudo systemctl enable --now ufw && sudo ufw enable");
        return;
    }
    if !ctx.is_root {
        r.ok("firewall.active", "ufw service active");
        r.skip("firewall.rules", "ufw rule review (needs sudo)");
        return;
    }
    let status = ctx.run(&["ufw", "status", "verbose"]).stdout;
    if !status.contains("Status: active") {
        r.fail(
            "firewall.active",
            "ufw service running but firewall inactive",
        )
        .target(Target::None)
        .fix("sudo ufw enable");
        return;
    }
    r.ok("firewall.active", "ufw active");
    if status.contains("Default: deny (incoming)") {
        r.ok("firewall.default_deny", "ufw default deny incoming");
    } else {
        r.fail(
            "firewall.default_deny",
            "ufw default incoming policy is not deny",
        )
        .target(Target::None)
        .fix("sudo ufw default deny incoming");
    }
    // sed -n '/^--/,$p' | tail -n +2 | grep -v '^$'
    let rules: Vec<&str> = status
        .lines()
        .skip_while(|l| !l.starts_with("--"))
        .skip(1)
        .filter(|l| !l.is_empty())
        .collect();
    if !rules.is_empty() {
        r.info("firewall.rules", "ufw allow rules:")
            .add_detail(&rules.join("\n"));
    }
    if ALLOW_ANYWHERE
        .as_ref()
        .is_some_and(|re| rules.iter().any(|l| re.is_match(l)))
    {
        r.warn(
            "firewall.allow_anywhere",
            "Some ufw rules allow from Anywhere (check they are intended)",
        )
        .hint(
            "Scope rules to your LAN, e.g. sudo ufw allow from 192.168.1.0/24 to any port <port>",
        );
    }
}

#[cfg(test)]
mod tests {
    use crate::checks::testing::{FakeSystem, msgs};
    use crate::contract::Severity::{Fail, Info, Pass, Skip, Warn};

    #[test]
    fn pattern_compiles() {
        assert!(super::ALLOW_ANYWHERE.is_some());
    }

    #[test]
    fn no_firewall() {
        let sys = FakeSystem::new();
        assert_eq!(
            msgs(&sys.ctx(false), super::SECTION),
            vec![(Fail, "No firewall detected".into())]
        );
    }

    #[test]
    fn ufw_as_user_and_root() {
        let sys = FakeSystem::new();
        sys.bin("ufw");
        sys.cmd("systemctl is-active -q ufw", true, "");
        assert_eq!(
            msgs(&sys.ctx(false), super::SECTION),
            vec![
                (Pass, "ufw service active".into()),
                (Skip, "ufw rule review (needs sudo)".into())
            ]
        );
        sys.cmd(
            "ufw status verbose",
            true,
            "Status: active\nDefault: deny (incoming), allow (outgoing)\n\nTo                         Action      From\n--                         ------      ----\n22/tcp                     ALLOW IN    Anywhere\n\n",
        );
        assert_eq!(
            msgs(&sys.ctx(true), super::SECTION),
            vec![
                (Pass, "ufw active".into()),
                (Pass, "ufw default deny incoming".into()),
                (Info, "ufw allow rules:".into()),
                (
                    Warn,
                    "Some ufw rules allow from Anywhere (check they are intended)".into()
                ),
            ]
        );
    }
}
