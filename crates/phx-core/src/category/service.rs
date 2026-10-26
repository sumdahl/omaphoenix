//! `service`: enabled systemd units, enabled (not started) on restore.

use phx_contract::Item;

use super::{Category, Env, Error, Outcome};
use crate::sys;

/// `service`.
pub struct Service;

fn scoped<'a>(scope: &str, args: &[&'a str]) -> Vec<&'a str> {
    let mut v = vec!["systemctl"];
    if scope == "user" {
        v.push("--user");
    }
    v.extend_from_slice(args);
    v
}

impl Category for Service {
    fn name(&self) -> &'static str {
        "service"
    }

    fn detect(&self, _: &Env<'_>) -> Result<Vec<Item>, Error> {
        if !sys::has("systemctl") {
            return Ok(Vec::new());
        }
        let mut items = Vec::new();
        for scope in ["system", "user"] {
            let argv = scoped(
                scope,
                &[
                    "list-unit-files",
                    "--state=enabled",
                    "--no-legend",
                    "--type=service,socket,timer,path",
                ],
            );
            // No user manager from a TTY or over SSH: skip that scope.
            let Ok(lines) = sys::lines(None, &argv) else {
                continue;
            };
            for l in lines {
                let Some(unit) = l.split_whitespace().next() else {
                    continue;
                };
                if unit.contains("@.") {
                    continue; // templates need an instance name
                }
                items.push(Item::new(format!("{scope}/{unit}"), ""));
            }
        }
        Ok(items)
    }

    fn capture(&self, env: &Env<'_>, items: &[Item]) -> Result<(), Error> {
        if items.is_empty() {
            return Ok(());
        }
        let ids: Vec<&str> = items
            .iter()
            .map(|i| i.id.as_str())
            .filter(|id| {
                let out = env.excluded_units.contains(*id);
                if out {
                    env.info(&format!("left out by the audit: service {id}"));
                }
                !out
            })
            .collect();
        std::fs::write(
            env.tree.join("services.txt"),
            format!("{}\n", ids.join("\n")),
        )?;
        Ok(())
    }

    fn restore(&self, env: &Env<'_>) -> Result<Outcome, Error> {
        let mut res = Outcome::default();
        let text = match std::fs::read_to_string(env.tree.join("services.txt")) {
            Ok(t) => t,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(res),
            Err(e) => return Err(e.into()),
        };
        let mut todo: [(&str, Vec<&str>); 2] = [("system", Vec::new()), ("user", Vec::new())];
        for id in text.split_whitespace() {
            let (scope, unit) = id.split_once('/').unwrap_or((id, ""));
            // Hardware rules match the scoped id; the bare unit name only counts when the user named it (F-019).
            if !(env.want(id) || (!env.items.is_empty() && env.want(unit))) {
                continue;
            }
            let state =
                sys::output(None, &scoped(scope, &["is-enabled", unit])).unwrap_or_default();
            if matches!(state.as_str(), "enabled" | "static" | "alias") {
                res.present += 1;
            } else if let Some((_, units)) = todo.iter_mut().find(|(s, _)| *s == scope) {
                units.push(unit);
            }
        }
        for (scope, units) in todo {
            if units.is_empty() {
                continue;
            }
            if env.dry_run {
                res.planned
                    .extend(units.iter().map(|u| format!("enable {scope} {u}")));
                continue;
            }
            let run = |args: &[&str]| -> bool {
                if scope == "user" {
                    sys::output(None, &scoped("user", args)).is_ok()
                } else {
                    let mut argv = vec!["sudo", "systemctl"];
                    argv.extend_from_slice(args);
                    sys::interactive(&argv).is_ok()
                }
            };
            run(&["daemon-reload"]);
            for u in units {
                if run(&["enable", u]) {
                    res.changed("service", &format!("{scope}/{u}"), "", scope != "user");
                    res.applied += 1;
                } else {
                    res.failed.push(format!("{scope} {u} (unit file missing? its package or /etc file may not be restored)"));
                }
            }
        }
        Ok(res)
    }
}

#[cfg(test)]
mod tests {
    use super::Service;
    use crate::category::{Category, Env};

    #[test]
    fn a_unit_skipped_as_other_hardware_is_not_enabled() {
        let t = tempfile::tempdir().unwrap_or_else(|e| unreachable!("{e}"));
        std::fs::write(
            t.path().join("services.txt"),
            "system/phx-hw-only.service\nsystem/phx-test-unit.service\n",
        )
        .unwrap_or_else(|e| unreachable!("{e}"));
        let log = |_: &str, _: &str| {};
        let mut env = Env::new(t.path(), t.path(), &log);
        env.dry_run = true;
        env.foreign = true;
        env.category = "service".into();
        env.machine = std::collections::BTreeMap::from([(
            "service".to_owned(),
            vec!["system/phx-hw-only.service".to_owned()],
        )]);
        let res = Service.restore(&env).unwrap_or_default();
        assert!(
            !res.planned.iter().any(|p| p.contains("phx-hw-only")),
            "{:?}",
            res.planned
        );
        assert!(res.failed.is_empty(), "{:?}", res.failed);
        assert_eq!(
            env.skipped.borrow().as_slice(),
            ["system/phx-hw-only.service"]
        );
    }
}
