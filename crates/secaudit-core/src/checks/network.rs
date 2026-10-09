//! "Network exposure": listening sockets, DNS-over-TLS and LLMNR, open Wi-Fi.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::LazyLock;

use regex::Regex;

use crate::audit::{Ctx, Report, Section, shell_quote};

pub(super) const SECTION: Section = Section {
    name: "Network exposure",
    run,
};

static LOCAL: LazyLock<Option<Regex>> =
    LazyLock::new(|| Regex::new(r"^(127\.|\[::1\]|::1|\[::ffff:127\.|224\.|\[ff0)|%lo:").ok());
/// Docker, libvirt and podman bridges.
static BRIDGE: LazyLock<Option<Regex>> =
    LazyLock::new(|| Regex::new(r"^(172\.(1[6-9]|2[0-9]|3[01])\.|192\.168\.122\.|10\.88\.)").ok());

fn is(re: &LazyLock<Option<Regex>>, s: &str) -> bool {
    re.as_ref().is_some_and(|re| re.is_match(s))
}

/// Process name from `ss -p`'s `users:(("name",…))`, or `?` without root.
fn proc_of(users: &str) -> &str {
    users
        .split_once("users:((\"")
        .and_then(|(_, rest)| rest.split('"').next())
        .filter(|n| !n.is_empty())
        .unwrap_or("?")
}

/// (local address, users column) of each `ss -H` row.
fn sockets(ctx: &Ctx, flags: &str) -> Vec<(String, String)> {
    ctx.run(&["ss", flags])
        .stdout
        .lines()
        .filter_map(|l| {
            let w: Vec<&str> = l.split_whitespace().collect();
            let addr = (*w.get(3)?).to_owned();
            Some((addr, w.get(5..).map(|u| u.join(" ")).unwrap_or_default()))
        })
        .collect()
}

fn run(ctx: &Ctx, r: &mut Report) {
    let mut exposed = BTreeSet::new();
    let mut bridged = BTreeSet::new();
    for (addr, users) in sockets(ctx, "-Htlnp") {
        if is(&LOCAL, &addr) {
            continue;
        }
        let line = format!("{addr} {}", proc_of(&users));
        if is(&BRIDGE, &addr) {
            bridged.insert(line)
        } else {
            exposed.insert(line)
        };
    }
    if exposed.is_empty() {
        r.ok(
            "network.tcp_exposed",
            "No TCP services listening on external interfaces",
        );
    } else {
        r.warn(
            "network.tcp_exposed",
            "TCP services listening on all/external interfaces (verify intended and firewalled):",
        )
        .add_detail(&exposed.into_iter().collect::<Vec<_>>().join("\n"))
        .hint("Bind each service to 127.0.0.1 in its config, or keep ufw denying the port");
    }
    if !bridged.is_empty() {
        r.info(
            "network.tcp_bridge",
            "TCP listeners on container/VM bridges only:",
        )
        .add_detail(&bridged.into_iter().collect::<Vec<_>>().join("\n"));
    }

    // UDP is mostly ephemeral client sockets: list fixed low ports, count the rest per process.
    let mut udp: BTreeMap<String, usize> = BTreeMap::new();
    for (addr, users) in sockets(ctx, "-Hulnp") {
        if is(&LOCAL, &addr) || is(&BRIDGE, &addr) {
            continue;
        }
        let port: u32 = addr
            .rsplit(':')
            .next()
            .and_then(|p| p.parse().ok())
            .unwrap_or(u32::MAX);
        let key = if port < 1024 && port != 68 && port != 546 {
            format!("port {addr} {}", proc_of(&users))
        } else {
            format!("proc {}", proc_of(&users))
        };
        *udp.entry(key).or_default() += 1;
    }
    if !udp.is_empty() {
        let lines: Vec<String> = udp
            .iter()
            .map(|(k, n)| match k.strip_prefix("port ") {
                Some(rest) => rest.to_owned(),
                None => format!("{}: {n} high-port socket(s)", k.trim_start_matches("proc ")),
            })
            .collect();
        r.info(
            "network.udp",
            "External UDP sockets (mDNS 5353 is LAN discovery):",
        )
        .add_detail(&lines.join("\n"));
    }
    if !ctx.is_root {
        r.info(
            "network.needs_root",
            "Run with sudo to see the owning process for every socket ('?' above)",
        );
    }

    dns(ctx, r);
    wifi(ctx, r);
}

fn dns(ctx: &Ctx, r: &mut Report) {
    if !ctx.have("resolvectl") {
        return;
    }
    let status = ctx.run(&["resolvectl", "status"]).stdout;
    let proto = status
        .lines()
        .find(|l| l.contains("Protocols:"))
        .unwrap_or_default();
    if proto.contains("+DNSOverTLS") {
        r.ok("network.dns_over_tls", "DNS-over-TLS enabled");
    } else if !proto.is_empty() {
        r.info(
            "network.dns_over_tls",
            "DNS-over-TLS off (DNS queries visible to the network)",
        );
    }
    if proto.contains("+LLMNR") {
        r.warn("network.llmnr", "LLMNR enabled (name-poisoning risk on untrusted LANs)").fix(
            r"sudo mkdir -p /etc/systemd/resolved.conf.d && printf '[Resolve]\nLLMNR=no\n' | sudo tee /etc/systemd/resolved.conf.d/no-llmnr.conf && sudo systemctl restart systemd-resolved",
        );
    }
}

