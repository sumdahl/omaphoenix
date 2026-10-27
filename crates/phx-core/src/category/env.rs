use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

/// Where and how a category works.
pub struct Env<'a> {
    /// The user's home.
    pub home: PathBuf,
    /// Snapshot tree: written by capture, read by restore.
    pub tree: PathBuf,
    /// Plan only.
    pub dry_run: bool,
    /// `(level, message)`; level is `info`, `warn` or `step`.
    pub log: &'a dyn Fn(&str, &str),
    /// Asked only when encrypted secrets are restored.
    pub passphrase: Option<&'a dyn Fn() -> Result<zeroize::Zeroizing<String>, String>>,
    /// Restore only these item globs; empty means all.
    pub items: Vec<String>,
    /// The category being restored (for machine-specific patterns).
    pub category: String,
    /// Restoring onto different hardware than the snapshot's.
    pub foreign: bool,
    /// Hardware-specific patterns per category, skipped when foreign.
    pub machine: BTreeMap<String, Vec<String>>,
    /// Items skipped because they are hardware-specific.
    pub skipped: RefCell<Vec<String>>,
    /// Files the audit gate leaves out of a capture (absolute paths).
    pub excluded: BTreeSet<PathBuf>,
    /// Services (`scope/unit`) the audit gate leaves out of a capture.
    pub excluded_units: BTreeSet<String>,
}

impl<'a> Env<'a> {
    /// An environment for `home` and `tree`.
    pub fn new(home: &Path, tree: &Path, log: &'a dyn Fn(&str, &str)) -> Self {
        Self {
            home: home.to_path_buf(),
            tree: tree.to_path_buf(),
            dry_run: false,
            log,
            passphrase: None,
            items: Vec::new(),
            category: String::new(),
            foreign: false,
            machine: BTreeMap::new(),
            skipped: RefCell::new(Vec::new()),
            excluded: BTreeSet::new(),
            excluded_units: BTreeSet::new(),
        }
    }

    /// Whether the audit gate leaves `path` out, logging it when it does.
    pub fn audit_excludes(&self, path: &Path) -> bool {
        let out = self.excluded.contains(path);
        if out {
            self.info(&format!("left out by the audit: {}", path.display()));
        }
        out
    }

    /// Logs at `info`.
    pub fn info(&self, msg: &str) {
        (self.log)("info", msg);
    }

    /// Logs at `warn`.
    pub fn warn(&self, msg: &str) {
        (self.log)("warn", msg);
    }

    /// Whether `id` (or a parent dir) is in scope; hardware-specific items skip unless named.
    pub fn want(&self, id: &str) -> bool {
        let explicit = !self.items.is_empty() && matches_id(&self.items, id);
        if !self.items.is_empty() && !explicit {
            return false;
        }
        // A folder doesn't choose the hardware-specific files in it; only naming the file does (F-024).
        let named = self
            .items
            .iter()
            .any(|p| p == id || phx_contract::glob::path_match(p, id) == Some(true));
        if self.foreign
            && !named
            && self
                .machine
                .get(&self.category)
                .is_some_and(|p| matches_id(p, id))
        {
            let mut skipped = self.skipped.borrow_mut();
            if !skipped.iter().any(|s| s == id) {
                skipped.push(id.to_owned());
            }
            return false;
        }
        true
    }
}

/// Whether a pattern matches `id` or one of its parent directories.
#[must_use]
pub fn matches_id(patterns: &[impl AsRef<str>], id: &str) -> bool {
    patterns.iter().any(|p| {
        let mut cur = id;
        loop {
            if cur.is_empty() || cur == "." || cur == "/" {
                return false;
            }
            if phx_contract::glob::path_match(p.as_ref(), cur) == Some(true) {
                return true;
            }
            cur = match cur.rfind('/') {
                Some(0) => "/",
                Some(i) => cur.get(..i).unwrap_or_default(),
                None => ".",
            };
        }
    })
}

#[cfg(test)]
mod tests {
    use super::{Env, matches_id};
    use std::collections::BTreeMap;

    #[test]
    fn matches_parents_like_go() {
        assert!(matches_id(&[".config/hypr"], ".config/hypr/bindings.lua"));
        assert!(matches_id(
            &[".config/hypr/monitors.*"],
            ".config/hypr/monitors.lua"
        ));
        assert!(!matches_id(&[".config/hypr"], ".config/hyprlock.conf"));
        assert!(matches_id(&["/etc/x"], "/etc/x/y"));
        assert!(!matches_id(&["*"], ""));
    }

    #[test]
    fn want_scopes_and_skips_hardware_items() {
        let log = |_: &str, _: &str| {};
        let mut env = Env::new("/h".as_ref(), "/t".as_ref(), &log);
        assert!(env.want("anything"));
        env.foreign = true;
        env.category = "config".into();
        env.machine = BTreeMap::from([(
            "config".to_owned(),
            vec![".config/hypr/monitors.*".to_owned()],
        )]);
        assert!(!env.want(".config/hypr/monitors.lua"));
        assert!(env.want(".config/hypr/bindings.lua"));
        assert_eq!(
            env.skipped.borrow().as_slice(),
            [".config/hypr/monitors.lua"]
        );
        env.items = vec![".config/hypr/monitors.lua".into()];
        assert!(
            env.want(".config/hypr/monitors.lua"),
            "named explicitly, so restored anyway"
        );
        assert!(!env.want(".config/hypr/bindings.lua"), "not named");
        env.items = vec![".config/hypr".into()];
        assert!(
            !env.want(".config/hypr/monitors.lua"),
            "naming the folder doesn't choose the hardware file in it"
        );
        assert!(
            env.want(".config/hypr/bindings.lua"),
            "the rest of the folder is restored"
        );
    }
}
