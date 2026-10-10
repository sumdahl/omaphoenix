//! Security Audit engine and finding contract; must never depend on an OmaPhoenix crate (ADR-0013).

pub mod audit;
pub mod checks;
pub mod contract;
pub mod store;

pub use contract::{CONTRACT_SCHEMA, ContractError, Finding, Severity, Target};
