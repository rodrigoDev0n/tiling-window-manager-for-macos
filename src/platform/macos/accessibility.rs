//! macOS Accessibility (ApplicationServices) interoperability.
//!
//! This module is the single place where the raw AX FFI lives. Every pointer
//! crossing the boundary is either owned by a small RAII guard or wrapped with
//! Core Foundation's retain/release rules. Higher-level modules (`windows`)
//! build on the primitives exported here instead of declaring their own FFI.

use std::os::raw::{c_uchar, c_void};

use core_foundation_sys::string::CFStringRef;

/// Core Foundation `Boolean`.
pub type Boolean = c_uchar;
/// Opaque handle to an accessibility element.
pub type AXUIElementRef = *const c_void;
/// Opaque handle to a boxed accessibility value.
pub type AXValueRef = *const c_void;
/// Accessibility error code (`AXError`).
pub type AXError = i32;

/// `kAXErrorSuccess`.
pub const AX_ERROR_SUCCESS: AXError = 0;

/// `kAXValueTypeCGPoint`.
pub const K_AX_VALUE_CG_POINT_TYPE: i32 = 1;
/// `kAXValueTypeCGSize`.
pub const K_AX_VALUE_CG_SIZE_TYPE: i32 = 2;

/// Core Graphics point, laid out as expected by `AXValueGetValue`.
#[repr(C)]
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct CGPoint {
    pub x: f64,
    pub y: f64,
}

/// Core Graphics size, laid out as expected by `AXValueGetValue`.
#[repr(C)]
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct CGSize {
    pub width: f64,
    pub height: f64,
}

#[link(name = "ApplicationServices", kind = "framework")]
unsafe extern "C" {
    /// Returns whether the current process is trusted for accessibility access.
    pub fn AXIsProcessTrusted() -> Boolean;
    /// Creates an accessibility element for the application with the given pid.
    pub fn AXUIElementCreateApplication(pid: libc::pid_t) -> AXUIElementRef;
    /// Copies the value of an attribute into `value` (returned with the Create rule).
    pub fn AXUIElementCopyAttributeValue(
        element: AXUIElementRef,
        attribute: CFStringRef,
        value: *mut core_foundation_sys::base::CFTypeRef,
    ) -> AXError;
    /// Extracts a typed value from an `AXValue` element.
    pub fn AXValueGetValue(value: AXValueRef, the_type: i32, out_value: *mut c_void) -> Boolean;
    /// Returns the Core Foundation type id for `AXValue`.
    pub fn AXValueGetTypeID() -> core_foundation_sys::base::CFTypeID;
}

/// Value of the `kAXWindowsAttribute` attribute constant.
///
/// Modern SDKs ship the accessibility attribute constants as `CFSTR(...)`
/// macros instead of exported variables, so they are encoded here as strings.
pub const AX_WINDOWS_ATTRIBUTE: &str = "AXWindows";
/// Value of the `kAXTitleAttribute` attribute constant.
pub const AX_TITLE_ATTRIBUTE: &str = "AXTitle";
/// Value of the `kAXPositionAttribute` attribute constant.
pub const AX_POSITION_ATTRIBUTE: &str = "AXPosition";
/// Value of the `kAXSizeAttribute` attribute constant.
pub const AX_SIZE_ATTRIBUTE: &str = "AXSize";

/// Returns whether this process currently holds Accessibility authorization.
///
/// The check never prompts the user, so callers may invoke it freely without
/// triggering repeated permission dialogs.
pub fn is_trusted() -> bool {
    // SAFETY: `AXIsProcessTrusted` takes no arguments and has no preconditions.
    unsafe { AXIsProcessTrusted() != 0 }
}

/// Human-readable instructions shown when authorization is missing.
pub const PERMISSION_INSTRUCTIONS: &str = "\
Accessibility permission is required to inspect windows.

To grant it:
  1. Open System Settings.
  2. Go to Privacy & Security > Accessibility.
  3. Enable the entry for this program (or add it with the \"+\" button).
  4. Re-run the command.

Note: after rebuilding the binary you may need to toggle the entry off and on again.";
