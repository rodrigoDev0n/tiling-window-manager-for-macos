//! Application window enumeration built on the Accessibility API.

use std::os::raw::c_void;

use core_foundation::base::TCFType;
use core_foundation::string::CFString;
use core_foundation_sys::array::{CFArrayGetCount, CFArrayGetValueAtIndex, CFArrayRef};
use core_foundation_sys::base::{CFGetTypeID, CFTypeRef};
use core_foundation_sys::string::CFStringGetTypeID;

use crate::core::geometry::Rect;
use crate::core::window::{ProcessId, WindowInfo};
use crate::platform::macos::accessibility::{
    self, AX_ERROR_SUCCESS, AXUIElementRef, AXValueRef, CGPoint, CGSize, CfGuard,
    K_AX_VALUE_CG_POINT_TYPE, K_AX_VALUE_CG_SIZE_TYPE,
};
use crate::platform::macos::applications;

/// Discovers the windows of every accessible running application.
pub fn discover_windows() -> Vec<WindowInfo> {
    applications::running_applications()
        .into_iter()
        .flat_map(|app| discover_application_windows(app.pid, &app.name))
        .collect()
}

fn discover_application_windows(pid: ProcessId, application: &str) -> Vec<WindowInfo> {
    let mut windows = Vec::new();

    // SAFETY: `pid` comes from NSWorkspace; the returned element is owned.
    let application_element = unsafe { accessibility::AXUIElementCreateApplication(pid.0) };
    if application_element.is_null() {
        return windows;
    }
    let _application_guard = unsafe { CfGuard::new(application_element as CFTypeRef) };

    let Some(array_guard) =
        copy_attribute(application_element, accessibility::AX_WINDOWS_ATTRIBUTE)
    else {
        return windows;
    };

    let array = array_guard.as_ref() as CFArrayRef;
    // SAFETY: `array` is a valid CFArray returned by the windows attribute.
    let count = unsafe { CFArrayGetCount(array) };

    for index in 0..count {
        // SAFETY: `index` is within `[0, count)`, so the borrowed pointer is valid
        // for as long as `array_guard` is alive.
        let element = unsafe { CFArrayGetValueAtIndex(array, index) } as AXUIElementRef;
        if element.is_null() {
            continue;
        }

        let title = copy_string(element, accessibility::AX_TITLE_ATTRIBUTE)
            .filter(|title| !title.is_empty());

        windows.push(WindowInfo {
            pid,
            application: application.to_string(),
            title,
            bounds: window_bounds(element),
        });
    }

    windows
}

pub(crate) fn window_bounds(element: AXUIElementRef) -> Option<Rect> {
    let position = copy_point(element, accessibility::AX_POSITION_ATTRIBUTE)?;
    let size = copy_size(element, accessibility::AX_SIZE_ATTRIBUTE)?;

    let rect = Rect::new(position.x, position.y, size.width, size.height);
    rect.is_valid().then_some(rect)
}

/// Copies an attribute, returning an owned guard or `None` when unsupported.
pub(crate) fn copy_attribute(element: AXUIElementRef, attribute: &str) -> Option<CfGuard> {
    let attribute = CFString::new(attribute);
    let mut value: CFTypeRef = std::ptr::null();

    // SAFETY: `element` is a valid accessibility element, `attribute` is a
    // valid CFString, and `value` points to valid writable storage. The returned
    // value follows the Create rule on success.
    let error = unsafe {
        accessibility::AXUIElementCopyAttributeValue(
            element,
            attribute.as_concrete_TypeRef(),
            &mut value,
        )
    };

    if error == AX_ERROR_SUCCESS && !value.is_null() {
        // SAFETY: on success the value follows the Create rule (owned `+1`).
        Some(unsafe { CfGuard::new(value) })
    } else {
        None
    }
}

pub(crate) fn copy_string(element: AXUIElementRef, attribute: &str) -> Option<String> {
    let guard = copy_attribute(element, attribute)?;

    // SAFETY: the value is checked to be a CFString before wrapping. The guard
    // keeps the original reference alive; `wrap_under_get_rule` adds its own.
    unsafe {
        if CFGetTypeID(guard.as_ref()) != CFStringGetTypeID() {
            return None;
        }
        Some(
            CFString::wrap_under_get_rule(
                guard.as_ref() as core_foundation_sys::string::CFStringRef
            )
            .to_string(),
        )
    }
}

pub(crate) fn copy_point(element: AXUIElementRef, attribute: &str) -> Option<CGPoint> {
    let guard = copy_ax_value(element, attribute)?;
    let mut point = CGPoint::default();

    // SAFETY: the value was confirmed to be an AXValue and `point` has the
    // layout expected by the CGPoint variant.
    let success = unsafe {
        accessibility::AXValueGetValue(
            guard.as_ref() as AXValueRef,
            K_AX_VALUE_CG_POINT_TYPE,
            &mut point as *mut CGPoint as *mut c_void,
        )
    };
    (success != 0).then_some(point)
}

pub(crate) fn copy_size(element: AXUIElementRef, attribute: &str) -> Option<CGSize> {
    let guard = copy_ax_value(element, attribute)?;
    let mut size = CGSize::default();

    // SAFETY: the value was confirmed to be an AXValue and `size` has the
    // layout expected by the CGSize variant.
    let success = unsafe {
        accessibility::AXValueGetValue(
            guard.as_ref() as AXValueRef,
            K_AX_VALUE_CG_SIZE_TYPE,
            &mut size as *mut CGSize as *mut c_void,
        )
    };
    (success != 0).then_some(size)
}

fn copy_ax_value(element: AXUIElementRef, attribute: &str) -> Option<CfGuard> {
    let guard = copy_attribute(element, attribute)?;
    // SAFETY: type id comparison on a live Core Foundation object.
    let is_ax_value = unsafe { CFGetTypeID(guard.as_ref()) == accessibility::AXValueGetTypeID() };
    is_ax_value.then_some(guard)
}
