//! `pkg-other`: tools from language package managers, reinstalled one at a time.

use std::collections::BTreeMap;

use phx_contract::Item;
use serde::{Deserialize, Serialize};

use super::{Category, Env, Error, Outcome, read_json, write_json};
use crate::sys;

mod npm;

/// `pkg-other`.
pub struct PkgOther;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
struct OtherPkg {
    manager: String,
    name: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    version: String,
    /// Non-registry origin: recorded, not reinstalled.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    source: String,
}

impl OtherPkg {
    fn id(&self) -> String {
        format!("{}:{}", self.manager, self.name)
    }

    fn new(manager: &str, name: &str, version: &str, source: &str) -> Self {
        Self {
            manager: manager.into(),
            name: name.into(),
            version: version.into(),
            source: source.into(),
        }
    }
}

#[derive(Deserialize)]
struct MiseVersion {
    #[serde(default)]
    version: String,
    /// `mise ls` also lists tools the config asks for but nothing installed yet (F-028).
    #[serde(default)]
    installed: Option<bool>,
}

#[derive(Deserialize, Default)]
struct Crates2 {
    #[serde(default)]
    installs: BTreeMap<String, serde_json::Value>,
}

/// The installed tools in `mise ls --global --json`.
fn mise_tools(json: &str) -> Vec<OtherPkg> {
    let tools: BTreeMap<String, Vec<MiseVersion>> = serde_json::from_str(json).unwrap_or_default();
    tools
        .iter()
        .flat_map(|(name, versions)| {
            versions
                .iter()
                .filter(|v| v.installed != Some(false))
                .map(move |v| OtherPkg::new("mise", name, &v.version, ""))
        })
        .collect()
}

fn detect_other(home: &std::path::Path) -> Vec<OtherPkg> {
    let mut out = Vec::new();
    if sys::has("mise")
        && let Ok(s) = sys::output(None, &["mise", "ls", "--global", "--json"])
    {
        out.extend(mise_tools(&s));
    }
    for (name, version) in npm::globals(home) {
        if name == "npm" || name == "corepack" {
            continue;
        }
        out.push(if version.is_empty() {
            OtherPkg::new("npm", &name, "", "linked local checkout")
        } else {
            OtherPkg::new("npm", &name, &version, "")
        });
    }
    if let Ok(b) = std::fs::read(home.join(".cargo/.crates2.json")) {
        let crates: Crates2 = serde_json::from_slice(&b).unwrap_or_default();
        for key in crates.installs.keys() {
            // "name version (source)"
            let mut f = key.splitn(3, ' ');
            let (Some(name), Some(version), Some(source)) = (f.next(), f.next(), f.next()) else {
                continue;
            };
            let source = if source.contains("crates.io-index") {
                ""
            } else {
                source.trim_matches(['(', ')'])
            };
            out.push(OtherPkg::new("cargo", name, version, source));
        }
    }
    if sys::has("uv") {
        // mise's uv shim fails when no uv version is set; the tools restore installed are still there (F-029).
        let list = sys::lines(None, &["uv", "tool", "list"]).or_else(|e| {
            if sys::has("mise") {
                sys::lines(
                    None,
                    &["mise", "exec", "uv@latest", "--", "uv", "tool", "list"],
                )
            } else {
                Err(e)
            }
        });
        for l in list.unwrap_or_default() {
            let f: Vec<&str> = l.split_whitespace().collect();
            if let (false, Some(name), Some(v)) = (l.starts_with('-'), f.first(), f.get(1)) {
                out.push(OtherPkg::new("uv", name, v.trim_start_matches('v'), ""));
            }
        }
    }
    out.sort_by_key(OtherPkg::id);
    out
}

