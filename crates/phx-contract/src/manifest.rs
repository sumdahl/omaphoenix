//! `manifest.json`: what one snapshot holds and where it came from.

use std::collections::BTreeMap;

use serde::{Deserialize, Deserializer, Serialize};

/// Snapshot format this crate reads and writes.
pub const FORMAT: u32 = 1;

/// One captured item.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct Item {
    /// Category-specific id, e.g. a package name or a path relative to `~`.
    pub id: String,
    /// Extra facts, e.g. a plugin's `url@commit`.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub note: String,
}

impl Item {
    /// An item with an optional note.
    #[must_use]
    pub fn new(id: impl Into<String>, note: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            note: note.into(),
        }
    }
}

/// A snapshot's manifest. Category keys stay sorted (`BTreeMap`), as Go
/// writes them, so a rewrite doesn't reorder the file.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct Manifest {
    /// Snapshot format; see [`FORMAT`].
    pub format: u32,
    /// `omarchy version` at capture time.
    #[serde(default)]
    pub omarchy_version: String,
    /// Hostname.
    #[serde(default)]
    pub host: String,
    /// DMI vendor and model, for hardware-aware restore; absent on old snapshots.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub hardware: String,
    /// User name.
    #[serde(default)]
    pub user: String,
    /// Login shell.
    #[serde(default)]
    pub shell: String,
    /// What the audit gate left out or flagged; absent when no audit ran
    /// (the Go version, or `OMAPHOENIX_AUDIT=off`). Additive: format 1
    /// readers ignore it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub audit: Option<AuditSummary>,
    /// Items per category. Go writes `null` for none (a nil list) and so
    /// does this, so switching implementations doesn't change the file.
    #[serde(
        default,
        deserialize_with = "null_as_empty",
        serialize_with = "empty_as_null"
    )]
    pub items: BTreeMap<String, Vec<Item>>,
}

/// The audit gate's record in a snapshot (docs/04): counts and IDs only,
/// never finding text that could hold a secret.
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

/// Why a manifest can't be used.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum ManifestError {
    /// Not valid manifest JSON.
    #[error("snapshot manifest is not valid")]
    Json(#[source] serde_json::Error),
    /// Written by a newer OmaPhoenix.
    #[error("snapshot uses format {0}; this version reads up to {FORMAT}. Update OmaPhoenix")]
    NewerFormat(u32),
}

impl Default for Manifest {
    fn default() -> Self {
        Self::new()
    }
}

impl Manifest {
    /// An empty manifest of the current [`FORMAT`].
    #[must_use]
    pub fn new() -> Self {
        Self {
            format: FORMAT,
            omarchy_version: String::new(),
            host: String::new(),
            hardware: String::new(),
            user: String::new(),
            shell: String::new(),
            audit: None,
            items: BTreeMap::new(),
        }
    }

    /// Parses a manifest, refusing formats newer than [`FORMAT`].
    ///
    /// # Errors
    /// See [`ManifestError`].
    pub fn parse(json: &str) -> Result<Self, ManifestError> {
        let m: Self = serde_json::from_str(json).map_err(ManifestError::Json)?;
        if m.format > FORMAT {
            return Err(ManifestError::NewerFormat(m.format));
        }
        Ok(m)
    }
}

fn empty_as_null<S: serde::Serializer>(
    items: &BTreeMap<String, Vec<Item>>,
    s: S,
) -> Result<S::Ok, S::Error> {
    use serde::ser::SerializeMap;
    let mut m = s.serialize_map(Some(items.len()))?;
    for (k, v) in items {
        if v.is_empty() {
            m.serialize_entry(k, &None::<()>)?;
        } else {
            m.serialize_entry(k, v)?;
        }
    }
    m.end()
}

fn null_as_empty<'de, D: Deserializer<'de>>(d: D) -> Result<BTreeMap<String, Vec<Item>>, D::Error> {
    Ok(
        Option::<BTreeMap<String, Option<Vec<Item>>>>::deserialize(d)?
            .unwrap_or_default()
            .into_iter()
            .map(|(k, v)| (k, v.unwrap_or_default()))
            .collect(),
    )
}

#[cfg(test)]
mod tests {
    use super::{Manifest, ManifestError};

    const FIXTURES: [(&str, &str); 3] = [
        (
            "full",
            include_str!("../../../tests/fixtures/go/manifest/full.json"),
        ),
        (
            "minimal",
            include_str!("../../../tests/fixtures/go/manifest/minimal.json"),
        ),
        (
            "nil-items",
            include_str!("../../../tests/fixtures/go/manifest/nil-items.json"),
        ),
    ];

    /// Go's manifests parse, and writing them back gives the same JSON, except
    /// that Go's `null` item map comes back as `{}`.
    #[test]
    fn go_manifests_round_trip() {
        for (name, json) in FIXTURES {
            let m = Manifest::parse(json).unwrap_or_else(|e| unreachable!("{name}: {e}"));
            let mut want: serde_json::Value = serde_json::from_str(json).unwrap_or_default();
            if want["items"].is_null() {
                want["items"] = serde_json::json!({});
            }
            // An empty list is written as null, as Go writes a nil one.
            if let Some(items) = want["items"].as_object_mut() {
                for v in items.values_mut() {
                    if v.as_array().is_some_and(Vec::is_empty) {
                        *v = serde_json::Value::Null;
                    }
                }
            }
            assert_eq!(serde_json::to_value(&m).ok(), Some(want), "{name}");
        }
    }

    #[test]
    fn reads_go_fields() {
        let m = Manifest::parse(FIXTURES[0].1).unwrap_or_else(|e| unreachable!("{e}"));
        assert_eq!(m.hardware, "Dell Inc. Latitude 7430");
        assert_eq!(
            m.items["plugin"][0].note,
            "https://github.com/sumdahl/media@3f2a1c9"
        );
        assert_eq!(
            m.items["config"][2].id, "a<b>&c \"q\" ünï",
            "Go's \\u003c escapes decode"
        );
        assert!(m.items["etc"].is_empty());
    }

    #[test]
    fn refuses_newer_format_and_tolerates_new_fields() {
        assert!(matches!(
            Manifest::parse(r#"{"format":2}"#),
            Err(ManifestError::NewerFormat(2))
        ));
        assert!(Manifest::parse(r#"{"format":1,"audit":null,"future":[1]}"#).is_ok());
        assert!(
            Manifest::parse(r#"{"items":{}}"#).is_err(),
            "format is required"
        );
    }
}
