//! The results dashboard.

use std::fmt::Write as _;
use std::path::Path;
use std::process::Command;

use phx_audit::store::{Run, sev_name};

use super::Ui;
use super::format::{
    B, CLEAR, GRN, N, RED, YEL, counts, grade_color, grade_of, issue_keys, lpad, mmss, print, rpad,
    score_of, trunc, width,
};
use crate::tui::times::local_time;

impl Ui {
    pub(super) fn render_dashboard(&self, dir: &Path, run: &Run) {
        let w = width();
        let [f, wn, p, inf, sk] = counts(run);
        let s = score_of(f, wn);
        let g = grade_of(s);
        let gc = grade_color(g);
        let m = &self.m;
        print(CLEAR);
        let mode = &run.meta.mode;
        self.title_bar(
            "Security Audit",
            &format!(
                "{} · {mode} scan",
                local_time(run.meta.started, "%a %-d %b %H:%M")
            ),
        );
        let bw = w.saturating_sub(22);
        let filled = bw * s / 100;
        let mut o = format!("\n   {B}{gc}{}{N} {B}{gc}{g}{N}  ", rpad(&s.to_string(), 3));
        let _ = write!(o, "{gc}{}{N}", "━".repeat(filled.max(1)));
        let _ = writeln!(
            o,
            "{m}{}{N}   {m}/100{N}",
            "─".repeat(bw.saturating_sub(filled).max(1))
        );
        let _ = writeln!(
            o,
            "   {RED}✗ {f} fail{N}   {YEL}! {wn} warn{N}   {GRN}✓ {p} pass{N}   {m}i {inf} notes   – {sk} skipped{N}"
        );
        let mut extra = String::new();
        if let Some(fin) = run.meta.finished {
            extra = format!("took {}", mmss(fin.saturating_sub(run.meta.started)));
        }
        if let Some(prev) = self.previous_run(dir).and_then(|p| Self::load(&p)) {
            let (a, b) = (issue_keys(&prev), issue_keys(run));
            let (new, resolved) = (b.difference(&a).count(), a.difference(&b).count());
            if !extra.is_empty() {
                extra.push_str(" · ");
            }
            extra.push_str("vs previous: ");
            if new > 0 {
                let _ = write!(extra, "{RED}{new} new{m}");
            } else {
                extra.push_str("0 new");
            }
            extra.push_str(", ");
            if resolved > 0 {
                let _ = write!(extra, "{GRN}{resolved} resolved{m}");
            } else {
                extra.push_str("0 resolved");
            }
        }
        if mode == "quick" {
            if !extra.is_empty() {
                extra.push_str(" · ");
            }
            extra.push_str("some checks need a full scan");
        }
        let _ = write!(o, "   {m}{extra}{N}\n\n");
        let _ = writeln!(
            o,
            "   {m}{} {} {} {}{N}",
            lpad("SECTION", 36),
            rpad("PASS", 5),
            rpad("WARN", 5),
            rpad("FAIL", 5)
        );
        for sec in self.engine_sections() {
            let of = |sev: &str| {
                run.items
                    .iter()
                    .filter(|i| i.finding.section == sec && sev_name(i.finding.sev) == sev)
                    .count()
            };
            if !run.items.iter().any(|i| i.finding.section == sec) {
                continue;
            }
            let (sp, sw, sf) = (of("pass"), of("warn"), of("fail"));
            let ic = if sf > 0 {
                format!("{RED}✗{N}")
            } else if sw > 0 {
                format!("{YEL}!{N}")
            } else {
                format!("{GRN}✓{N}")
            };
            let _ = writeln!(
                o,
                "   {ic} {} {} {}{}{N} {}{}{N}",
                lpad(&trunc(&sec, 34), 34),
                rpad(&sp.to_string(), 5),
                if sw > 0 { YEL } else { m },
                rpad(&sw.to_string(), 5),
                if sf > 0 { RED } else { m },
                rpad(&sf.to_string(), 5)
            );
        }
        o.push('\n');
        print(&o);
    }

    pub(super) fn dashboard(&self, dir: &Path) {
        loop {
            let Some(run) = Self::load(dir) else { return };
            self.render_dashboard(dir, &run);
            let [f, wn, p, inf, _] = counts(&run);
            let prev = self.previous_run(dir);
            let mut items: Vec<(String, String)> = Vec::new();
            let it = |l: String, v: &str| (l, v.to_owned());
            if f + wn > 0 {
                items.push(it(format!("Fix issues  ({})", f + wn), "issues"));
            }
            if inf > 0 {
                items.push(it(format!("Notes  ({inf})"), "notes"));
            }
            items.push(it(format!("Passed checks  ({p})"), "passed"));
            if prev.is_some() {
                items.push(it("Changes since previous scan".into(), "diff"));
            }
            for (l, v) in [
                ("Full output", "log"),
                ("Export Markdown report", "export"),
                ("Scan again", "rescan"),
                ("Back", "back"),
            ] {
                items.push(it(l.into(), v));
            }
            let Some(choice) = self.choose("", 9, &items) else {
                return;
            };
            match choice.as_str() {
                "issues" => self.browse(dir, &["fail", "warn"]),
                "notes" => self.browse(dir, &["info", "skip"]),
                "passed" => self.browse(dir, &["pass"]),
                "diff" => {
                    if let Some(p) = &prev {
                        self.show_diff(p, dir);
                    }
                }
                "log" => {
                    let _ = Command::new("less")
                        .arg("-R")
                        .arg("+g")
                        .arg(dir.join("output.txt"))
                        .status();
                }
                "export" => self.export_md(&run),
                "rescan" => {
                    self.scan(&run.meta.mode);
                    return;
                }
                _ => return,
            }
        }
    }
}