/// The install command for one tool; `has` says whether a program is on `PATH`.
///
/// On a fresh machine the system npm installs into root-owned `/usr` (F-020) and uv isn't there yet
/// (F-021), so with mise around, npm runs under mise's node and uv comes from mise.
fn install_args(o: &OtherPkg, has: &dyn Fn(&str) -> bool) -> Result<Vec<String>, String> {
    let argv: Vec<String> = match o.manager.as_str() {
        "mise" => {
            return Ok(vec![
                "mise".into(),
                "install".into(),
                "--yes".into(),
                format!("{}@{}", o.name, o.version),
            ]);
        }
        "npm" => vec![
            "npm".into(),
            "install".into(),
            "-g".into(),
            format!("{}@{}", o.name, o.version),
        ],
        "cargo" => vec![
            "cargo".into(),
            "install".into(),
            "--locked".into(),
            o.name.clone(),
            "--version".into(),
            o.version.clone(),
        ],
        "uv" => vec![
            "uv".into(),
            "tool".into(),
            "install".into(),
            format!("{}=={}", o.name, o.version),
        ],
        m => return Err(format!("unknown manager {m:?}")),
    };
    let tool = argv.first().map_or("", String::as_str);
    // Always name the tool: a bare `mise exec` first installs every missing tool in the
    // config, and one mise refuses (F-027) then fails the command.
    let prefix: &[&str] = match (o.manager.as_str(), has("mise"), has(tool)) {
        ("npm", true, _) => &["mise", "exec", "node", "--"],
        // mise's uv shim can be on PATH with no uv version behind it, so `has("uv")` proves nothing.
        ("uv", true, _) => &["mise", "exec", "uv@latest", "--"],
        (_, _, true) => &[],
        (_, true, false) => &["mise", "exec", "rust", "--"],
        (_, false, false) => return Err(format!("{tool} is not installed")),
    };
    Ok(prefix.iter().map(|s| (*s).to_owned()).chain(argv).collect())
}

impl Category for PkgOther {
    fn name(&self) -> &'static str {
        "pkg-other"
    }

    fn detect(&self, env: &Env<'_>) -> Result<Vec<Item>, Error> {
        Ok(detect_other(&env.home)
            .into_iter()
            .map(|o| Item::new(o.id(), o.version))
            .collect())
    }

    fn capture(&self, env: &Env<'_>, items: &[Item]) -> Result<(), Error> {
        let want: std::collections::BTreeSet<&str> = items.iter().map(|i| i.id.as_str()).collect();
        let keep: Vec<OtherPkg> = detect_other(&env.home)
            .into_iter()
            .filter(|o| want.contains(o.id().as_str()))
            .collect();
        if keep.is_empty() {
            return Ok(());
        }
        write_json(&env.tree.join("packages/other.json"), &keep)
    }

    fn restore(&self, env: &Env<'_>) -> Result<Outcome, Error> {
        let mut res = Outcome::default();
        let Some(want): Option<Vec<OtherPkg>> = read_json(&env.tree.join("packages/other.json"))?
        else {
            return Ok(res);
        };
        let have: std::collections::BTreeSet<String> =
            detect_other(&env.home).iter().map(OtherPkg::id).collect();
        let mut todo = Vec::new();
        for o in want {
            if !env.want(&o.id()) && !env.want(&o.name) {
                continue;
            }
            if have.contains(&o.id()) {
                res.present += 1;
            } else if !o.source.is_empty() {
                env.warn(&format!(
                    "reinstall manually: {} {} (from {})",
                    o.manager, o.name, o.source
                ));
            } else {
                todo.push(o);
            }
        }
        if env.dry_run {
            res.planned.extend(
                todo.iter()
                    .map(|o| format!("{} install {} {}", o.manager, o.name, o.version)),
            );
            return Ok(res);
        }
        let mut retry = Vec::new();
        for o in todo {
            match install_args(&o, &sys::has) {
                Ok(argv) => {
                    let argv: Vec<&str> = argv.iter().map(String::as_str).collect();
                    let failed = res.failed.len();
                    if res.install(&o.id(), &argv) {
                        res.changed(
                            "package",
                            &format!("{}@{}", o.name, o.version),
                            &o.manager,
                            false,
                        );
                    } else if o.manager == "mise" && res.failed.len() > failed {
                        retry.push((o, argv.iter().map(|a| (*a).to_owned()).collect::<Vec<_>>()));
                    }
                }
                Err(e) => res.failed.push(format!("{}: {e}", o.id())),
            }
        }
        // A mise tool can need another from the list that sorts after it (elixir needs erlang): once more, last.
        for (o, argv) in retry {
            let prefix = format!("{}: ", o.id());
            res.failed.retain(|f| !f.starts_with(&prefix));
            let argv: Vec<&str> = argv.iter().map(String::as_str).collect();
            if res.install(&o.id(), &argv) {
                res.changed(
                    "package",
                    &format!("{}@{}", o.name, o.version),
                    &o.manager,
                    false,
                );
            }
        }
        Ok(res)
    }
}

