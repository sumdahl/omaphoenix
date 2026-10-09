//! Security Audit engine and finding contract.
//!
//! Shared by the standalone Security Audit app and by OmaPhoenix, which runs
//! an audit before every backup. This crate must never depend on any
//! OmaPhoenix crate: it moves to its owner's repository at handover
//! (ADR-0013).
//!
//! Status: the finding contract is in [`contract`]. The ported checks arrive
//! with issue #5.

pub mod contract;

pub use contract::{CONTRACT_SCHEMA, ContractError, Finding, Severity, Target};
