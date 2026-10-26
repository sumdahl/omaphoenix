//! The finding contract (ADR-0016): JSON lines; legacy lines read as schema 0, newer schemas refused.

use serde::{Deserialize, Serialize};

/// Version of the finding contract this crate speaks.
pub const CONTRACT_SCHEMA: u32 = 1;

/// Longest line [`Finding::parse_line`] accepts; bounds memory on hostile input.
pub const MAX_LINE: usize = 64 * 1024;

/// How bad a finding is.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[serde(rename_all = "lowercase")]
#[non_exhaustive]
pub enum Severity {
    /// A real problem; costs 10 points.
    Fail,
    /// Worth fixing; costs 2 points.
    Warn,
    /// Checked and fine.
    Pass,
    /// Context only.
    Info,
    /// Not checked, usually because it needs root.
    Skip,
}

/// What a finding is about, so the gate can map it to a snapshot item.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "lowercase")]
#[non_exhaustive]
pub enum Target {
    /// An absolute path.
    File {
        /// The file.
        path: String,
    },
    /// A systemd unit, `user/foo.service` or `system/foo.service`.
    Unit {
        /// Scope and unit name.
        name: String,
    },
    /// A package.
    Package {
        /// Package name.
        name: String,
    },
    /// Machine-level: nothing a snapshot could exclude.
    None,
}

/// One audit finding.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct Finding {
    /// Contract version; 0 for legacy lines.
    #[serde(default)]
    pub schema: u32,
    /// Stable check id, `section.check`; absent on legacy lines.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    /// Severity.
    pub sev: Severity,
    /// Report section heading.
    pub section: String,
    /// One-line message; its wording may change, the id may not.
    pub msg: String,
    /// Extra detail, possibly multi-line.
    #[serde(default)]
    pub detail: String,
    /// A shell command that fixes it, shown to the user, never run unasked.
    #[serde(default)]
    pub fix: String,
    /// Advice in prose.
    #[serde(default)]
    pub hint: String,
    /// What the finding concerns.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub targets: Vec<Target>,
}

/// Why a line is not a usable finding.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum ContractError {
    /// The line is over [`MAX_LINE`] bytes.
    #[error("finding line is {0} bytes, over the {MAX_LINE}-byte limit")]
    TooLong(usize),
    /// Not valid finding JSON.
    #[error("not a valid finding")]
    Json(#[source] serde_json::Error),
    /// Written by a newer Security Audit than this reader understands.
    #[error(
        "finding uses contract schema {0}; this version understands up to {CONTRACT_SCHEMA}. Update OmaPhoenix"
    )]
    NewerSchema(u32),
    /// A schema 1+ finding without a valid `section.check` id.
    #[error("finding has no valid id (want lowercase `section.check`): {0:?}")]
    BadId(Option<String>),
}

impl Finding {
    /// Parses one JSONL line, legacy or v1.
    ///
    /// # Errors
    /// See [`ContractError`].
    pub fn parse_line(line: &str) -> Result<Self, ContractError> {
        if line.len() > MAX_LINE {
            return Err(ContractError::TooLong(line.len()));
        }
        let f: Self = serde_json::from_str(line).map_err(ContractError::Json)?;
        if f.schema > CONTRACT_SCHEMA {
            return Err(ContractError::NewerSchema(f.schema));
        }
        if f.schema >= 1 && !f.id.as_deref().is_some_and(valid_id) {
            return Err(ContractError::BadId(f.id));
        }
        Ok(f)
    }
}

/// `section.check`: two or more lowercase `[a-z0-9_]` parts joined by dots.
fn valid_id(id: &str) -> bool {
    let mut parts = id.split('.');
    let ok_part = |p: &str| {
        !p.is_empty()
            && p.bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_')
    };
    parts.clone().count() >= 2 && parts.all(ok_part)
}

#[cfg(test)]
mod tests;
