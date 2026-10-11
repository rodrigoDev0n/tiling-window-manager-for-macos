//! Platform-independent window metadata.

use crate::core::geometry::Rect;

/// A process identifier, modeled as a strong type to avoid mixing identities.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ProcessId(pub i32);

/// An opaque, caller-assigned identifier for a single window within one layout
/// calculation.
///
/// The layout engine never dereferences this value, so it can wrap any
/// caller-provided unique handle (for example an index assigned during
/// discovery). Identifiers must be unique within a calculation and must not be
/// persisted across process executions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct WindowId(pub u64);

/// Metadata describing a discoverable application window.
#[derive(Debug, Clone, PartialEq)]
pub struct WindowInfo {
    /// Owning process identifier.
    pub pid: ProcessId,
    /// Localized application name.
    pub application: String,
    /// Window title when the application exposes one.
    pub title: Option<String>,
    /// Window geometry when the application reports it.
    pub bounds: Option<Rect>,
}

/// Native classification of a window used for eligibility decisions.
///
/// The values are plain tokens so the `core` layer stays independent of macOS;
/// the platform layer maps Accessibility attributes onto them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WindowClassification {
    /// The accessibility role, for example `AXWindow` for standard windows.
    pub role: Option<String>,
    /// Whether the window is minimized, when the attribute is exposed.
    pub minimized: Option<bool>,
    /// Whether the window is in native fullscreen, when the attribute is exposed.
    pub fullscreen: Option<bool>,
    /// Whether the window's position and size can be changed programmatically,
    /// when the platform can determine it (SPEC-009 FR-016).
    pub controllable: Option<bool>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn window_info_is_constructible_without_geometry() {
        let window = WindowInfo {
            pid: ProcessId(4321),
            application: "Example".to_string(),
            title: None,
            bounds: None,
        };
        assert_eq!(window.pid, ProcessId(4321));
        assert!(window.bounds.is_none());
    }

    #[test]
    fn window_id_carries_an_opaque_value() {
        assert_eq!(WindowId(7), WindowId(7));
        assert_ne!(WindowId(7), WindowId(8));
    }
}
