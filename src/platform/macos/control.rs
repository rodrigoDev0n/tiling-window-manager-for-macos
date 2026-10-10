//! Native window control: moving and resizing windows through the Accessibility API.
//!
//! Ownership notes:
//! - Application elements are created with `AXUIElementCreateApplication` (owned).
//! - Window elements borrowed from `AXWindows` are retained into [`WindowHandle`]
//!   so they outlive the array that yielded them.
//! - `AXValue` geometry boxes are created with `AXValueCreate` (owned) and
//!   released by [`CfGuard`].

use std::fmt;
use std::os::raw::c_void;

use core_foundation::base::TCFType;
use core_foundation::string::CFString;
use core_foundation_sys::array::{CFArrayGetCount, CFArrayGetValueAtIndex, CFArrayRef};
use core_foundation_sys::base::{CFRelease, CFRetain, CFTypeRef};

use crate::core::geometry::Rect;
use crate::core::window::{ProcessId, WindowInfo};
use crate::platform::macos::accessibility::{
    self, AX_ERROR_SUCCESS, AXError, AXUIElementRef, Boolean, CGPoint, CGSize, CfGuard,
    K_AX_VALUE_CG_POINT_TYPE, K_AX_VALUE_CG_SIZE_TYPE,
};
use crate::platform::macos::{applications, windows};

/// Owns a `+1` reference to a window accessibility element.
pub struct WindowHandle {
    element: AXUIElementRef,
}

impl WindowHandle {
    /// Retains `element` so the handle remains valid after the source array is
    /// released.
    fn retain(element: AXUIElementRef) -> Self {
        // SAFETY: `element` is a live accessibility element (usually borrowed
        // from an `AXWindows` array). Retaining makes it owned by this handle.
        unsafe { CFRetain(element as CFTypeRef) };
        Self { element }
    }

    fn as_ptr(&self) -> AXUIElementRef {
        self.element
    }
}

impl Drop for WindowHandle {
    fn drop(&mut self) {
        // SAFETY: the reference was obtained through `CFRetain`.
        unsafe { CFRelease(self.element as CFTypeRef) };
    }
}

/// A discovered window together with a handle able to control it.
pub struct ManagedWindow {
    pub info: WindowInfo,
    pub handle: WindowHandle,
}

/// Reasons a native window operation failed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ControlError {
    /// The window does not expose a writable position or size.
    NotSettable,
    /// The requested rectangle or an `AXValue` box could not be built.
    InvalidGeometry,
    /// The applied geometry could not be read back.
    ReadBack,
    /// The accessibility element no longer exists.
    InvalidElement,
    /// The underlying accessibility API returned an error code.
    Ax(AXError),
}

impl ControlError {
    fn from_ax(error: AXError) -> Self {
        ControlError::Ax(error)
    }

    /// Returns whether the failure means the window cannot be controlled.
    pub fn is_unsupported(&self) -> bool {
        match self {
            ControlError::NotSettable | ControlError::InvalidElement => true,
            ControlError::Ax(error) => matches!(
                *error,
                accessibility::AX_ERROR_ATTRIBUTE_UNSUPPORTED
                    | accessibility::AX_ERROR_ACTION_UNSUPPORTED
                    | accessibility::AX_ERROR_ILLEGAL_ARGUMENT
                    | accessibility::AX_ERROR_CANNOT_COMPLETE
                    | accessibility::AX_ERROR_NOT_IMPLEMENTED
                    | accessibility::AX_ERROR_INVALID_UI_ELEMENT
            ),
            ControlError::InvalidGeometry | ControlError::ReadBack => false,
        }
    }
}

impl fmt::Display for ControlError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ControlError::NotSettable => {
                write!(f, "window does not support position or size changes")
            }
            ControlError::InvalidGeometry => write!(f, "invalid target geometry"),
            ControlError::ReadBack => write!(f, "could not read back the applied geometry"),
            ControlError::InvalidElement => write!(f, "window is no longer available"),
            ControlError::Ax(code) => write!(f, "{}", describe_ax_error(*code)),
        }
    }
}

impl std::error::Error for ControlError {}

