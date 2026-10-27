//! Report sections; messages match the Bash `bin/secaudit` word for word for the parity harness.

#![allow(
    clippy::case_sensitive_file_extension_comparisons,
    reason = "find -name and shell globs are case-sensitive; parity with Bash"
)]

use crate::audit::Section;

mod boot;
mod containers;
mod filesystem;
mod firewall;
mod kernel;
mod network;
mod packages;
mod rootkit;
mod screen_lock;
mod secrets;
mod services;
mod ssh;
mod users;

#[cfg(test)]
pub(crate) mod testing;

/// Every ported section, in report order.
pub const SECTIONS: &[Section] = &[
    packages::SECTION,
    boot::SECTION,
    kernel::SECTION,
    firewall::SECTION,
    network::SECTION,
    ssh::SECTION,
    users::SECTION,
    screen_lock::SECTION,
    filesystem::SECTION,
    services::SECTION,
    containers::SECTION,
    secrets::SECTION,
    rootkit::SECTION,
];
