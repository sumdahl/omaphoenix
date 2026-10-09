//! OmaPhoenix's on-disk contracts, byte-compatible with the Go version so a
//! snapshot repo written by either can be read by the other.
//!
//! - [`manifest`]: `manifest.json`, one per snapshot.
//! - [`selection`]: `selection.conf`, which items a snapshot captures.
//!
//! Parity is proven against fixtures written by the Go code
//! (`tests/fixtures/go`).

pub mod manifest;
pub mod selection;

mod glob;

pub use manifest::{FORMAT, Item, Manifest, ManifestError};
pub use selection::{Choice, Rules};