fn describe_ax_error(code: AXError) -> String {
    match code {
        accessibility::AX_ERROR_ATTRIBUTE_UNSUPPORTED => {
            "the window does not support this attribute".to_string()
        }
        accessibility::AX_ERROR_ACTION_UNSUPPORTED => {
            "the window does not support this action".to_string()
        }
        accessibility::AX_ERROR_ILLEGAL_ARGUMENT => {
            "the window rejected the requested geometry".to_string()
        }
        accessibility::AX_ERROR_CANNOT_COMPLETE => {
            "the window could not complete the change".to_string()
        }
        accessibility::AX_ERROR_INVALID_UI_ELEMENT => "the window is no longer valid".to_string(),
        accessibility::AX_ERROR_API_DISABLED => {
            "Accessibility API is disabled (check permissions)".to_string()
        }
        accessibility::AX_ERROR_NOT_IMPLEMENTED => {
            "the operation is not implemented for this window".to_string()
        }
        other => format!("accessibility error {other}"),
    }
}

/// Discovers every controllable window together with its accessibility handle.
pub fn discover_managed_windows() -> Vec<ManagedWindow> {
    applications::running_applications()
        .into_iter()
        .flat_map(|app| discover_application_windows(app.pid, &app.name))
        .collect()
}

/// Returns the focused window of the frontmost application, if available.
pub fn focused_window() -> Option<ManagedWindow> {
    let front = applications::frontmost_application()?;

    // SAFETY: `pid` comes from NSWorkspace; the returned element is owned.
    let application_element = unsafe { accessibility::AXUIElementCreateApplication(front.pid.0) };
    if application_element.is_null() {
        return None;
    }
    let _application_guard = unsafe { CfGuard::new(application_element as CFTypeRef) };

    let window_guard = windows::copy_attribute(
        application_element,
        accessibility::AX_FOCUSED_WINDOW_ATTRIBUTE,
    )?;
    let element = window_guard.as_ref() as AXUIElementRef;
    if element.is_null() {
        return None;
    }

    Some(ManagedWindow {
        info: read_window_info(element, front.pid, &front.name),
        handle: WindowHandle::retain(element),
    })
}

/// Applies `rect` to `handle` and returns the geometry macOS actually applied.
pub fn set_bounds(handle: &WindowHandle, rect: Rect) -> Result<Rect, ControlError> {
    if !rect.is_valid() {
        return Err(ControlError::InvalidGeometry);
    }

    // Fail before mutating anything if the window is not controllable.
    check_settable(handle, accessibility::AX_POSITION_ATTRIBUTE)?;
    check_settable(handle, accessibility::AX_SIZE_ATTRIBUTE)?;

    let point = CGPoint {
        x: rect.origin.x,
        y: rect.origin.y,
    };
    let size = CGSize {
        width: rect.size.width,
        height: rect.size.height,
    };

    set_ax_value(
        handle,
        accessibility::AX_POSITION_ATTRIBUTE,
        K_AX_VALUE_CG_POINT_TYPE,
        &point as *const CGPoint as *const c_void,
    )?;
    set_ax_value(
        handle,
        accessibility::AX_SIZE_ATTRIBUTE,
        K_AX_VALUE_CG_SIZE_TYPE,
        &size as *const CGSize as *const c_void,
    )?;

    read_back(handle)
}

/// Raises `handle` and makes it the focused window of `pid`.
pub fn focus_window(handle: &WindowHandle, pid: ProcessId) -> Result<(), ControlError> {
    let action = CFString::new(accessibility::AX_RAISE_ACTION);
    // SAFETY: `handle` is a valid element and `action` a valid CFString.
    let raise_error = unsafe {
        accessibility::AXUIElementPerformAction(handle.as_ptr(), action.as_concrete_TypeRef())
    };
    if raise_error != AX_ERROR_SUCCESS {
        return Err(ControlError::from_ax(raise_error));
    }

    // SAFETY: `pid` comes from NSWorkspace; the returned element is owned.
    let application_element = unsafe { accessibility::AXUIElementCreateApplication(pid.0) };
    if application_element.is_null() {
        return Err(ControlError::InvalidElement);
    }
    let _application_guard = unsafe { CfGuard::new(application_element as CFTypeRef) };

    let attribute = CFString::new(accessibility::AX_FOCUSED_WINDOW_ATTRIBUTE);
    // SAFETY: `application_element` is valid, the attribute is a valid CFString,
    // and the value is a valid AXUIElement (no ownership transfer).
    let error = unsafe {
        accessibility::AXUIElementSetAttributeValue(
            application_element,
            attribute.as_concrete_TypeRef(),
            handle.as_ptr() as CFTypeRef,
        )
    };
    if error == AX_ERROR_SUCCESS {
        Ok(())
    } else {
        Err(ControlError::from_ax(error))
    }
}

