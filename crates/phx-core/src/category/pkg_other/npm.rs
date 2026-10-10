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
pub(super) fn globals(home: &Path) -> BTreeMap<String, String> {
    if let Some(root) = global_root(home) {
        return read_root(&root);
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

/// `<prefix>/lib/node_modules`: from a configured prefix, else from where `npm` itself lives.
fn global_root(home: &Path) -> Option<PathBuf> {
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
    if let Some(prefix) = configured {
        let prefix = prefix
            .strip_prefix("~/")
            .map_or_else(|| PathBuf::from(&prefix), |rest| home.join(rest));
        return Some(prefix.join("lib/node_modules"));
    }
    // A real npm resolves to <root>/npm/bin/npm-cli.js; a shim (mise, volta) doesn't, so `npm ls` decides.
    let cli = std::fs::canonicalize(sys::which("npm")?).ok()?;
    cli.ends_with("npm/bin/npm-cli.js")
        .then(|| cli.ancestors().nth(3).map(Path::to_path_buf))
        .flatten()
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
}
