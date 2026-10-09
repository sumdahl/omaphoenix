//! The finding contract (docs/04, ADR-0016): one JSON object per line,
//! written by `secaudit --json` and read by OmaPhoenix's audit gate.
//!
//! Readers accept today's Bash lines, which have no `schema`, `id` or
//! `targets`, as schema 0, and refuse a schema newer than
//! [`CONTRACT_SCHEMA`], so a format change can never be misread silently.

use serde::{Deserialize, Serialize};

/// Version of the finding contract this crate speaks.
pub const CONTRACT_SCHEMA: u32 = 1;

/// Longest line [`Finding::parse_line`] accepts. Real findings are well under
/// 4 KiB; the bound keeps a hostile or corrupt file from costing memory.
pub const MAX_LINE: usize = 64 * 1024;

/// How bad a finding is.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Hash)]
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
mod tests {
    use super::{CONTRACT_SCHEMA, ContractError, Finding, MAX_LINE, Severity, Target};

    const LEGACY: &str = include_str!("../../../tests/fixtures/secaudit/legacy.jsonl");
    const V1: &str = include_str!("../../../tests/fixtures/secaudit/v1.jsonl");

    fn parse(line: &str) -> Finding {
        match Finding::parse_line(line) {
            Ok(f) => f,
            Err(e) => unreachable!("fixture line must parse: {e}: {line}"),
        }
    }

    /// Every fixture parses, and re-serialising gives the same JSON value.
    #[test]
    fn fixtures_parse_and_round_trip() {
        for line in LEGACY.lines().chain(V1.lines()) {
            let f = parse(line);
            let again = serde_json::to_value(&f).ok();
            let orig: Option<serde_json::Value> = serde_json::from_str(line).ok();
            let mut orig = orig.unwrap_or_default();
            // Legacy lines gain `schema: 0` when written back; nothing else changes.
            if f.schema == 0 {
                orig["schema"] = 0.into();
            }
            // An empty `targets` is omitted on write.
            if orig
                .get("targets")
                .is_some_and(|t| t == &serde_json::json!([]))
            {
                orig.as_object_mut().map(|o| o.remove("targets"));
            }
            assert_eq!(again, Some(orig), "{line}");
        }
    }

    #[test]
    fn legacy_lines_are_schema_zero_without_id() {
        let f = parse(LEGACY.lines().next().unwrap_or_default());
        assert_eq!((f.schema, f.id, f.sev), (0, None, Severity::Warn));
        assert!(f.targets.is_empty());
    }

    #[test]
    fn v1_targets_keep_their_kind() {
        let kinds: Vec<Vec<Target>> = V1.lines().map(|l| parse(l).targets).collect();
        assert_eq!(
            kinds[1],
            vec![
                Target::Unit {
                    name: "user/sync.service".into()
                },
                Target::File {
                    path: "/home/u/.config/systemd/user/sync.service".into()
                },
            ]
        );
        assert_eq!(kinds[3], vec![Target::None]);
    }

    #[test]
    fn refuses_a_newer_schema() {
        let line = format!(
            r#"{{"schema":{},"id":"a.b","sev":"pass","section":"s","msg":"m"}}"#,
            CONTRACT_SCHEMA + 1
        );
        assert!(matches!(
            Finding::parse_line(&line),
            Err(ContractError::NewerSchema(2))
        ));
    }

    #[test]
    fn v1_requires_a_valid_id() {
        for id in [
            None,
            Some(""),
            Some("nodot"),
            Some("Secrets.x"),
            Some("a..b"),
            Some("a.b-c"),
        ] {
            let id_field = id.map_or(String::new(), |i| format!(r#""id":"{i}","#));
            let line = format!(r#"{{"schema":1,{id_field}"sev":"pass","section":"s","msg":"m"}}"#);
            assert!(
                matches!(Finding::parse_line(&line), Err(ContractError::BadId(_))),
                "{line}"
            );
        }
        for id in ["a.b", "secrets.env_readable", "net.v6.ra_2"] {
            let line =
                format!(r#"{{"schema":1,"id":"{id}","sev":"pass","section":"s","msg":"m"}}"#);
            assert!(Finding::parse_line(&line).is_ok(), "{line}");
        }
    }

    #[test]
    fn rejects_unknown_severity_and_target_kind() {
        assert!(Finding::parse_line(r#"{"sev":"bad","section":"s","msg":"m"}"#).is_err());
        let line = r#"{"schema":1,"id":"a.b","sev":"pass","section":"s","msg":"m","targets":[{"kind":"disk"}]}"#;
        assert!(Finding::parse_line(line).is_err());
    }

    #[test]
    fn bounds_line_length() {
        let line = format!(
            r#"{{"sev":"pass","section":"s","msg":"{}"}}"#,
            "x".repeat(MAX_LINE)
        );
        assert!(matches!(
            Finding::parse_line(&line),
            Err(ContractError::TooLong(_))
        ));
    }
}
