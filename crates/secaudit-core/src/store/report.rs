use std::fs;
use std::path::PathBuf;

use super::{Item, Run, sev_name, sev_order};

impl Run {
    /// Writes `report.md`, a checklist of open issues, and returns its path.
    ///
    /// # Errors
    /// If the file can't be written.
    pub fn export_markdown(&self, started: &str) -> std::io::Result<PathBuf> {
        let issues: Vec<&Item> = self
            .items
            .iter()
            .filter(|i| i.is_issue() && !i.accepted)
            .collect();
        let mut sections: Vec<&str> = Vec::new();
        for i in &issues {
            if !sections.contains(&i.finding.section.as_str()) {
                sections.push(&i.finding.section);
            }
        }
        let c = self.counts();
        let mut out = vec![
            format!("# Security audit: {}", self.meta.host),
            String::new(),
            format!(
                "{started} · {} scan · kernel {} · score **{} ({})** · {} fail · {} warn",
                self.meta.mode,
                if self.meta.kernel.is_empty() {
                    "?"
                } else {
                    &self.meta.kernel
                },
                c.score(),
                c.grade(),
                c.fail,
                c.warn
            ),
            String::new(),
        ];
        for sec in sections {
            out.push(format!("## {sec}"));
            out.push(String::new());
            let mut in_sec: Vec<&&Item> =
                issues.iter().filter(|i| i.finding.section == sec).collect();
            in_sec.sort_by_key(|i| sev_order(i.finding.sev));
            for i in in_sec {
                let f = &i.finding;
                out.push(format!(
                    "- [ ] **{}** {}",
                    sev_name(f.sev).to_uppercase(),
                    i.title()
                ));
                if !f.detail.is_empty() {
                    out.push(String::new());
                    out.push("  ```".into());
                    out.extend(f.detail.lines().map(|l| format!("  {l}")));
                    out.push("  ```".into());
                }
                if !f.fix.is_empty() {
                    out.push(String::new());
                    out.push(format!("  Fix: `{}`", f.fix));
                }
                if !f.hint.is_empty() {
                    out.push(String::new());
                    out.push(format!("  {}", f.hint));
                }
                out.push(String::new());
            }
        }
        let path = self.path.join("report.md");
        fs::write(&path, out.join("\n"))?;
        Ok(path)
    }
}
