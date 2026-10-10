use phx_contract::Manifest;

use super::{App, AppError};
use crate::sys;

impl App<'_> {
    /// Re-audits this machine and compares it with the restored snapshot's audit.
    ///
    /// # Errors
    /// When no restore is recorded, or its snapshot carries no audit.
    pub fn reaudit(&self) -> Result<(String, crate::gate::Reaudit), AppError> {
        let j = self.journal()?;
        let text = sys::output(
            Some(&self.repo.dir),
            &["git", "show", &format!("{}:manifest.json", j.snapshot)],
        )
        .map_err(|e| AppError::Other(format!("read snapshot {}: {e}", j.snapshot)))?;
        let m =
            Manifest::parse(&text).map_err(|e| AppError::Other(format!("read manifest: {e}")))?;
        let before = m.audit.ok_or_else(|| {
            AppError::Other(format!(
                "snapshot {} has no audit record (taken with the gate off, or by the Go version)",
                j.snapshot
            ))
        })?;
        let now = crate::gate::run(&self.home, None, &|_, _| {})
            .summary
            .unwrap_or_default();
        Ok((j.snapshot, crate::gate::compare(&before, &now)))
    }
}
