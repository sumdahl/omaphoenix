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
mod tests;
