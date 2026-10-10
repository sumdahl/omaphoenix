use std::fmt::Write as _;
use std::fs;
use std::io::Write;
use std::path::PathBuf;

use super::{Meta, Store, now, sev_name};
use crate::contract::Severity;

impl Store {
    /// Runs a quick in-process audit and records it as run `stamp`.
    ///
    /// # Errors
    /// If the run can't be written.
    pub fn record_quick_scan(
        &self,
        ctx: &crate::audit::Ctx,
        host: &str,
        kernel: &str,
        stamp: &str,
    ) -> std::io::Result<PathBuf> {
        self.record_quick_scan_with(ctx, host, kernel, stamp, |_, _, _| {})
    }

    /// [`Store::record_quick_scan`] with `(done, total, starting)` progress per section.
    ///
    /// # Errors
    /// If the run directory or its files can't be written.
    pub fn record_quick_scan_with(
        &self,
        ctx: &crate::audit::Ctx,
        host: &str,
        kernel: &str,
        stamp: &str,
        mut progress: impl FnMut(usize, usize, &str),
    ) -> std::io::Result<PathBuf> {
        let mut meta = Meta {
            mode: "quick".into(),
            started: now(),
            host: host.to_owned(),
            user: ctx.user.clone(),
            kernel: kernel.to_owned(),
            finished: None,
        };
        let dir = self.begin(stamp, &meta)?;
        let total = crate::checks::SECTIONS.len();
        let mut done = 0;
        let report = crate::audit::run_with(ctx, crate::checks::SECTIONS, |p| match p {
            crate::audit::Progress::Start(name) => progress(done, total, name),
            crate::audit::Progress::Done(..) => done += 1,
        });
        let mut f = std::io::BufWriter::new(fs::File::create(dir.join("findings.jsonl"))?);
        let mut summary = String::new();
        for finding in report.findings() {
            serde_json::to_writer(&mut f, finding).map_err(std::io::Error::other)?;
            f.write_all(b"\n")?;
            if matches!(finding.sev, Severity::Fail | Severity::Warn) {
                let _ = writeln!(
                    summary,
                    "[{}] {}: {}",
                    sev_name(finding.sev).to_uppercase(),
                    finding.section,
                    finding.msg
                );
            }
        }
        f.flush()?;
        fs::write(dir.join("output.txt"), summary)?;
        self.finish(&dir, &mut meta)?;
        Ok(dir)
    }
}