fn wifi(ctx: &Ctx, r: &mut Report) {
    if !ctx.have("nmcli") {
        return;
    }
    let open: Vec<String> = ctx
        .run(&["nmcli", "-t", "-f", "NAME,TYPE", "connection", "show"])
        .stdout
        .lines()
        .filter_map(|l| {
            let mut f = l.split(':');
            let name = f.next()?;
            f.next()
                .is_some_and(|t| t.contains("wireless"))
                .then(|| name.to_owned())
        })
        .filter(|c| {
            ctx.run(&[
                "nmcli",
                "-g",
                "802-11-wireless-security.key-mgmt",
                "connection",
                "show",
                c,
            ])
            .stdout
            .trim()
            .is_empty()
        })
        .collect();
    if !open.is_empty() {
        let fix: Vec<String> = open
            .iter()
            .map(|c| {
                format!(
                    "nmcli connection modify {} connection.autoconnect no;",
                    shell_quote(c)
                )
            })
            .collect();
        r.warn(
            "network.open_wifi",
            "Saved open (unencrypted) Wi-Fi networks, may auto-join:",
        )
        .add_detail(&open.join("\n"))
        .fix(fix.join(" "));
    }
}

#[cfg(test)]
mod tests {
    use crate::checks::testing::{FakeSystem, msgs};
    use crate::contract::Severity::{Info, Pass, Warn};

    #[test]
    fn patterns_compile() {
        assert!(super::LOCAL.is_some() && super::BRIDGE.is_some());
    }

    #[test]
    fn sockets_dns_and_wifi() {
        let sys = FakeSystem::new();
        sys.bin("resolvectl");
        sys.bin("nmcli");
        sys.cmd(
            "ss -Htlnp",
            true,
            "LISTEN 0 4096 127.0.0.1:631 0.0.0.0:* users:((\"cupsd\",pid=1,fd=7))\nLISTEN 0 128 0.0.0.0:22 0.0.0.0:*\nLISTEN 0 128 172.17.0.1:8080 0.0.0.0:* users:((\"docker-proxy\",pid=2,fd=4))\n",
        );
        sys.cmd(
            "ss -Hulnp",
            true,
            "UNCONN 0 0 0.0.0.0:5353 0.0.0.0:*\nUNCONN 0 0 0.0.0.0:40000 0.0.0.0:* users:((\"brave\",pid=3,fd=1))\nUNCONN 0 0 0.0.0.0:40001 0.0.0.0:* users:((\"brave\",pid=3,fd=2))\nUNCONN 0 0 0.0.0.0:68 0.0.0.0:*\n",
        );
        sys.cmd(
            "resolvectl status",
            true,
            "Global\n  Protocols: +LLMNR +mDNS -DNSOverTLS\n",
        );
        sys.cmd(
            "nmcli -t -f NAME,TYPE connection show",
            true,
            "Cafe:802-11-wireless\nHome:802-11-wireless\nwired:802-3-ethernet\n",
        );
        sys.cmd(
            "nmcli -g 802-11-wireless-security.key-mgmt connection show Home",
            true,
            "wpa-psk\n",
        );
        let ctx = sys.ctx(false);
        let got = msgs(&ctx, super::SECTION);
        assert_eq!(
            got,
            vec![
                (Warn, "TCP services listening on all/external interfaces (verify intended and firewalled):".into()),
                (Info, "TCP listeners on container/VM bridges only:".into()),
                (Info, "External UDP sockets (mDNS 5353 is LAN discovery):".into()),
                (Info, "Run with sudo to see the owning process for every socket ('?' above)".into()),
                (Info, "DNS-over-TLS off (DNS queries visible to the network)".into()),
                (Warn, "LLMNR enabled (name-poisoning risk on untrusted LANs)".into()),
                (Warn, "Saved open (unencrypted) Wi-Fi networks, may auto-join:".into()),
            ]
        );
        let report = crate::audit::run(&ctx, &[super::SECTION]);
        let detail = |id: &str| {
            report
                .findings()
                .iter()
                .find(|f| f.id.as_deref() == Some(id))
                .map(|f| f.detail.clone())
        };
        assert_eq!(
            detail("network.tcp_exposed").as_deref(),
            Some("0.0.0.0:22 ?")
        );
        assert_eq!(
            detail("network.udp").as_deref(),
            Some("?: 2 high-port socket(s)\nbrave: 2 high-port socket(s)")
        );
        assert_eq!(detail("network.open_wifi").as_deref(), Some("Cafe"));
        let _ = Pass;
    }
}
