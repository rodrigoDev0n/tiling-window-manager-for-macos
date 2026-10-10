//! Core Graphics interoperability for display identity.
//!
//! Only display identity lives here; geometry comes from AppKit (`NSScreen`),
//! which reports values in the same point units the rest of the layout domain
//! uses.

use std::os::raw::c_uint;

#[link(name = "CoreGraphics", kind = "framework")]
unsafe extern "C" {
    /// Returns the `CGDirectDisplayID` of the main display.
    pub fn CGMainDisplayID() -> c_uint;
}

/// Returns the identifier of the main (menu-bar) display.
pub fn main_display_id() -> u32 {
    // SAFETY: `CGMainDisplayID` takes no arguments and has no preconditions.
    unsafe { CGMainDisplayID() }
}
