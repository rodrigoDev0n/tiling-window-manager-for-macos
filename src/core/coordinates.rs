//! Conversions between AppKit and Accessibility/Core Graphics coordinates.
//!
//! Both Accessibility (`AX*`) and Core Graphics use a global display coordinate
//! space whose origin is the **top-left** corner of the main display, with X
//! increasing to the right and Y increasing **downward**. AppKit uses the same
//! origin at the **bottom-left** of the main display, with Y increasing
//! **upward**. All values are expressed in points (not backing pixels).
//!
//! These functions are pure and platform independent; they are the single place
//! where the vertical flip is defined.

use crate::core::geometry::{Point, Rect};

/// Converts an AppKit rect (bottom-left origin, Y up) to AX coordinates
/// (top-left origin, Y down), given the main display height in points.
pub fn appkit_to_ax(rect: Rect, main_height: f64) -> Option<Rect> {
    if !valid_height(main_height) || !rect.is_valid() {
        return None;
    }
    let y = main_height - (rect.origin.y + rect.size.height);
    let converted = Rect::new(rect.origin.x, y, rect.size.width, rect.size.height);
    converted.is_valid().then_some(converted)
}

/// Converts an AppKit point (bottom-left origin, Y up) to AX coordinates
/// (top-left origin, Y down).
pub fn appkit_point_to_ax(point: Point, main_height: f64) -> Option<Point> {
    if !valid_height(main_height) || !point.is_valid() {
        return None;
    }
    let converted = Point::new(point.x, main_height - point.y);
    converted.is_valid().then_some(converted)
}

/// Converts an AX point (top-left origin, Y down) to AppKit coordinates
/// (bottom-left origin, Y up).
pub fn ax_point_to_appkit(point: Point, main_height: f64) -> Option<Point> {
    appkit_point_to_ax(point, main_height)
}

/// Converts an AX rect (top-left origin, Y down) to AppKit coordinates
/// (bottom-left origin, Y up), given the main display height in points.
///
/// The vertical flip is an involution, so this function is mathematically
/// identical to [`appkit_to_ax`]; the separate name documents which direction
/// the caller intends.
pub fn ax_to_appkit(rect: Rect, main_height: f64) -> Option<Rect> {
    appkit_to_ax(rect, main_height)
}

fn valid_height(main_height: f64) -> bool {
    main_height.is_finite() && main_height > 0.0
}

#[cfg(test)]
mod tests {
    use super::*;

    const MAIN_HEIGHT: f64 = 900.0;

    #[test]
    fn main_display_frame_is_unchanged() {
        let frame = Rect::new(0.0, 0.0, 1440.0, 900.0);
        assert_eq!(appkit_to_ax(frame, MAIN_HEIGHT), Some(frame));
    }

    #[test]
    fn visible_frame_flips_to_top_offset() {
        // AppKit visibleFrame sits at the bottom, leaving 25pt for the menu bar.
        let visible = Rect::new(0.0, 0.0, 1440.0, 875.0);
        let ax = appkit_to_ax(visible, MAIN_HEIGHT);
        assert_eq!(ax, Some(Rect::new(0.0, 25.0, 1440.0, 875.0)));
    }

    #[test]
    fn display_above_main_gets_negative_y() {
        let above = Rect::new(0.0, 900.0, 1440.0, 900.0);
        let ax = appkit_to_ax(above, MAIN_HEIGHT);
        assert_eq!(ax, Some(Rect::new(0.0, -900.0, 1440.0, 900.0)));
    }

    #[test]
    fn display_below_main_gets_positive_y() {
        let below = Rect::new(0.0, -900.0, 1440.0, 900.0);
        let ax = appkit_to_ax(below, MAIN_HEIGHT);
        assert_eq!(ax, Some(Rect::new(0.0, 900.0, 1440.0, 900.0)));
    }

    #[test]
    fn display_to_the_left_gets_negative_x() {
        let left = Rect::new(-1920.0, 0.0, 1920.0, 1080.0);
        let ax = appkit_to_ax(left, MAIN_HEIGHT);
        assert_eq!(ax, Some(Rect::new(-1920.0, -180.0, 1920.0, 1080.0)));
    }

    #[test]
    fn rect_flip_is_an_involution() {
        // The vertical flip is its own inverse, so applying it twice restores
        // the original rect.
        let original = Rect::new(-1920.0, -180.0, 1920.0, 1080.0);
        let once = appkit_to_ax(original, MAIN_HEIGHT).expect("to ax");
        let twice = appkit_to_ax(once, MAIN_HEIGHT);
        assert_eq!(twice, Some(original));
    }

    #[test]
    fn point_round_trip() {
        let original = Point::new(-100.0, 50.0);
        let round_tripped = ax_point_to_appkit(
            appkit_point_to_ax(original, MAIN_HEIGHT).expect("to ax"),
            MAIN_HEIGHT,
        );
        assert_eq!(round_tripped, Some(original));
    }

    #[test]
    fn invalid_height_is_rejected() {
        let rect = Rect::new(0.0, 0.0, 100.0, 100.0);
        assert_eq!(appkit_to_ax(rect, 0.0), None);
        assert_eq!(appkit_to_ax(rect, f64::NAN), None);
    }

    #[test]
    fn invalid_rect_is_rejected() {
        assert_eq!(
            appkit_to_ax(Rect::new(0.0, 0.0, 0.0, 100.0), MAIN_HEIGHT),
            None
        );
    }

    #[test]
    fn ax_to_appkit_round_trips_with_appkit_to_ax() {
        let ax = Rect::new(100.0, 25.0, 800.0, 600.0);
        let appkit = ax_to_appkit(ax, MAIN_HEIGHT).expect("converted");
        assert_eq!(appkit_to_ax(appkit, MAIN_HEIGHT), Some(ax));
        assert_eq!(ax_to_appkit(appkit, MAIN_HEIGHT), Some(ax));
    }

    #[test]
    fn ax_to_appkit_rejects_invalid_input() {
        assert_eq!(ax_to_appkit(Rect::new(0.0, 0.0, 100.0, 100.0), 0.0), None);
    }
}
