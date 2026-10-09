//! Report sections, ported one at a time from the Bash `bin/secaudit`.
//! Each keeps the Bash messages word for word, so the parity harness
//! (`scripts/secaudit-parity`) can diff the two on the same machine.

use crate::audit::Section;

mod secrets;

/// Every ported section, in report order.
pub const SECTIONS: &[Section] = &[secrets::SECTION];
