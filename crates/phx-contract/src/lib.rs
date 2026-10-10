//! OmaPhoenix's on-disk contracts, byte-compatible with the Go version's.

pub mod manifest;
pub mod selection;

pub mod glob;

pub use manifest::{AuditSummary, FORMAT, GateItem, Item, Manifest, ManifestError};
pub use selection::{Choice, Rules};
