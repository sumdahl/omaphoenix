//! Running a scan.

use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use ratatui::crossterm::event::{self, Event, KeyCode, KeyModifiers};
use ratatui::crossterm::terminal;
use secaudit_core::store::{KEEP_RUNS, now};

use super::Ui;
use super::format::{CLEAR, FRAMES, N, RED, YEL, counts, grade_of, print, score_of};

impl Ui {
    #[allow(clippy::too_many_lines, reason = "mirrors scan() in the script")]
    pub(super) fn scan(&self, mode: &str) {
        print(CLEAR);
        let sudo = mode != "quick";
        let mut cap = mode.to_owned();
        if let Some(f) = cap.get_mut(0..1) {
            f.make_ascii_uppercase();
        }
        if sudo {
            self.title_bar("Security Audit", &format!("{cap} scan"));
            print(&format!(
                "\n  {cap} scan reads root-only files (shadow, sudoers, firewall rules, LUKS headers).\n"
            ));
            print(&format!(
                "  {}The audit is read-only; sudo is only used to look.{N}\n\n",
                self.m
            ));
            if !Command::new("sudo")
                .arg("-v")
                .status()
                .is_ok_and(|s| s.success())
            {
                print(&format!("\n  {RED}No sudo access, scan cancelled.{N}\n"));
                self.pause();
                return;
            }
            print(CLEAR);
        }
        let stamp = Command::new("date")
            .arg("+%Y%m%d-%H%M%S")
            .output()
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_owned())
            .unwrap_or_default();
        let dir = self.runs_dir.join(stamp);
        let started = now();
        let _ = std::fs::create_dir_all(&dir);
        let kernel = std::fs::read_to_string("/proc/sys/kernel/osrelease")
            .unwrap_or_default()
            .trim()
            .to_owned();
        let meta = serde_json::json!({"mode": mode, "started": started, "host": self.host, "user": self.user, "kernel": kernel});
        let _ = std::fs::write(dir.join("meta.json"), format!("{meta}\n"));
        let findings = dir.join("findings.jsonl");
        let Ok(out) = std::fs::File::create(dir.join("output.txt")) else {
            return;
        };
        let err = out.try_clone().ok();
        let mut cmd = if sudo {
            let mut c = Command::new("sudo");
            c.arg("-n").arg(&self.engine);
            c
        } else {
            Command::new(&self.engine)
        };
        if mode == "deep" {
            cmd.arg("--deep");
        }
        cmd.arg("--json")
            .arg(&findings)
            .stdin(Stdio::null())
            .stdout(out);
        if let Some(e) = err {
            cmd.stderr(e);
        }
        let Ok(mut child) = cmd.spawn() else { return };
        let t0 = Instant::now();
        let _ = terminal::enable_raw_mode();
        print("\x1b[?25l");
        print(CLEAR);
        let mut i = 0usize;
        let cancelled = loop {
            if child.try_wait().ok().flatten().is_some() {
                break false;
            }
            let ctrl_c = event::poll(Duration::from_millis(120)).unwrap_or(false)
                && matches!(event::read(), Ok(Event::Key(k)) if k.code == KeyCode::Char('c') && k.modifiers.contains(KeyModifiers::CONTROL));
            if ctrl_c {
                // The engine may run as root; kill its whole process tree.
                let pattern = format!("--json {}", findings.display());
                let mut pk = if sudo {
                    Command::new("sudo")
                } else {
                    Command::new("pkill")
                };
                if sudo {
                    pk.args(["-n", "pkill"]);
                }
                let _ = pk
                    .args(["-TERM", "-f", "--", &pattern])
                    .stderr(Stdio::null())
                    .status();
                let _ = child.kill();
                let _ = child.wait();
                let _ = terminal::disable_raw_mode();
                print("\x1b[?25h");
                let _ = std::fs::remove_dir_all(&dir);
                print(&format!("\n\n   {YEL}Scan cancelled.{N}\n"));
                std::thread::sleep(Duration::from_secs(1));
                break true;
            }
            let frame = FRAMES.get(i % FRAMES.len()).copied().unwrap_or("⠋");
            i += 1;
            self.render_progress(&dir, mode, frame, t0.elapsed().as_secs(), false);
        };
        if cancelled {
            return;
        }
        let _ = child.wait();
        let finished = now();
        let mut meta = meta;
        meta["finished"] = finished.into();
        let _ = std::fs::write(dir.join("meta.json"), format!("{meta}\n"));
        self.render_progress(&dir, mode, "", finished.saturating_sub(started), true);
        let _ = terminal::disable_raw_mode();
        print("\x1b[?25h");
        if std::fs::metadata(&findings).map_or(0, |m| m.len()) == 0 {
            print(&format!(
                "\n\n   {RED}The scan produced no results. Last lines of output:{N}\n\n"
            ));
            let out = std::fs::read_to_string(dir.join("output.txt")).unwrap_or_default();
            let l: Vec<&str> = out.lines().collect();
            for line in l.get(l.len().saturating_sub(5)..).unwrap_or_default() {
                print(&format!("     {line}\n"));
            }
            let _ = std::fs::remove_dir_all(&dir);
            self.pause();
            return;
        }
        for old in self.runs_newest_first().iter().skip(KEEP_RUNS) {
            let _ = std::fs::remove_dir_all(old);
        }
        if let Some(run) = Self::load(&dir) {
            let [f, w, ..] = counts(&run);
            let s = score_of(f, w);
            let _ = Command::new("notify-send")
                .args([
                    "-a",
                    "secaudit",
                    "-i",
                    "security-high-symbolic",
                    "Security audit finished",
                ])
                .arg(format!("Score {s} ({}) · {f} fail · {w} warn", grade_of(s)))
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn();
        }
        std::thread::sleep(Duration::from_millis(600));
        self.dashboard(&dir);
    }
}
