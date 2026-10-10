//! Overlay visibility state and placement (SPEC-007).
//!
//! Pure logic: no AppKit or Core Foundation types appear in this module.

use crate::core::geometry::{Point, Rect, Size};

/// Fixed overlay width in points.
pub const OVERLAY_WIDTH: f64 = 460.0;
/// Line height used to size the overlay to its content, in points.
pub const OVERLAY_LINE_HEIGHT: f64 = 20.0;
/// Vertical padding added above and below the shortcut text, in points.
pub const OVERLAY_VERTICAL_PADDING: f64 = 32.0;

/// Visibility state of the shortcut overlay.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct OverlayState {
    visible: bool,
}

impl OverlayState {
    /// Creates the state with the given initial visibility.
    pub const fn new(visible: bool) -> Self {
        Self { visible }
    }

    pub fn is_visible(&self) -> bool {
        self.visible
    }

    /// Toggles visibility and returns the new value.
    pub fn toggle(&mut self) -> bool {
        self.visible = !self.visible;
        self.visible
    }
}

/// Computes the top-left origin of the overlay inside `frame`.
///
/// The overlay is centered horizontally and placed `top_inset` points from the
/// top of the visible frame. The result is clamped so that, when the overlay
/// fits inside `frame`, it never extends outside it. When the overlay is larger
/// than `frame` on an axis, its origin is pinned to the frame origin so the
/// overflow extends past the opposite edge.
///
/// Coordinates follow the macOS top-left origin convention (`frame.origin` is
/// the top-left corner).
pub fn overlay_origin(frame: Rect, size: Size, top_inset: f64) -> Point {
    let min_x = frame.origin.x;
    let min_y = frame.origin.y;
    // Largest origin that still keeps the overlay inside the frame; never less
    // than the frame origin when the overlay is larger than the frame.
    let hi_x = (frame.origin.x + frame.size.width - size.width).max(min_x);
    let hi_y = (frame.origin.y + frame.size.height - size.height).max(min_y);

    let centered_x = frame.origin.x + (frame.size.width - size.width) / 2.0;
    let near_top_y = frame.origin.y + top_inset;

    Point::new(centered_x.clamp(min_x, hi_x), near_top_y.clamp(min_y, hi_y))
}

/// Computes a compact overlay size that fits `content` without scrolling.
///
/// The width is fixed so the key/description columns stay aligned; the height
/// grows with the number of rendered lines.
pub fn overlay_size(content: &str) -> Size {
    let lines = content.lines().count().max(1) as f64;
    Size::new(
        OVERLAY_WIDTH,
        lines * OVERLAY_LINE_HEIGHT + OVERLAY_VERTICAL_PADDING,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn overlay_size_grows_with_line_count() {
        let one = overlay_size("only one line");
        let three = overlay_size("a\nb\nc");
        assert!((one.width - OVERLAY_WIDTH).abs() < f64::EPSILON);
        assert!(three.height > one.height);
        assert!(
            (three.height - (3.0 * OVERLAY_LINE_HEIGHT + OVERLAY_VERTICAL_PADDING)).abs()
                < f64::EPSILON
        );
    }

    #[test]
    fn overlay_size_handles_empty_content() {
        let size = overlay_size("");
        assert!(size.is_valid());
        assert!(
            (size.height - (OVERLAY_LINE_HEIGHT + OVERLAY_VERTICAL_PADDING)).abs() < f64::EPSILON
        );
    }

    #[test]
    fn toggle_flips_visibility() {
        let mut state = OverlayState::new(false);
        assert!(state.toggle());
        assert!(state.is_visible());
        assert!(!state.toggle());
        assert!(!state.is_visible());
    }

    #[test]
    fn toggle_is_idempotent_for_repeated_calls() {
        let mut state = OverlayState::new(true);
        assert!(!state.toggle());
        assert!(state.toggle());
        // A second toggle always flips again; the state machine is stateless.
        assert!(!state.toggle());
    }

    #[test]
    fn is_visible_reflects_toggle_transitions() {
        let mut state = OverlayState::new(false);
        assert!(!state.is_visible());
        state.toggle();
        assert!(state.is_visible());
        state.toggle();
        assert!(!state.is_visible());
    }

    #[test]
    fn origin_centers_horizontally_and_respects_top_inset() {
        let frame = Rect::new(0.0, 25.0, 1440.0, 875.0);
        let origin = overlay_origin(frame, Size::new(400.0, 300.0), 40.0);
        assert!((origin.x - 520.0).abs() < f64::EPSILON);
        assert!((origin.y - 65.0).abs() < f64::EPSILON);
    }

    #[test]
    fn origin_stays_inside_the_frame() {
        let frame = Rect::new(100.0, 100.0, 800.0, 600.0);
        let size = Size::new(200.0, 200.0);
        let origin = overlay_origin(frame, size, 10.0);

        assert!(origin.x >= frame.origin.x);
        assert!(origin.y >= frame.origin.y);
        assert!(origin.x + size.width <= frame.origin.x + frame.size.width);
        assert!(origin.y + size.height <= frame.origin.y + frame.size.height);
    }

    #[test]
    fn oversized_overlay_is_pinned_to_the_frame_origin() {
        let frame = Rect::new(100.0, 100.0, 800.0, 600.0);
        let origin = overlay_origin(frame, Size::new(2000.0, 2000.0), 10.0);
        assert!((origin.x - frame.origin.x).abs() < f64::EPSILON);
        assert!((origin.y - frame.origin.y).abs() < f64::EPSILON);
    }

    #[test]
    fn non_zero_origin_is_handled() {
        let frame = Rect::new(1920.0, 0.0, 1440.0, 900.0);
        let origin = overlay_origin(frame, Size::new(320.0, 200.0), 0.0);
        assert!((origin.x - 2480.0).abs() < f64::EPSILON);
        assert!((origin.y - 0.0).abs() < f64::EPSILON);
    }
}
