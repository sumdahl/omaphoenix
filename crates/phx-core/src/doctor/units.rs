use std::fs;

use super::App;
use crate::sys;

impl App<'_> {
    // Enabled user services that run something from `$HOME` snapshots don't carry.
    pub(super) fn check_units(&self, add: &mut dyn FnMut(&str, &str, String)) {
        let dir = self.home.join(".config/systemd/user");
        let home = format!("{}/", self.home.display());
        let mut entries: Vec<_> = fs::read_dir(&dir).into_iter().flatten().flatten().collect();
        entries.sort_by_key(fs::DirEntry::file_name);
        let mut seen = std::collections::BTreeSet::new();
        for e in entries {
            let unit = e.file_name().to_string_lossy().into_owned();
            if !unit.ends_with(".service") {
                continue;
            }
            let text =
                String::from_utf8_lossy(&sys::read_bounded(&e.path(), 1 << 20).unwrap_or_default())
                    .into_owned();
            for line in text.lines() {
                let Some((key, val)) = line.trim().split_once('=') else {
                    continue;
                };
                if !key.starts_with("Exec") {
                    continue;
                }
                for tok in val.split_whitespace() {
                    let tok = tok
                        .trim_start_matches(['-', '@', ':', '+', '!'])
                        .replace("%h", &self.home.to_string_lossy());
                    let Some(rel) = tok.strip_prefix(&home) else {
                        continue;
                    };
                    if !covered_by_snapshot(rel) && seen.insert(format!("{unit}{rel}")) {
                        add(
                            "warn",
                            "services",
                            format!(
                                "{unit} runs ~/{rel}, which snapshots don't include; it won't work after a restore until that is back"
                            ),
                        );
                    }
                    break;
                }
            }
        }
    }
}

fn covered_by_snapshot(rel: &str) -> bool {
    [
        ".config/",
        ".local/bin/",
        ".local/share/applications/",
        ".local/share/fonts/",
    ]
    .iter()
    .any(|p| rel.starts_with(p))
}
