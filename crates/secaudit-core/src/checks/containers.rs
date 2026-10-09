//! "Containers & VMs": Docker daemon exposure and risky containers, libvirt.

use std::sync::LazyLock;

use regex::Regex;

use crate::audit::{Ctx, Report, Section, read_text};

pub(super) const SECTION: Section = Section {
    name: "Containers & VMs",
    run,
};

static DANGEROUS_CAPS: LazyLock<Option<Regex>> = LazyLock::new(|| {
    Regex::new(r#""CapAdd": \[[^\]]*"(SYS_ADMIN|ALL|NET_ADMIN|SYS_PTRACE)""#).ok()
});

fn run(ctx: &Ctx, r: &mut Report) {
    if ctx.have("docker") {
        docker(ctx, r);
    }
    if ctx
        .run(&["systemctl", "is-active", "-q", "libvirtd"])
        .success
    {
        r.info("containers.libvirtd", "libvirtd running");
    }
}

fn docker(ctx: &Ctx, r: &mut Report) {
    if !ctx.run(&["systemctl", "is-active", "-q", "docker"]).success {
        r.ok("containers.docker", "Docker daemon not running");
        return;
    }
    r.info("containers.docker", "Docker daemon running");

    let mut confs = vec![ctx.sys("/etc/docker/daemon.json")];
    let dropins = ctx.sys("/etc/systemd/system/docker.service.d");
    let mut extra: Vec<_> = std::fs::read_dir(&dropins)
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|e| e == "conf"))
        .collect();
    extra.sort();
    confs.extend(extra);
    if confs.iter().any(|f| read_text(f).contains("tcp://")) {
        r.fail("containers.docker_tcp", "Docker daemon exposed over TCP")
            .hint("Remove tcp:// hosts from /etc/docker/daemon.json; use SSH contexts for remote access");
    }

    if !ctx.run(&["docker", "info"]).success {
        r.skip(
            "containers.inspect",
            "Container inspection (no access to docker socket)",
        );
        return;
    }
    for line in ctx
        .run(&["docker", "ps", "--format", "{{.ID}} {{.Names}}"])
        .stdout
        .lines()
    {
        let mut w = line.split_whitespace();
        let Some(id) = w.next() else { continue };
        let name = w.collect::<Vec<_>>().join(" ");
        let inspect = ctx.run(&["docker", "inspect", id]).stdout;
        if inspect.contains("\"Privileged\": true") {
            r.fail(
                "containers.privileged",
                format!("Container {name} is privileged"),
            )
            .hint("Recreate without --privileged; add only the devices/capabilities it needs");
        }
        if inspect.contains("\"NetworkMode\": \"host\"") {
            r.warn(
                "containers.host_network",
                format!("Container {name} uses host networking (bypasses ufw-docker)"),
            )
            .hint("Use a bridge network and publish only needed ports on 127.0.0.1");
        }
        if inspect.contains("/var/run/docker.sock") || inspect.contains("/run/docker.sock") {
            r.fail(
                "containers.docker_socket",
                format!("Container {name} mounts the Docker socket"),
            )
            .hint(
                "Socket access is root on the host; use a socket proxy with a read-only allowlist",
            );
        }
        if DANGEROUS_CAPS
            .as_ref()
            .is_some_and(|re| re.is_match(&inspect))
        {
            r.warn(
                "containers.capabilities",
                format!("Container {name} has dangerous capabilities"),
            )
            .hint("Drop the cap_add entries the container does not strictly need");
        }
        let ports = ctx.run(&["docker", "port", id]).stdout;
        let published = ports
            .lines()
            .filter(|l| !l.contains("127.0.0.1") && !l.contains("[::1]"))
            .collect::<Vec<_>>()
            .join("\n");
        if !published.is_empty() {
            r.warn(
                "containers.published_ports",
                format!("Container {name} publishes ports on all interfaces:"),
            )
            .add_detail(&published)
            .hint("Publish as 127.0.0.1:<host>:<container> unless the LAN needs it");
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::checks::testing::{FakeSystem, msgs};
    use crate::contract::Severity::{Fail, Info, Pass, Skip, Warn};

    #[test]
    fn pattern_compiles() {
        assert!(super::DANGEROUS_CAPS.is_some());
    }

    #[test]
    fn docker_states() {
        let sys = FakeSystem::new();
        assert!(
            msgs(&sys.ctx(false), super::SECTION).is_empty(),
            "no docker, no libvirt"
        );
        sys.bin("docker");
        assert_eq!(
            msgs(&sys.ctx(false), super::SECTION),
            vec![(Pass, "Docker daemon not running".into())]
        );
        sys.cmd("systemctl is-active -q docker", true, "");
        assert_eq!(
            msgs(&sys.ctx(false), super::SECTION),
            vec![
                (Info, "Docker daemon running".into()),
                (
                    Skip,
                    "Container inspection (no access to docker socket)".into()
                ),
            ]
        );
    }

    #[test]
    fn risky_containers() {
        let sys = FakeSystem::new();
        sys.bin("docker");
        sys.cmd("systemctl is-active -q docker", true, "");
        sys.cmd("systemctl is-active -q libvirtd", true, "");
        sys.file(
            "/etc/docker/daemon.json",
            r#"{"hosts":["tcp://0.0.0.0:2375"]}"#,
        );
        sys.cmd("docker info", true, "");
        sys.cmd("docker ps --format {{.ID}} {{.Names}}", true, "abc web\n");
        sys.cmd(
            "docker inspect abc",
            true,
            r#"[{"HostConfig":{"Privileged": true,"NetworkMode": "host","CapAdd": ["NET_ADMIN"],"Binds":["/var/run/docker.sock:/var/run/docker.sock"]}}]"#,
        );
        sys.cmd(
            "docker port abc",
            true,
            "80/tcp -> 0.0.0.0:8080\n80/tcp -> 127.0.0.1:8081\n",
        );
        assert_eq!(
            msgs(&sys.ctx(false), super::SECTION),
            vec![
                (Info, "Docker daemon running".into()),
                (Fail, "Docker daemon exposed over TCP".into()),
                (Fail, "Container web is privileged".into()),
                (
                    Warn,
                    "Container web uses host networking (bypasses ufw-docker)".into()
                ),
                (Fail, "Container web mounts the Docker socket".into()),
                (Warn, "Container web has dangerous capabilities".into()),
                (
                    Warn,
                    "Container web publishes ports on all interfaces:".into()
                ),
                (Info, "libvirtd running".into()),
            ]
        );
    }
}
