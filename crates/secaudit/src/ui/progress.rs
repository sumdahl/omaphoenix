//! The live scan progress screen.

use std::fmt::Write as _;
use std::path::Path;

use super::Ui;
use super::format::{B, EL, GRN, I, N, RED, YEL, lpad, mmss, print, rpad, width};

impl Ui {
    pub(super) fn render_progress(
        &self,
        dir: &Path,
        mode: &str,
        frame: &str,
        elapsed: u64,
        done: bool,
    ) {
        let w = width();
        let secs = self.engine_sections();
        let total = secs.len().max(1);
        let out = std::fs::read_to_string(dir.join("output.txt")).unwrap_or_default();
        let mut order: Vec<(String, usize, usize)> = Vec::new();
        let mut cur: Option<usize> = None;
        for line in out.lines() {
            if let Some(sec) = line.strip_prefix("== ").and_then(|l| l.strip_suffix(" ==")) {
                cur = Some(order.iter().position(|o| o.0 == sec).unwrap_or_else(|| {
                    order.push((sec.to_owned(), 0, 0));
                    order.len() - 1
                }));
            }
            if let Some(o) = cur.and_then(|i| order.get_mut(i)) {
                if line.contains("[WARN]") {
                    o.1 += 1;
                }
                if line.contains("[FAIL]") {
                    o.2 += 1;
                }
            }
        }
        order.retain(|o| o.0 != "Summary");
        let current = order.last().map(|o| o.0.clone()).unwrap_or_default();
        let completed = if done {
            total
        } else {
            order.len().saturating_sub(1)
        };
        let who = if mode == "quick" {
            format!("as {}", self.user)
        } else {
            "as root".into()
        };
        let mut cap = mode.to_owned();
        if let Some(f) = cap.get_mut(0..1) {
            f.make_ascii_uppercase();
        }
        print("\x1b[H");
        self.title_bar(
            "Security Audit",
            &format!("{cap} scan · {who} · {}", mmss(elapsed)),
        );
        let mut s = format!("{EL}\n");
        let (a, m) = (&self.a, &self.m);
        for name in &secs {
            let seen = order.iter().find(|o| &o.0 == name);
            match seen {
                Some((_, sw, sf)) if *name != current || done => {
                    let mut tag = String::new();
                    let ic = if *sf > 0 {
                        tag = format!("{RED}{sf} fail{N}");
                        format!("{RED}✗{N}")
                    } else if *sw > 0 {
                        format!("{YEL}!{N}")
                    } else {
                        format!("{GRN}✓{N}")
                    };
                    if *sw > 0 {
                        let sep = if tag.is_empty() { "" } else { "  " };
                        let _ = write!(tag, "{sep}{YEL}{sw} warn{N}");
                    }
                    let _ = writeln!(s, "   {ic}  {} {tag}{EL}", lpad(name, 34));
                }
                _ if *name == current => {
                    let _ = writeln!(
                        s,
                        "   {a}{frame}{N}  {B}{}{N} {m}{I}checking…{N}{EL}",
                        lpad(name, 34)
                    );
                }
                _ => {
                    let _ = writeln!(s, "   {m}·  {name}{N}{EL}");
                }
            }
        }
        let bw = w.saturating_sub(14);
        let filled = bw * completed / total;
        let _ = write!(
            s,
            "{EL}\n   {a}{}{N}{m}{}{N}  {m}{} / {total}{N}{EL}\n",
            "━".repeat(filled.max(1)),
            "─".repeat(bw.saturating_sub(filled).max(1)),
            rpad(&completed.to_string(), 2)
        );
        if !done {
            let _ = write!(s, "\n   {m}ctrl+c to cancel{N}{EL}");
        }
        s.push_str("\x1b[J");
        print(&s.replace('\n', "\r\n"));
    }
}
