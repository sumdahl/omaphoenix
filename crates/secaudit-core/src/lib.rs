//! Security Audit engine and finding contract.
//!
//! Shared by the standalone Security Audit app and by OmaPhoenix, which runs
//! an audit before every backup. This crate must never depend on any
//! OmaPhoenix crate: it moves to its owner's repository at handover
//! (ADR-0013).
//!
//! - [`contract`]: the finding contract (docs/04, ADR-0016).
//! - [`audit`]: the engine: context, report, score.
//! - [`checks`]: report sections, ported from the Bash version (issue #5).

pub mod audit;
pub mod checks;
pub mod contract;

pub use contract::{CONTRACT_SCHEMA, ContractError, Finding, Severity, Target};
