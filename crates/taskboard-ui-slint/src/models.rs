// SPDX-License-Identifier: AGPL-3.0-only

//! The slint-boundary model assembly (D2/D3): builds the generated
//! window model values from presentation output and view state, and
//! maps the pure presentation enums 1:1 onto the generated slint
//! enums. Thin by construction — the markup owns all tone/colour
//! decisions (D18); this module only ferries typed data.

use crate::responsive::Breakpoint;

/// Maps the pure breakpoint onto the generated slint enum (D17's
/// push path).
#[must_use]
pub const fn to_slint_breakpoint(breakpoint: Breakpoint) -> crate::Breakpoint {
    match breakpoint {
        Breakpoint::Compact => crate::Breakpoint::Compact,
        Breakpoint::Regular => crate::Breakpoint::Regular,
        Breakpoint::Expansive => crate::Breakpoint::Expansive,
    }
}