fn discover_application_windows(pid: ProcessId, application: &str) -> Vec<ManagedWindow> {
    let mut managed = Vec::new();

    // SAFETY: `pid` comes from NSWorkspace; the returned element is owned.
    let application_element = unsafe { accessibility::AXUIElementCreateApplication(pid.0) };
    if application_element.is_null() {
        return managed;
    }
    let _application_guard = unsafe { CfGuard::new(application_element as CFTypeRef) };

    let Some(array_guard) =
        windows::copy_attribute(application_element, accessibility::AX_WINDOWS_ATTRIBUTE)
    else {
        return managed;
    };

    let array = array_guard.as_ref() as CFArrayRef;
    // SAFETY: `array` is a valid CFArray returned by the windows attribute.
    let count = unsafe { CFArrayGetCount(array) };

    for index in 0..count {
        // SAFETY: `index` is within `[0, count)`, so the borrowed pointer is
        // valid for as long as `array_guard` is alive; the handle retains it.
        let element = unsafe { CFArrayGetValueAtIndex(array, index) } as AXUIElementRef;
        if element.is_null() {
            continue;
        }

        managed.push(ManagedWindow {
            info: read_window_info(element, pid, application),
            handle: WindowHandle::retain(element),
        });
    }

    managed
}

fn read_window_info(element: AXUIElementRef, pid: ProcessId, application: &str) -> WindowInfo {
    WindowInfo {
        pid,
        application: application.to_string(),
        title: windows::copy_string(element, accessibility::AX_TITLE_ATTRIBUTE)
            .filter(|title| !title.is_empty()),
        bounds: windows::window_bounds(element),
    }
}

fn check_settable(handle: &WindowHandle, attribute: &str) -> Result<(), ControlError> {
    let attribute = CFString::new(attribute);
    let mut settable: Boolean = 0;

    // SAFETY: `handle` is valid, `attribute` is a valid CFString, and `settable`
    // points to writable storage.
    let error = unsafe {
        accessibility::AXUIElementIsAttributeSettable(
            handle.as_ptr(),
            attribute.as_concrete_TypeRef(),
            &mut settable,
        )
    };

    if error != AX_ERROR_SUCCESS {
        return Err(ControlError::from_ax(error));
    }
    if settable != 0 {
        Ok(())
    } else {
        Err(ControlError::NotSettable)
    }
}

fn set_ax_value(
    handle: &WindowHandle,
    attribute: &str,
    value_type: i32,
    value: *const c_void,
) -> Result<(), ControlError> {
    // SAFETY: `value` points to a CGPoint or CGSize matching `value_type`.
    let ax_value = unsafe { accessibility::AXValueCreate(value_type, value) };
    if ax_value.is_null() {
        return Err(ControlError::InvalidGeometry);
    }
    let guard = unsafe { CfGuard::new(ax_value as CFTypeRef) };

    let attribute = CFString::new(attribute);
    // SAFETY: `handle` is valid and `guard` wraps a valid AXValue.
    let error = unsafe {
        accessibility::AXUIElementSetAttributeValue(
            handle.as_ptr(),
            attribute.as_concrete_TypeRef(),
            guard.as_ref(),
        )
    };

    if error == AX_ERROR_SUCCESS {
        Ok(())
    } else {
        Err(ControlError::from_ax(error))
    }
}

fn read_back(handle: &WindowHandle) -> Result<Rect, ControlError> {
    let position = windows::copy_point(handle.as_ptr(), accessibility::AX_POSITION_ATTRIBUTE)
        .ok_or(ControlError::ReadBack)?;
    let size = windows::copy_size(handle.as_ptr(), accessibility::AX_SIZE_ATTRIBUTE)
        .ok_or(ControlError::ReadBack)?;

    let rect = Rect::new(position.x, position.y, size.width, size.height);
    rect.is_valid()
        .then_some(rect)
        .ok_or(ControlError::ReadBack)
}
