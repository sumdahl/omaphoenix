//! Findings browser, detail view and fix runner.

use std::fmt::Write as _;
use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::Duration;

use phx_audit::store::{Run, sev_name};

use super::format::{
    B, CLEAR, GRN, I, N, RED, icon, label, lines, lpad, print, sev_color, trunc, width,
};
use super::{Ui, gum};

impl Ui {
    pub(super) fn browse(&self, dir: &Path, sevs: &[&str]) {
        let w = width();
        loop {
            let Some(run) = Self::load(dir) else { return };
            print(CLEAR);
            let header = match sevs.first() {
                Some(&"fail") => "Issues · fail first · enter for details and fix",
                Some(&"info") => "Notes · context worth knowing, not problems",
                _ => "Passed checks",
            };
            self.title_bar("Security Audit", &format!("{} scan", run.meta.mode));
            print("\n");
            let applied = run.applied();
            let mut rows: Vec<_> = run
                .items
                .iter()
                .filter(|i| sevs.contains(&sev_name(i.finding.sev)))
                .collect();
            rows.sort_by_key(|i| (phx_audit::store::sev_order(i.finding.sev), i.n));
            let m = &self.m;
            let mut items: Vec<(String, String)> = rows
                .iter()
                .map(|i| {
                    let sev = sev_name(i.finding.sev);
                    let mark = if applied.contains(&i.n) {
                        format!("{GRN}✓ {N}")
                    } else {
                        "  ".into()
                    };
                    let msg = i.finding.msg.strip_suffix(':').unwrap_or(&i.finding.msg);
                    let l = format!(
                        "{}{} {}{N}  {m}{}{N} {mark}{}",
                        sev_color(sev, m),
                        icon(sev),
                        lpad(label(sev), 4),
                        lpad(&trunc(&i.finding.section, 22), 22),
                        trunc(msg, w.saturating_sub(40))
                    );
                    (l, i.n.to_string())
                })
                .collect();
            items.push((format!("{m}← Back{N}"), "back".into()));
            let Some(choice) = self.choose(header, lines().saturating_sub(8), &items) else {
                return;
            };
            if choice == "back" {
                return;
            }
            if let Ok(n) = choice.parse() {
                self.detail_view(dir, n);
            }
        }
    }

    pub(super) fn detail_view(&self, dir: &Path, n: usize) {
        let Some(run) = Self::load(dir) else { return };
        let Some(item) = run.items.iter().find(|i| i.n == n) else {
            return;
        };
        let fd = &item.finding;
        let sev = sev_name(fd.sev);
        let w = width();
        let (a, m) = (&self.a, &self.m);
        loop {
            print(CLEAR);
            self.title_bar("Security Audit", &fd.section);
            let msg = fd.msg.strip_suffix(':').unwrap_or(&fd.msg);
            let mut o = format!(
                "\n   {B}{}{} {}{N}  {B}{msg}{N}\n",
                sev_color(sev, m),
                icon(sev),
                label(sev)
            );
            if !fd.detail.is_empty() {
                let max = lines().saturating_sub(18).max(4);
                let dl: Vec<&str> = fd.detail.lines().collect();
                let _ = writeln!(o, "\n   {m}Details{N}");
                for l in dl.iter().take(max) {
                    let _ = writeln!(o, "     {}", trunc(l, w.saturating_sub(6)));
                }
                if dl.len() > max {
                    let _ = writeln!(o, "     {m}… {} more (see Full output){N}", dl.len() - max);
                }
            }
            if !fd.fix.is_empty() {
                let _ = writeln!(o, "\n   {m}Suggested fix{N}");
                if fd.fix.chars().count() > w.saturating_sub(6) {
                    let _ = writeln!(o, "     {a}{}{N}", trunc(&fd.fix, w.saturating_sub(6)));
                    let _ = writeln!(
                        o,
                        "     {m}({} chars; copy to see it all){N}",
                        fd.fix.chars().count()
                    );
                } else {
                    let _ = writeln!(o, "     {a}{}{N}", fd.fix);
                }
            }
            if !fd.hint.is_empty() {
                let _ = writeln!(o, "\n   {m}How to address{N}\n     {I}{}{N}", fd.hint);
            }
            if run.applied().contains(&n) {
                let _ = writeln!(
                    o,
                    "\n   {GRN}✓ Fix applied this session; scan again to verify{N}"
                );
            }
            o.push('\n');
            print(&o);
            let mut items = Vec::new();
            if !fd.fix.is_empty() {
                items.push(("Run fix".to_owned(), "run".to_owned()));
                items.push(("Copy fix to clipboard".to_owned(), "copy".to_owned()));
            }
            if fd.fix.is_empty() && !fd.hint.is_empty() {
                items.push(("Copy advice to clipboard".to_owned(), "copyhint".to_owned()));
            }
            items.push(("← Back".to_owned(), "back".to_owned()));
            let Some(choice) = self.choose("", 4, &items) else {
                return;
            };
            match choice.as_str() {
                "run" => self.run_fix(&run, n, &fd.fix),
                "copy" => self.copy_text(&fd.fix),
                "copyhint" => self.copy_text(&fd.hint),
                _ => return,
            }
        }
    }

    pub(super) fn copy_text(&self, text: &str) {
        let copied = Command::new("wl-copy")
            .stdin(Stdio::piped())
            .spawn()
            .ok()
            .and_then(|mut c| {
                c.stdin.take()?.write_all(text.as_bytes()).ok()?;
                c.wait().ok()
            });
        match copied {
            Some(s) if s.success() => {
                print(&format!("{}   ✓ Copied\x1b[0m\n", self.env.fg("2")));
                std::thread::sleep(Duration::from_millis(700));
            }
            Some(_) => {}
            None => {
                print(&format!("\n   {RED}wl-copy not available{N}\n"));
                self.pause();
            }
        }
    }

    pub(super) fn run_fix(&self, run: &Run, n: usize, cmd: &str) {
        print(CLEAR);
        self.title_bar("Security Audit", "Run fix");
        print("\n   This will run:\n\n");
        let fw = width().saturating_sub(6).max(1);
        let chars: Vec<char> = cmd.chars().collect();
        for line in cmd.lines() {
            let lc: Vec<char> = line.chars().collect();
            for chunk in lc.chunks(fw) {
                print(&format!(
                    "     {}{}{N}\n",
                    self.a,
                    chunk.iter().collect::<String>()
                ));
            }
        }
        if chars.is_empty() {
            print(&format!("     {}{N}\n", self.a));
        }
        print("\n");
        if !gum::confirm(&self.env, "Run this command now?", "Run", "Cancel") {
            return;
        }
        print("\n");
        let rc = Command::new("bash")
            .arg("-c")
            .arg(cmd)
            .status()
            .ok()
            .and_then(|s| s.code())
            .unwrap_or(1);
        if rc == 0 {
            let _ = run.mark_applied(n);
            print(&format!(
                "\n   {GRN}✓ Done.{N} {}Run a new scan to confirm the finding is gone.{N}\n",
                self.m
            ));
        } else {
            print(&format!("\n   {RED}✗ Command exited with status {rc}{N}\n"));
        }
        self.pause();
    }
}
