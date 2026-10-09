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
    /// Items per category. Go writes `null` for none; read as empty.
    #[serde(default, deserialize_with = "null_as_empty")]
    pub items: BTreeMap<String, Vec<Item>>,
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

impl Manifest {
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
