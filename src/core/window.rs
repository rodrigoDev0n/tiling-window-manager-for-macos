//! Platform-independent window metadata.

use crate::core::geometry::Rect;

/// A process identifier, modeled as a strong type to avoid mixing identities.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ProcessId(pub i32);

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
}
