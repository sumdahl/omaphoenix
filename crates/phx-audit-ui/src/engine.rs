//! The audit engine the apps start for a scan: `omaphoenix audit`, as a separate process so sudo can run it.

use std::path::PathBuf;

/// The argument that makes `omaphoenix` the audit engine.
pub const ARG: &str = "audit";

/// The `omaphoenix` binary: `$OMAPHOENIX_AUDIT_ENGINE`, this program when it is `omaphoenix`,
/// else `omaphoenix` next to it (the desktop app), else on `PATH`.
#[must_use]
pub fn exe() -> PathBuf {
    if let Some(e) = std::env::var_os("OMAPHOENIX_AUDIT_ENGINE").filter(|e| !e.is_empty()) {
        return PathBuf::from(e);
    }
    let me = std::env::current_exe().ok();
    if let Some(m) = me
        .as_ref()
        .filter(|m| m.file_name().is_some_and(|n| n == "omaphoenix"))
    {
        return m.clone();
    }
    me.and_then(|m| m.parent().map(|d| d.join("omaphoenix")))
        .filter(|p| p.is_file())
        .unwrap_or_else(|| PathBuf::from("omaphoenix"))
}
