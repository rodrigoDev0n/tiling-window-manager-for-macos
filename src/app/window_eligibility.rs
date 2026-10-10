//! Pure window eligibility rules (SPEC-005 FR-002).
//!
//! A window is eligible when it is a standard application window that is not
//! minimized or fullscreen, has valid geometry, and belongs to the target
//! display. Windows that cannot be classified reliably are skipped rather than
//! modified.

use crate::core::display::{Display, DisplayId};
use crate::core::display_assignment;
use crate::core::geometry::Rect;
use crate::core::layout_plan::{ExclusionReason, WindowRef};

/// The accessibility role of a standard application window (SPEC-002).
pub const STANDARD_WINDOW_ROLE: &str = "AXWindow";

/// Classifies `window` and returns its validated bounds when eligible.
///
/// The role and minimized attribute must be readable; an unreadable
/// classification is treated as unreliable and the window is excluded.
pub fn assess<W: WindowRef>(
    window: &W,
    displays: &[Display],
    target: DisplayId,
) -> Result<Rect, ExclusionReason> {
    let classification = window.classification();

    match classification.role.as_deref() {
        Some(STANDARD_WINDOW_ROLE) => {}
        Some(_) => return Err(ExclusionReason::NonStandardRole),
        None => return Err(ExclusionReason::Unclassifiable),
    }

    match classification.minimized {
        Some(true) => return Err(ExclusionReason::Minimized),
        None => return Err(ExclusionReason::Unclassifiable),
        Some(false) => {}
    }

    if classification.fullscreen == Some(true) {
        return Err(ExclusionReason::Fullscreen);
    }

    let bounds = window
        .info()
        .bounds
        .ok_or(ExclusionReason::InvalidGeometry)?;
    if !bounds.is_valid() {
        return Err(ExclusionReason::InvalidGeometry);
    }

    match display_assignment::assign(displays, bounds) {
        None => Err(ExclusionReason::NoIntersectingDisplay),
        Some(assigned) if assigned != target => Err(ExclusionReason::NotOnTargetDisplay),
        Some(_) => Ok(bounds),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::window::{ProcessId, WindowClassification, WindowInfo};

    struct FakeWindow {
        info: WindowInfo,
        classification: WindowClassification,
    }

    impl FakeWindow {
        fn new(
            role: Option<&str>,
            minimized: Option<bool>,
            fullscreen: Option<bool>,
            bounds: Option<Rect>,
        ) -> Self {
            Self {
                info: WindowInfo {
                    pid: ProcessId(1),
                    application: "App".to_string(),
                    title: None,
                    bounds,
                },
                classification: WindowClassification {
                    role: role.map(str::to_string),
                    minimized,
                    fullscreen,
                },
            }
        }
    }

    impl WindowRef for FakeWindow {
        fn info(&self) -> &WindowInfo {
            &self.info
        }
        fn classification(&self) -> &WindowClassification {
            &self.classification
        }
    }

    fn displays() -> Vec<Display> {
        vec![Display {
            id: DisplayId(1),
            name: None,
            main: true,
            scale: 2.0,
            bounds: Rect::new(-1920.0, 0.0, 3840.0, 1080.0),
            usable: Rect::new(-1920.0, 25.0, 3840.0, 1055.0),
        }]
    }

    #[test]
    fn standard_window_is_eligible() {
        let window = FakeWindow::new(
            Some("AXWindow"),
            Some(false),
            Some(false),
            Some(Rect::new(0.0, 0.0, 500.0, 500.0)),
        );
        assert_eq!(
            assess(&window, &displays(), DisplayId(1)),
            Ok(Rect::new(0.0, 0.0, 500.0, 500.0))
        );
    }

    #[test]
    fn non_standard_role_is_excluded() {
        let window = FakeWindow::new(
            Some("AXSheet"),
            Some(false),
            Some(false),
            Some(Rect::new(0.0, 0.0, 500.0, 500.0)),
        );
        assert_eq!(
            assess(&window, &displays(), DisplayId(1)),
            Err(ExclusionReason::NonStandardRole)
        );
    }

    #[test]
    fn unreadable_role_is_excluded_as_unclassifiable() {
        let window = FakeWindow::new(
            None,
            Some(false),
            Some(false),
            Some(Rect::new(0.0, 0.0, 500.0, 500.0)),
        );
        assert_eq!(
            assess(&window, &displays(), DisplayId(1)),
            Err(ExclusionReason::Unclassifiable)
        );
    }

    #[test]
    fn minimized_window_is_excluded() {
        let window = FakeWindow::new(
            Some("AXWindow"),
            Some(true),
            Some(false),
            Some(Rect::new(0.0, 0.0, 500.0, 500.0)),
        );
        assert_eq!(
            assess(&window, &displays(), DisplayId(1)),
            Err(ExclusionReason::Minimized)
        );
    }

    #[test]
    fn unreadable_minimized_is_excluded_as_unclassifiable() {
        let window = FakeWindow::new(
            Some("AXWindow"),
            None,
            Some(false),
            Some(Rect::new(0.0, 0.0, 500.0, 500.0)),
        );
        assert_eq!(
            assess(&window, &displays(), DisplayId(1)),
            Err(ExclusionReason::Unclassifiable)
        );
    }

    #[test]
    fn fullscreen_window_is_excluded() {
        let window = FakeWindow::new(
            Some("AXWindow"),
            Some(false),
            Some(true),
            Some(Rect::new(0.0, 0.0, 500.0, 500.0)),
        );
        assert_eq!(
            assess(&window, &displays(), DisplayId(1)),
            Err(ExclusionReason::Fullscreen)
        );
    }

    #[test]
    fn unreadable_fullscreen_is_treated_as_not_fullscreen() {
        let window = FakeWindow::new(
            Some("AXWindow"),
            Some(false),
            None,
            Some(Rect::new(0.0, 0.0, 500.0, 500.0)),
        );
        assert_eq!(
            assess(&window, &displays(), DisplayId(1)),
            Ok(Rect::new(0.0, 0.0, 500.0, 500.0))
        );
    }

    #[test]
    fn missing_geometry_is_excluded() {
        let window = FakeWindow::new(Some("AXWindow"), Some(false), Some(false), None);
        assert_eq!(
            assess(&window, &displays(), DisplayId(1)),
            Err(ExclusionReason::InvalidGeometry)
        );
    }

    #[test]
    fn invalid_geometry_is_excluded() {
        let window = FakeWindow::new(
            Some("AXWindow"),
            Some(false),
            Some(false),
            Some(Rect::new(0.0, 0.0, 0.0, 0.0)),
        );
        assert_eq!(
            assess(&window, &displays(), DisplayId(1)),
            Err(ExclusionReason::InvalidGeometry)
        );
    }

    #[test]
    fn window_off_any_display_is_excluded() {
        let window = FakeWindow::new(
            Some("AXWindow"),
            Some(false),
            Some(false),
            Some(Rect::new(5000.0, 5000.0, 100.0, 100.0)),
        );
        assert_eq!(
            assess(&window, &displays(), DisplayId(1)),
            Err(ExclusionReason::NoIntersectingDisplay)
        );
    }

    #[test]
    fn window_on_another_display_is_excluded() {
        let window = FakeWindow::new(
            Some("AXWindow"),
            Some(false),
            Some(false),
            Some(Rect::new(-1900.0, 100.0, 100.0, 100.0)),
        );
        assert_eq!(
            assess(&window, &displays(), DisplayId(99)),
            Err(ExclusionReason::NotOnTargetDisplay)
        );
    }
}
