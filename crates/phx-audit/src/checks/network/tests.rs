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
