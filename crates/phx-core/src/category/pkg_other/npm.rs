//! Global npm packages, read from the global `node_modules` instead of starting `npm ls` (~125 MB of Node).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::sys;

#[derive(Deserialize, Default)]
struct NpmTree {
    #[serde(default)]
    dependencies: BTreeMap<String, NpmDep>,
}

#[derive(Deserialize, Default)]
struct NpmDep {
    #[serde(default)]
    version: String,
}

/// Global packages as `name → version`; an empty version is a local checkout, as `npm ls` reports it.
///
/// Reads the `npm` on `PATH` and every node mise installed: a restore installs through mise's node,
/// which needn't be the `npm` on `PATH` (F-029).
pub(super) fn globals(home: &Path) -> BTreeMap<String, String> {
    if let Some(root) = configured_root(home) {
        return read_root(&root);
    }
    let roots: Vec<PathBuf> = path_root().into_iter().chain(mise_roots(home)).collect();
    if !roots.is_empty() {
        return roots.iter().flat_map(|r| read_root(r)).collect();
    }
    if !sys::has("npm") {
        return BTreeMap::new();
    }
    let json = sys::output(None, &["npm", "ls", "-g", "--depth=0", "--json"]).unwrap_or_default();
    let tree: NpmTree = serde_json::from_str(&json).unwrap_or_default();
    tree.dependencies
        .into_iter()
        .map(|(n, d)| (n, d.version))
        .collect()
}

/// `<prefix>/lib/node_modules` from a configured prefix.
fn configured_root(home: &Path) -> Option<PathBuf> {
    let configured = std::env::var("NPM_CONFIG_PREFIX")
        .or_else(|_| std::env::var("npm_config_prefix"))
        .ok()
        .filter(|p| !p.is_empty())
        .or_else(|| {
            std::fs::read_to_string(home.join(".npmrc"))
                .ok()?
                .lines()
                .find_map(|l| {
                    l.trim()
                        .strip_prefix("prefix")
                        .map(|v| v.trim_start().trim_start_matches('=').trim().to_owned())
                })
        });
    configured.map(|prefix| {
        prefix
            .strip_prefix("~/")
            .map_or_else(|| PathBuf::from(&prefix), |rest| home.join(rest))
            .join("lib/node_modules")
    })
}

/// The global root of the `npm` on `PATH`.
fn path_root() -> Option<PathBuf> {
    // A real npm resolves to <root>/npm/bin/npm-cli.js; a shim (mise, volta) doesn't, so `npm ls` decides.
    let cli = std::fs::canonicalize(sys::which("npm")?).ok()?;
    cli.ends_with("npm/bin/npm-cli.js")
        .then(|| cli.ancestors().nth(3).map(Path::to_path_buf))
        .flatten()
}

/// The global root of each node mise installed (version aliases are symlinks and skipped).
fn mise_roots(home: &Path) -> Vec<PathBuf> {
    let data = std::env::var_os("MISE_DATA_DIR")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("XDG_DATA_HOME").map(|d| PathBuf::from(d).join("mise")))
        .unwrap_or_else(|| home.join(".local/share/mise"));
    node_roots(&data.join("installs/node"))
}

fn node_roots(installs: &Path) -> Vec<PathBuf> {
    let mut roots: Vec<PathBuf> = std::fs::read_dir(installs)
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| e.file_type().is_ok_and(|t| t.is_dir()))
        .map(|e| e.path().join("lib/node_modules"))
        .filter(|r| r.is_dir())
        .collect();
    roots.sort();
    roots
}

fn read_root(root: &Path) -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    for e in std::fs::read_dir(root).into_iter().flatten().flatten() {
        let name = e.file_name().to_string_lossy().into_owned();
        if name.starts_with('.') {
            continue;
        }
        if name.starts_with('@') {
            for s in std::fs::read_dir(e.path()).into_iter().flatten().flatten() {
                let full = format!("{name}/{}", s.file_name().to_string_lossy());
                out.insert(full, version(&s.path()));
            }
        } else {
            out.insert(name, version(&e.path()));
        }
    }
    out
}

fn version(dir: &Path) -> String {
    #[derive(Deserialize)]
    struct Pkg {
        #[serde(default)]
        version: String,
    }
    std::fs::read(dir.join("package.json"))
        .ok()
        .and_then(|b| serde_json::from_slice::<Pkg>(&b).ok())
        .map(|p| p.version)
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    #[test]
    fn reads_plain_scoped_and_versionless_packages_like_npm_ls() {
        let t = tempfile::tempdir().unwrap_or_else(|e| unreachable!("{e}"));
        let pkg = |rel: &str, json: &str| {
            let d = t.path().join(rel);
            std::fs::create_dir_all(&d).unwrap_or_else(|e| unreachable!("{e}"));
            std::fs::write(d.join("package.json"), json).unwrap_or_else(|e| unreachable!("{e}"));
        };
        pkg("cline", r#"{"name":"cline","version":"3.0.61"}"#);
        pkg("@qwen-code/qwen-code", r#"{"version":"0.19.6"}"#);
        pkg("codex-monorepo", r#"{"name":"codex-monorepo"}"#);
        pkg(".hidden", r#"{"version":"1"}"#);
        let got: Vec<(String, String)> = super::read_root(t.path()).into_iter().collect();
        let want = [
            ("@qwen-code/qwen-code", "0.19.6"),
            ("cline", "3.0.61"),
            ("codex-monorepo", ""),
        ];
        assert_eq!(got, want.map(|(a, b)| (a.to_owned(), b.to_owned())));
    }

    #[test]
    fn finds_packages_under_mises_node_not_only_the_npm_on_path() {
        let t = tempfile::tempdir().unwrap_or_else(|e| unreachable!("{e}"));
        let node = t.path().join(".local/share/mise/installs/node");
        let pkg = node.join("25.9.0/lib/node_modules/cline");
        std::fs::create_dir_all(&pkg).unwrap_or_else(|e| unreachable!("{e}"));
        std::fs::write(pkg.join("package.json"), r#"{"version":"3.0.61"}"#)
            .unwrap_or_else(|e| unreachable!("{e}"));
        std::os::unix::fs::symlink("./25.9.0", node.join("latest"))
            .unwrap_or_else(|e| unreachable!("{e}"));
        assert_eq!(
            super::node_roots(&node),
            [node.join("25.9.0/lib/node_modules")],
            "one root per install, aliases skipped"
        );
        assert_eq!(
            super::read_root(&node.join("25.9.0/lib/node_modules"))
                .get("cline")
                .map(String::as_str),
            Some("3.0.61")
        );
    }
}
