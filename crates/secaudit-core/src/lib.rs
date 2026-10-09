//! Security Audit engine and finding contract.
//!
//! Shared by the standalone Security Audit app and by OmaPhoenix, which runs
//! an audit before every backup. This crate must never depend on any
//! OmaPhoenix crate: it moves to its owner's repository at handover
//! (ADR-0013).
//!
//! Status: skeleton (issue #2). The contract types arrive with issue #3, the
//! ported checks with issue #5.

/// Version of the finding contract this crate speaks (docs/04).
pub const CONTRACT_SCHEMA: u32 = 1;

#[cfg(test)]
mod tests {
    #[test]
    fn contract_schema_is_v1() {
        assert_eq!(super::CONTRACT_SCHEMA, 1);
    }
}
