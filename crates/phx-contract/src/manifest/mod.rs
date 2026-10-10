//! `manifest.json`: what one snapshot holds and where it came from.

mod audit;
#[cfg(test)]
mod tests;

use std::collections::BTreeMap;

use serde::{Deserialize, Deserializer, Serialize};

pub use audit::{AuditSummary, GateItem};

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

/// A snapshot's manifest; category keys stay sorted, as Go writes them.
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
    /// What the audit gate left out or flagged; absent when no audit ran.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub audit: Option<AuditSummary>,
    /// Items per category; an empty list is written as `null`, as Go does.
    #[serde(
        default,
        deserialize_with = "null_as_empty",
        serialize_with = "empty_as_null"
    )]
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
