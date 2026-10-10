use serde::{Deserialize, Serialize};

/// The audit gate's record in a snapshot: counts and ids only, never finding text.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct AuditSummary {
    /// What ran the audit.
    pub tool: String,
    /// Its version.
    pub version: String,
    /// Score over the gate sections.
    pub score: u32,
    /// Letter grade.
    pub grade: String,
    /// Items left out of this snapshot (fail findings).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub excluded: Vec<GateItem>,
    /// Items captured despite a warning.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub warned: Vec<GateItem>,
}

/// One item the gate acted on.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct GateItem {
    /// `category:id`, e.g. `config:.config/app/.env`.
    pub item: String,
    /// The finding's stable check id.
    pub id: String,
    /// `fail` or `warn`.
    pub sev: String,
}

impl GateItem {
    /// A gate record.
    #[must_use]
    pub fn new(item: impl Into<String>, id: impl Into<String>, sev: impl Into<String>) -> Self {
        Self {
            item: item.into(),
            id: id.into(),
            sev: sev.into(),
        }
    }
}

impl AuditSummary {
    /// A summary for `tool` at `version`.
    #[must_use]
    pub fn new(
        tool: impl Into<String>,
        version: impl Into<String>,
        score: u32,
        grade: char,
    ) -> Self {
        Self {
            tool: tool.into(),
            version: version.into(),
            score,
            grade: grade.to_string(),
            excluded: Vec::new(),
            warned: Vec::new(),
        }
    }
}