#[cfg(test)]
mod tests {
    use super::{OtherPkg, install_args, mise_tools};

    #[test]
    fn a_mise_tool_the_config_names_but_nothing_installed_is_not_present() {
        let json = r#"{"node":[{"version":"25.9.0","installed":true}],
            "gh":[{"version":"2.103.3","installed":false}],
            "go":[{"version":"1.27.3"}]}"#;
        let got: Vec<String> = mise_tools(json).iter().map(OtherPkg::id).collect();
        assert_eq!(
            got,
            ["mise:go", "mise:node"],
            "older mise without the field counts as installed"
        );
    }

    #[test]
    fn install_commands_by_manager() {
        let all = |_: &str| true;
        let a =
            install_args(&OtherPkg::new("mise", "node", "22.1.0", ""), &all).unwrap_or_default();
        assert_eq!(a, ["mise", "install", "--yes", "node@22.1.0"]);
        assert!(install_args(&OtherPkg::new("brew", "x", "1", ""), &all).is_err());
    }

    #[test]
    fn on_a_fresh_machine_npm_and_uv_go_through_mise() {
        // Fresh Omarchy: system npm on PATH, no uv, mise present.
        let fresh = |p: &str| matches!(p, "npm" | "mise" | "cargo");
        let npm =
            install_args(&OtherPkg::new("npm", "cline", "3.0.61", ""), &fresh).unwrap_or_default();
        assert_eq!(
            npm,
            [
                "mise",
                "exec",
                "node",
                "--",
                "npm",
                "install",
                "-g",
                "cline@3.0.61"
            ]
        );
        let uv =
            install_args(&OtherPkg::new("uv", "spotdl", "4.5.2", ""), &fresh).unwrap_or_default();
        assert_eq!(
            uv,
            [
                "mise",
                "exec",
                "uv@latest",
                "--",
                "uv",
                "tool",
                "install",
                "spotdl==4.5.2"
            ]
        );
        let cargo =
            install_args(&OtherPkg::new("cargo", "ncal", "0.1.0", ""), &fresh).unwrap_or_default();
        assert_eq!(
            cargo.first().map(String::as_str),
            Some("cargo"),
            "cargo worked as is"
        );
        let shim = |p: &str| matches!(p, "npm" | "mise" | "uv");
        let uv =
            install_args(&OtherPkg::new("uv", "spotdl", "4.5.2", ""), &shim).unwrap_or_default();
        assert_eq!(
            uv.get(..4).map(<[String]>::to_vec),
            Some(
                ["mise", "exec", "uv@latest", "--"]
                    .map(String::from)
                    .to_vec()
            ),
            "a uv shim on PATH still goes through mise"
        );
        let no_cargo = |p: &str| p == "mise";
        let cargo = install_args(&OtherPkg::new("cargo", "ncal", "0.1.0", ""), &no_cargo)
            .unwrap_or_default();
        assert_eq!(
            cargo.get(..4).map(<[String]>::to_vec),
            Some(["mise", "exec", "rust", "--"].map(String::from).to_vec()),
            "mise is always told which tool, never left to install the whole config"
        );
        let none = |_: &str| false;
        assert!(install_args(&OtherPkg::new("uv", "x", "1", ""), &none).is_err());
    }
}
