//! Run diff, Markdown export and history.

use std::fmt::Write as _;
use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};

use phx_audit::store::{Run, sev_name};

use super::Ui;
use super::format::{
    B, CLEAR, GRN, N, RED, YEL, counts, grade_color, grade_of, icon, issue_keys, lines, lpad,
    print, rpad, score_of, sev_color,
};
use crate::tui::times::local_time;

impl Ui {
    pub(super) fn show_diff(&self, prev: &Path, cur: &Path) {
        let (Some(p), Some(c)) = (Self::load(prev), Self::load(cur)) else {
            return;
        };
        let (a, b) = (issue_keys(&p), issue_keys(&c));
        let m = &self.m;
        print(CLEAR);
        self.title_bar(
            "Security Audit",
            &format!(
                "vs scan from {} ({})",
                local_time(p.meta.started, "%-d %b %H:%M"),
                p.meta.mode
            ),
        );
        let mut o = format!("\n   {B}New issues{N}\n");
        let new: Vec<&String> = b.difference(&a).collect();
        if new.is_empty() {
            let _ = writeln!(o, "     {m}none{N}");
        }
        for k in new {
            let mut parts = k.splitn(3, '\t');
            let (sev, sec, msg) = (
                parts.next().unwrap_or(""),
                parts.next().unwrap_or(""),
                parts.next().unwrap_or(""),
            );
            let _ = writeln!(
                o,
                "     {}{}{N} {m}{sec}:{N} {}",
                sev_color(sev, m),
                icon(sev),
                msg.strip_suffix(':').unwrap_or(msg)
            );
        }
        let _ = write!(o, "\n   {B}Resolved{N}\n");
        let resolved: Vec<&String> = a.difference(&b).collect();
        if resolved.is_empty() {
            let _ = writeln!(o, "     {m}none{N}");
        }
        for k in resolved {
            let mut parts = k.splitn(3, '\t');
            let (_, sec, msg) = (
                parts.next(),
                parts.next().unwrap_or(""),
                parts.next().unwrap_or(""),
            );
            let _ = writeln!(
                o,
                "     {GRN}✓{N} {m}{sec}:{N} {}",
                msg.strip_suffix(':').unwrap_or(msg)
            );
        }
        if p.meta.mode != c.meta.mode {
            let _ = writeln!(
                o,
                "\n   {m}The scans used different modes, so some differences come from coverage, not changes.{N}"
            );
        }
        print(&o);
        self.pause();
    }

    pub(super) fn export_md(&self, run: &Run) {
        let out = run.path.join("report.md");
        let [f, wn, ..] = counts(run);
        let s = score_of(f, wn);
        let mut md = format!("# Security audit: {}\n\n", run.meta.host);
        let _ = write!(
            md,
            "{} · {} scan · kernel {} · score **{s} ({})** · {f} fail · {wn} warn\n\n",
            local_time(run.meta.started, "%Y-%m-%d %H:%M"),
            run.meta.mode,
            run.meta.kernel,
            grade_of(s)
        );
        md.push_str(&checklist(run));
        let _ = std::fs::write(&out, md);
        let path = out.display().to_string();
        let _ = Command::new("wl-copy")
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .ok()
            .and_then(|mut c| {
                c.stdin.take()?.write_all(path.as_bytes()).ok()?;
                c.wait().ok()
            });
        let home = std::env::var("HOME").unwrap_or_default();
        let shown = if !home.is_empty() && path.starts_with(&home) {
            path.replacen(&home, "~", 1)
        } else {
            path
        };
        print(&format!(
            "\n   {GRN}✓ Saved{N} {shown}\n   {}(path copied to clipboard){N}\n",
            self.m
        ));
        self.pause();
    }

    pub(super) fn history_view(&self) {
        loop {
            print(CLEAR);
            self.title_bar("Security Audit", "History");
            print("\n");
            let m = &self.m;
            let mut items: Vec<(String, String)> = self
                .runs_newest_first()
                .iter()
                .filter_map(|d| {
                    let run = Self::load(d)?;
                    let [f, w, ..] = counts(&run);
                    let s = score_of(f, w);
                    let g = grade_of(s);
                    let l = format!(
                        "{}  {m}{}{N}  {}{} {g}{N}   {RED}{} fail{N}  {YEL}{} warn{N}",
                        local_time(run.meta.started, "%a %-d %b %H:%M"),
                        lpad(&run.meta.mode, 5),
                        grade_color(g),
                        rpad(&s.to_string(), 3),
                        rpad(&f.to_string(), 2),
                        rpad(&w.to_string(), 2)
                    );
                    Some((l, d.display().to_string()))
                })
                .collect();
            items.push((format!("{m}← Back{N}"), "back".into()));
            let Some(choice) = self.choose(
                "Past scans · newest first",
                lines().saturating_sub(8),
                &items,
            ) else {
                return;
            };
            if choice == "back" {
                return;
            }
            self.dashboard(Path::new(&choice));
        }
    }
}

/// The issue checklist, as the original's jq program writes it.
pub(super) fn checklist(run: &Run) -> String {
    let issues: Vec<_> = run
        .items
        .iter()
        .filter(|i| matches!(sev_name(i.finding.sev), "fail" | "warn"))
        .collect();
    let mut sections: Vec<&str> = Vec::new();
    for i in &issues {
        if !sections.contains(&i.finding.section.as_str()) {
            sections.push(&i.finding.section);
        }
    }
    let mut out = String::new();
    for sec in sections {
        let mut items: Vec<_> = issues.iter().filter(|i| i.finding.section == sec).collect();
        items.sort_by_key(|i| sev_name(i.finding.sev) != "fail");
        let body: Vec<String> = items
            .iter()
            .map(|i| {
                let fd = &i.finding;
                let mut s = format!(
                    "- [ ] **{}** {}",
                    sev_name(fd.sev).to_uppercase(),
                    fd.msg.strip_suffix(':').unwrap_or(&fd.msg)
                );
                if !fd.detail.is_empty() {
                    let d: Vec<String> = fd.detail.split('\n').map(|l| format!("  {l}")).collect();
                    let _ = write!(s, "\n\n  ```\n{}\n  ```", d.join("\n"));
                }
                if !fd.fix.is_empty() {
                    let _ = write!(s, "\n\n  Fix: `{}`", fd.fix);
                }
                if !fd.hint.is_empty() {
                    let _ = write!(s, "\n\n  {}", fd.hint);
                }
                s
            })
            .collect();
        let _ = writeln!(out, "## {sec}\n\n{}\n", body.join("\n\n"));
    }
    out
}
